//! Tile-based parallel PBF processing for RMDF generation.
//!
//! This module implements a fully parallel tile-based architecture for generating
//! RMDF tiles from OSM PBF files. Each tile is processed independently:
//!
//! 1. Extract PBF data within tile bounds + 500m buffer
//! 2. Build tile-specific AreaGrids for proximity/nogo computation
//! 3. Compute flags for nodes in tile core bounds
//! 4. Build GenerationGraph with correct flags
//! 5. Write RMDF tile file
//!
//! This approach:
//! - Scales to planet.osm.pbf (memory controlled by tile size)
//! - Eliminates intermediate database storage
//! - Processes tiles in parallel using Rayon
//! - Ensures correct border node classification via buffer zones

use anyhow::{Context, Result};
use geo::{CoordsIter, Distance, GeodesicArea, Haversine, HaversineClosestPoint, Point};
use osmpbfreader::{OsmObj, OsmPbfReader};
use rayon::prelude::*;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

use crate::map_data::generation_graph::GenerationGraph;
use crate::map_data::osm::{
    OsmNode, OsmRelation, OsmRelationMember, OsmRelationMemberRole, OsmRelationMemberType, OsmWay,
};
use crate::map_data::proximity::AreaGrid;
use crate::osm_data::pbf_area_reader::PbfAreaReader;
use crate::rmdf::format::TileId;
use std::collections::HashMap;
// TODO: Move this constant somewhere accessible or re-export from osm_data
// use crate::osm_data::data_reader::ALLOWED_HIGHWAY_VALUES;

// Temporarily define locally until we re-organize modules
const ALLOWED_HIGHWAY_VALUES: [&str; 17] = [
    "motorway",
    "trunk",
    "primary",
    "secondary",
    "tertiary",
    "unclassified",
    "residential",
    "motorway_link",
    "trunk_link",
    "primary_link",
    "secondary_link",
    "tertiary_link",
    "living_street",
    "track",
    "escape",
    "raceway",
    "road",
];

// Constants from src/osm_data/pbf_reader.rs
const RESIDENTIAL_PROXIMITY_THRESHOLD_METERS: f64 = 500.0;
const RESIDENTIAL_PART_COVERED: f64 = 0.10;
const THRESHOLD_AREA: f64 = (RESIDENTIAL_PROXIMITY_THRESHOLD_METERS
    * RESIDENTIAL_PROXIMITY_THRESHOLD_METERS
    * std::f64::consts::PI)
    * RESIDENTIAL_PART_COVERED;
const MILITARY_ENTRY_MAX_M: f64 = 100.0;

/// Data extracted from PBF for a single tile
struct TileData {
    tile_id: TileId,
    nodes: HashMap<u64, OsmNode>, // OSM ID -> Node with coordinates
    ways: Vec<OsmWay>,
    relations: Vec<OsmRelation>,
}

impl TileData {
    fn new(tile_id: TileId) -> Self {
        Self {
            tile_id,
            nodes: HashMap::new(),
            ways: Vec::new(),
            relations: Vec::new(),
        }
    }
}

/// Geographic bounds discovered from scanning a PBF file
#[derive(Debug, Clone, Copy)]
pub struct PbfBounds {
    pub lat_min: Option<f64>,
    pub lat_max: Option<f64>,
    pub lon_min: Option<f64>,
    pub lon_max: Option<f64>,
}

impl PbfBounds {
    /// Create empty bounds (no coordinates discovered yet)
    fn new_empty() -> Self {
        Self {
            lat_min: None,
            lat_max: None,
            lon_min: None,
            lon_max: None,
        }
    }

    /// Update bounds with a new coordinate
    fn update(&mut self, lat: f64, lon: f64) {
        self.lat_min = Some(self.lat_min.map_or(lat, |v| v.min(lat)));
        self.lat_max = Some(self.lat_max.map_or(lat, |v| v.max(lat)));
        self.lon_min = Some(self.lon_min.map_or(lon, |v| v.min(lon)));
        self.lon_max = Some(self.lon_max.map_or(lon, |v| v.max(lon)));
    }

    /// Check if all bounds are valid (all fields are Some)
    fn is_valid(&self) -> bool {
        self.lat_min.is_some()
            && self.lat_max.is_some()
            && self.lon_min.is_some()
            && self.lon_max.is_some()
    }

    /// Unwrap bounds to concrete values (must check is_valid() first)
    fn unwrap(&self) -> (f64, f64, f64, f64) {
        (
            self.lat_min.unwrap(),
            self.lat_max.unwrap(),
            self.lon_min.unwrap(),
            self.lon_max.unwrap(),
        )
    }
}

pub struct PbfStreamer {
    input_file: PathBuf,
    output_dir: PathBuf,
    tile_size_degrees: f32,
}

impl PbfStreamer {
    pub fn new(input_file: &Path, output_dir: &Path, tile_size_degrees: f32) -> Result<Self> {
        Ok(Self {
            input_file: input_file.to_path_buf(),
            output_dir: output_dir.to_path_buf(),
            tile_size_degrees,
        })
    }

    /// Process all tiles in parallel using Rayon.
    ///
    /// This is the main entry point for tile-based generation. It:
    /// 1. Calculates all tile boundaries
    /// 2. Processes each tile in parallel
    /// 3. Reports progress during processing
    ///
    /// # Errors
    ///
    /// Returns error if any tile fails to process. Processing stops on first error.
    pub fn partition_parallel(&self) -> Result<()> {
        let start_time = Instant::now();
        info!("Starting tile-based parallel partitioning");

        // Scan PBF to discover geographic bounds
        let pbf_bounds = self.scan_pbf_bounds()
            .context("Failed to scan PBF bounds")?;

        // Calculate tiles that intersect with bounds
        let tiles = self.calculate_all_tiles(pbf_bounds);
        let total_tiles = tiles.len();
        info!("Processing {} tiles in parallel", total_tiles);

        // Progress counter (shared across threads)
        let completed = Arc::new(AtomicUsize::new(0));
        let start_time_clone = start_time;

        // Process tiles in parallel using Rayon
        tiles.par_iter().try_for_each(|tile_id| -> Result<()> {
            self.process_tile(*tile_id)?;

            // Update progress
            let count = completed.fetch_add(1, Ordering::Relaxed) + 1;

            // Report progress every 100 tiles or at completion
            if count % 100 == 0 || count == total_tiles {
                let elapsed = start_time_clone.elapsed();
                let rate = count as f64 / elapsed.as_secs_f64();
                let remaining = if rate > 0.0 {
                    ((total_tiles - count) as f64 / rate) as u64
                } else {
                    0
                };

                info!(
                    "Progress: {}/{} tiles ({:.1}%) - {:.1} tiles/sec - ETA: {}s",
                    count,
                    total_tiles,
                    (count as f64 / total_tiles as f64) * 100.0,
                    rate,
                    remaining
                );
            }

            Ok(())
        })?;

        let total_duration = start_time.elapsed();
        info!(
            "Tile-based partitioning complete - {} tiles in {:.1}s ({:.1} tiles/sec)",
            total_tiles,
            total_duration.as_secs_f64(),
            total_tiles as f64 / total_duration.as_secs_f64()
        );

        Ok(())
    }

    /// Process a single tile
    fn process_tile(&self, tile_id: TileId) -> Result<()> {
        // Wrap entire tile processing in context for better error messages
        (|| -> Result<()> {
            // Step 1: Calculate bounds
            let core_bounds = self.calculate_tile_bounds(tile_id);
            let buffered_bounds = self.add_buffer_to_bounds(core_bounds);

            // Step 2: Extract PBF data for this tile
            let tile_data = self.extract_tile_data(tile_id, buffered_bounds)
                .with_context(|| format!(
                    "Failed to extract PBF data for tile {:?} (bounds: lat={:.3}..{:.3}, lon={:.3}..{:.3})",
                    tile_id, buffered_bounds.lat_min, buffered_bounds.lat_max,
                    buffered_bounds.lon_min, buffered_bounds.lon_max
                ))?;

            // Handle empty tiles (ocean, poles, etc.) - skip writing
            if tile_data.nodes.is_empty() && tile_data.ways.is_empty() {
                info!("Tile {:?} is empty, skipping RMDF write", tile_id);
                return Ok(());
            }

            // Validate tile data (warns but doesn't fail)
            self.validate_tile_data(&tile_data)?;

            // Step 3: Build area grids for proximity computation
            let (residential_grid, military_grid) = self.build_area_grids(&tile_data, buffered_bounds)
                .with_context(|| format!(
                    "Failed to build area grids for tile {:?} ({} nodes, {} ways, {} relations)",
                    tile_id, tile_data.nodes.len(), tile_data.ways.len(), tile_data.relations.len()
                ))?;

            // Step 4: Compute proximity flags for nodes in core bounds
            let nodes_with_flags = self.compute_proximity_flags(
                tile_data.nodes,
                core_bounds,
                &residential_grid,
                &military_grid,
            ).with_context(|| format!(
                "Failed to compute proximity flags for tile {:?}",
                tile_id
            ))?;

            // Step 5: Build generation graph
            let graph = self.build_generation_graph(
                nodes_with_flags,
                tile_data.ways,
                tile_data.relations,
            ).with_context(|| format!(
                "Failed to build generation graph for tile {:?}",
                tile_id
            ))?;

            // Step 6: Write RMDF tile file
            self.write_rmdf_tile(tile_id, graph)
                .with_context(|| format!(
                    "Failed to write RMDF tile {:?} to disk",
                    tile_id
                ))?;

            Ok(())
        })().with_context(|| format!("Failed to process tile {:?}", tile_id))
    }

    /// Compute residential proximity flag (from src/osm_data/pbf_reader.rs:90-126)
    fn compute_residential_proximity(lat: f64, lon: f64, residential_areas: &AreaGrid) -> bool {
        let tot_area = match residential_areas.find_closest_areas_refs(
            lat as f32, lon as f32, 1, // Search 1 grid step (~1.1km)
        ) {
            Some(areas) => areas.iter().fold(0., |tot, multi_polygon| {
                let geo_point = Point::new(lon, lat);
                let distance = match multi_polygon.haversine_closest_point(&geo_point) {
                    geo::Closest::Intersection(_) => 0.,
                    geo::Closest::SinglePoint(p) => Haversine.distance(p, geo_point),
                    geo::Closest::Indeterminate => {
                        multi_polygon.coords_iter().fold(10000., |min, coords| {
                            let dist = Haversine.distance(geo_point, Point::from(coords));
                            if dist < min {
                                dist
                            } else {
                                min
                            }
                        })
                    }
                };

                if distance <= RESIDENTIAL_PROXIMITY_THRESHOLD_METERS {
                    let area = multi_polygon.geodesic_area_signed().abs();
                    return tot + area;
                }
                tot
            }),
            None => 0.,
        };

        tot_area > THRESHOLD_AREA
    }

    /// Compute nogo area flag (from src/osm_data/pbf_reader.rs:128-149)
    fn compute_nogo_area(lat: f64, lon: f64, military_areas: &AreaGrid) -> bool {
        match military_areas.find_closest_areas_refs(lat as f32, lon as f32, 1) {
            None => false,
            Some(areas) => areas.iter().any(|multi_polygon| {
                let geo_point = Point::new(lon, lat);
                match multi_polygon.haversine_closest_point(&geo_point) {
                    geo::Closest::Intersection(p) => {
                        // Only mark as nogo if inside military area >100m from boundary
                        Haversine.distance(geo_point, p) > MILITARY_ENTRY_MAX_M
                    }
                    geo::Closest::SinglePoint(_) => false,
                    geo::Closest::Indeterminate => false,
                }
            }),
        }
    }

    /// Scan PBF file to discover geographic bounds
    ///
    /// This performs a single pass through all nodes in the PBF file to determine
    /// the min/max latitude and longitude. This allows us to generate only tiles
    /// that intersect with the actual data, instead of processing all possible tiles.
    ///
    /// # Performance
    ///
    /// For regional PBF files (e.g., Montenegro 30MB), this scan takes 2-5 seconds
    /// but eliminates checking millions of empty tiles during generation.
    ///
    /// # Errors
    ///
    /// Returns error if PBF contains no valid nodes (empty file).
    fn scan_pbf_bounds(&self) -> Result<PbfBounds> {
        info!("Scanning PBF file to discover geographic bounds...");
        let start_time = Instant::now();

        let mut bounds = PbfBounds::new_empty();
        let mut node_count = 0u64;

        let file = File::open(&self.input_file)
            .context("Failed to open PBF file for bounds scanning")?;
        let mut pbf = OsmPbfReader::new(file);

        for obj_result in pbf.iter() {
            let obj = obj_result.context("Failed to read PBF object during bounds scan")?;

            if let OsmObj::Node(node) = obj {
                let lat = node.lat();
                let lon = node.lon();

                bounds.update(lat, lon);
                node_count += 1;

                // Log progress every 1M nodes
                if node_count % 1_000_000 == 0 {
                    info!("Scanned {} million nodes...", node_count / 1_000_000);
                }
            }
        }

        if !bounds.is_valid() {
            anyhow::bail!("PBF file contains no valid nodes");
        }

        let (lat_min, lat_max, lon_min, lon_max) = bounds.unwrap();
        let elapsed = start_time.elapsed();

        info!(
            "PBF bounds scan complete: scanned {} nodes in {:.1}s",
            node_count,
            elapsed.as_secs_f64()
        );
        info!(
            "Discovered bounds: lat=[{:.3}, {:.3}], lon=[{:.3}, {:.3}]",
            lat_min, lat_max, lon_min, lon_max
        );

        Ok(bounds)
    }

    /// Calculate tiles that intersect with PBF bounds
    ///
    /// Only generates tiles that overlap with the discovered geographic bounds,
    /// plus a buffer to ensure edge tiles are included.
    ///
    /// # Arguments
    ///
    /// * `pbf_bounds` - Geographic bounds discovered from PBF scan
    ///
    /// # Performance Impact
    ///
    /// For Montenegro (0.1° grid): generates ~400 tiles instead of 6.48M tiles
    fn calculate_all_tiles(&self, pbf_bounds: PbfBounds) -> Vec<TileId> {
        let (lat_min, lat_max, lon_min, lon_max) = pbf_bounds.unwrap();

        // Add buffer to ensure edge tiles are included
        // Use 0.01° buffer (2x the per-tile buffer of 0.005°)
        let buffer = 0.01f64;
        let lat_min_buffered = (lat_min - buffer).max(-90.0);
        let lat_max_buffered = (lat_max + buffer).min(90.0);
        let lon_min_buffered = (lon_min - buffer).max(-180.0);
        let lon_max_buffered = (lon_max + buffer).min(180.0);

        // Calculate tile indices for buffered bounds
        // Tile formula: col = floor((lon + 180) / tile_size), row = floor((lat + 90) / tile_size)
        let col_min = ((lon_min_buffered + 180.0) / self.tile_size_degrees as f64).floor() as u16;
        let col_max = ((lon_max_buffered + 180.0) / self.tile_size_degrees as f64).ceil() as u16;
        let row_min = ((lat_min_buffered + 90.0) / self.tile_size_degrees as f64).floor() as u16;
        let row_max = ((lat_max_buffered + 90.0) / self.tile_size_degrees as f64).ceil() as u16;

        let mut tiles = Vec::new();
        for col in col_min..col_max {
            for row in row_min..row_max {
                tiles.push(TileId { col, row });
            }
        }

        // Calculate what world coverage would be for comparison
        let world_cols = (360.0 / self.tile_size_degrees).ceil() as u64;
        let world_rows = (180.0 / self.tile_size_degrees).ceil() as u64;
        let world_total = world_cols * world_rows;

        info!(
            "Generated {} tiles for bounded region (lat=[{:.3}, {:.3}], lon=[{:.3}, {:.3}]) vs {} world tiles ({:.1}% coverage)",
            tiles.len(),
            lat_min_buffered,
            lat_max_buffered,
            lon_min_buffered,
            lon_max_buffered,
            world_total,
            (tiles.len() as f64 / world_total as f64) * 100.0
        );

        tiles
    }

    /// Calculate geographic bounds for a tile
    fn calculate_tile_bounds(&self, tile_id: TileId) -> crate::rmdf::format::TileBounds {
        let lon_min = (tile_id.col as f32 * self.tile_size_degrees) - 180.0;
        let lon_max = lon_min + self.tile_size_degrees;
        let lat_min = (tile_id.row as f32 * self.tile_size_degrees) - 90.0;
        let lat_max = lat_min + self.tile_size_degrees;

        crate::rmdf::format::TileBounds {
            lat_min,
            lat_max,
            lon_min,
            lon_max,
        }
    }

    /// Add buffer zone to tile bounds (500m = RESIDENTIAL_PROXIMITY_THRESHOLD_METERS)
    fn add_buffer_to_bounds(
        &self,
        bounds: crate::rmdf::format::TileBounds,
    ) -> crate::rmdf::format::TileBounds {
        // Convert meters to degrees: 1 degree ≈ 111km at equator
        // 500m / 111,000m ≈ 0.0045 degrees
        // Use slightly larger buffer (0.005) for safety at higher latitudes
        let buffer_degrees = 0.005f32;

        crate::rmdf::format::TileBounds {
            lat_min: (bounds.lat_min - buffer_degrees).max(-90.0),
            lat_max: (bounds.lat_max + buffer_degrees).min(90.0),
            lon_min: (bounds.lon_min - buffer_degrees).max(-180.0),
            lon_max: (bounds.lon_max + buffer_degrees).min(180.0),
        }
    }

    /// Check if a point is within geographic bounds
    fn point_in_bounds(&self, lat: f64, lon: f64, bounds: crate::rmdf::format::TileBounds) -> bool {
        lat >= bounds.lat_min as f64
            && lat < bounds.lat_max as f64
            && lon >= bounds.lon_min as f64
            && lon < bounds.lon_max as f64
    }

    /// Extract nodes, ways, and relations within buffered bounds
    fn extract_tile_data(
        &self,
        tile_id: TileId,
        buffered_bounds: crate::rmdf::format::TileBounds,
    ) -> Result<TileData> {
        let mut tile_data = TileData::new(tile_id);
        let mut nodes_in_bounds = std::collections::HashSet::new();

        // Phase 1: Collect all nodes in buffered bounds
        // File is opened in local scope and automatically closed when dropped
        {
            let file = File::open(&self.input_file)
                .with_context(|| format!("Failed to open PBF file for tile {:?}", tile_id))?;
            let mut pbf = OsmPbfReader::new(file);

            for obj_result in pbf.iter() {
                let obj = obj_result
                    .with_context(|| format!("Failed to read PBF object for tile {:?}", tile_id))?;

                match obj {
                    OsmObj::Node(node) => {
                        let lat = node.lat();
                        let lon = node.lon();

                        if self.point_in_bounds(lat, lon, buffered_bounds) {
                            nodes_in_bounds.insert(node.id.0);

                            let osm_node = OsmNode {
                                id: node.id.0 as u64,
                                lat,
                                lon,
                                residential_in_proximity: false, // Will be computed in Phase 4
                                nogo_area: false,                // Will be computed in Phase 4
                            };

                            tile_data.nodes.insert(osm_node.id, osm_node);
                        }
                    }
                    _ => {} // Collect ways and relations in second pass
                }
            }
        } // File handle dropped here

        // Phase 2: Re-open PBF and collect ways/relations that reference nodes in bounds
        {
            let file = File::open(&self.input_file)
                .with_context(|| format!("Failed to reopen PBF file for tile {:?}", tile_id))?;
            let mut pbf = OsmPbfReader::new(file);

            for obj_result in pbf.iter() {
                let obj = obj_result?;

                match obj {
                    OsmObj::Way(way) => {
                        // Filter highways only (matching current behavior)
                        let has_highway = way.tags.iter().any(|(k, v)| {
                            k == "highway"
                                && (ALLOWED_HIGHWAY_VALUES.contains(&v.as_str())
                                    || (v == "path"
                                        && way
                                            .tags
                                            .iter()
                                            .any(|(k2, v2)| k2 == "motorcycle" && v2 == "yes")))
                        });

                        if !has_highway {
                            continue;
                        }

                        // Include way if any node is in bounds
                        let has_node_in_bounds = way
                            .nodes
                            .iter()
                            .any(|node_id| nodes_in_bounds.contains(&node_id.0));

                        if has_node_in_bounds {
                            let osm_way = OsmWay {
                                id: way.id.0 as u64,
                                point_ids: way.nodes.iter().map(|n| n.0 as u64).collect(),
                                tags: Some(
                                    way.tags
                                        .iter()
                                        .map(|(k, v)| (k.to_string(), v.to_string()))
                                        .collect(),
                                ),
                            };
                            tile_data.ways.push(osm_way);
                        }
                    }
                    OsmObj::Relation(relation) => {
                        // Filter restriction relations only (matching current behavior)
                        let is_restriction = relation
                            .tags
                            .iter()
                            .any(|(k, v)| k == "type" && v.starts_with("restriction"));

                        if !is_restriction {
                            continue;
                        }

                        // Include relation if any member node is in bounds
                        let has_member_in_bounds = relation.refs.iter().any(|r| {
                            if let osmpbfreader::OsmId::Node(node_id) = r.member {
                                nodes_in_bounds.contains(&node_id.0)
                            } else {
                                false
                            }
                        });

                        if has_member_in_bounds {
                            let osm_relation = OsmRelation {
                                id: relation.id.0 as u64,
                                members: relation
                                    .refs
                                    .iter()
                                    .filter_map(|r| {
                                        let role = match r.role.as_str() {
                                            "from" => OsmRelationMemberRole::From,
                                            "to" => OsmRelationMemberRole::To,
                                            "via" => OsmRelationMemberRole::Via,
                                            _ => return None,
                                        };

                                        let (member_ref, member_type) = match r.member {
                                            osmpbfreader::OsmId::Way(id) => {
                                                (id.0 as u64, OsmRelationMemberType::Way)
                                            }
                                            osmpbfreader::OsmId::Node(id) => {
                                                (id.0 as u64, OsmRelationMemberType::Node)
                                            }
                                            osmpbfreader::OsmId::Relation(_) => return None, // Skip nested relations
                                        };

                                        Some(OsmRelationMember {
                                            member_ref,
                                            role,
                                            member_type,
                                        })
                                    })
                                    .collect(),
                                tags: relation
                                    .tags
                                    .iter()
                                    .map(|(k, v)| (k.to_string(), v.to_string()))
                                    .collect(),
                            };
                            tile_data.relations.push(osm_relation);
                        }
                    }
                    _ => {} // Nodes already collected in first pass
                }
            }
        } // File handle dropped here

        // Log warning for empty tiles (common for ocean tiles)
        if tile_data.nodes.is_empty() && tile_data.ways.is_empty() {
            info!(
                "Tile {:?} is empty (likely ocean or unpopulated area)",
                tile_id
            );
        } else {
            info!(
                "Extracted tile {:?}: {} nodes, {} ways, {} relations",
                tile_id,
                tile_data.nodes.len(),
                tile_data.ways.len(),
                tile_data.relations.len()
            );
        }

        Ok(tile_data)
    }

    /// Build residential and military AreaGrids from tile data
    fn build_area_grids(
        &self,
        tile_data: &TileData,
        buffered_bounds: crate::rmdf::format::TileBounds,
    ) -> Result<(AreaGrid, AreaGrid)> {
        // Try to extract residential areas
        let residential_grid =
            self.extract_residential_areas_for_bounds(buffered_bounds)
                .unwrap_or_else(|e| {
                    tracing::warn!(
                    "Failed to extract residential areas for tile {:?}: {:?}. Using empty grid.",
                    tile_data.tile_id, e
                );
                    AreaGrid::new()
                });

        // Try to extract military areas
        let military_grid = self
            .extract_military_areas_for_bounds(buffered_bounds)
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "Failed to extract military areas for tile {:?}: {:?}. Using empty grid.",
                    tile_data.tile_id,
                    e
                );
                AreaGrid::new()
            });

        info!(
            "Built area grids for tile {:?}: {} residential cells, {} military cells",
            tile_data.tile_id,
            residential_grid.len(),
            military_grid.len()
        );

        Ok((residential_grid, military_grid))
    }

    /// Extract residential areas within buffered bounds
    fn extract_residential_areas_for_bounds(
        &self,
        bounds: crate::rmdf::format::TileBounds,
    ) -> Result<AreaGrid> {
        let file = File::open(&self.input_file)
            .context("Failed to open PBF file for residential areas")?;
        let mut pbf = OsmPbfReader::new(file);

        let mut boundary_reader = PbfAreaReader::new(&mut pbf);

        // Read all residential areas (landuse=residential)
        boundary_reader.read(&|obj| {
            (obj.is_way() || obj.is_relation()) && obj.tags().contains("landuse", "residential")
        })?;

        // Filter to only areas within or intersecting buffered bounds
        let area_grid = self.filter_area_grid_by_bounds(boundary_reader.get_area_grid(), bounds)?;

        Ok(area_grid)
    }

    /// Extract military areas within buffered bounds
    fn extract_military_areas_for_bounds(
        &self,
        bounds: crate::rmdf::format::TileBounds,
    ) -> Result<AreaGrid> {
        let file =
            File::open(&self.input_file).context("Failed to open PBF file for military areas")?;
        let mut pbf = OsmPbfReader::new(file);

        let mut boundary_reader = PbfAreaReader::new(&mut pbf);

        // Read all military areas (landuse=military)
        boundary_reader.read(&|obj| {
            (obj.is_way() || obj.is_relation()) && obj.tags().contains("landuse", "military")
        })?;

        // Filter to only areas within or intersecting buffered bounds
        let area_grid = self.filter_area_grid_by_bounds(boundary_reader.get_area_grid(), bounds)?;

        Ok(area_grid)
    }

    /// Filter AreaGrid to only include polygons intersecting bounds
    fn filter_area_grid_by_bounds(
        &self,
        full_grid: AreaGrid,
        _bounds: crate::rmdf::format::TileBounds,
    ) -> Result<AreaGrid> {
        // Note: Current implementation of AreaGrid doesn't support filtering
        // For now, return the full grid (inefficient but correct)
        // TODO: Implement grid filtering if memory becomes an issue

        // The grid will be dropped after tile processing, so memory impact is temporary
        Ok(full_grid)
    }

    /// Compute proximity and nogo flags for nodes in core bounds
    fn compute_proximity_flags(
        &self,
        mut nodes: HashMap<u64, OsmNode>,
        core_bounds: crate::rmdf::format::TileBounds,
        residential_grid: &AreaGrid,
        military_grid: &AreaGrid,
    ) -> Result<HashMap<u64, OsmNode>> {
        let mut computed_count = 0;

        for (_node_id, node) in nodes.iter_mut() {
            // Only compute for nodes in core bounds (not buffer zone)
            if !self.point_in_bounds(node.lat, node.lon, core_bounds) {
                continue;
            }

            // Compute residential proximity flag
            node.residential_in_proximity =
                Self::compute_residential_proximity(node.lat, node.lon, residential_grid);

            // Compute nogo area flag
            node.nogo_area = Self::compute_nogo_area(node.lat, node.lon, military_grid);

            computed_count += 1;
        }

        info!(
            "Computed proximity flags for {} nodes in core bounds",
            computed_count
        );

        Ok(nodes)
    }

    /// Build GenerationGraph from tile data with proximity flags
    fn build_generation_graph(
        &self,
        nodes: HashMap<u64, OsmNode>,
        ways: Vec<OsmWay>,
        relations: Vec<OsmRelation>,
    ) -> Result<GenerationGraph> {
        info!(
            "Building generation graph from {} nodes, {} ways, {} relations",
            nodes.len(),
            ways.len(),
            relations.len()
        );

        let mut graph = GenerationGraph::new();

        // Insert all nodes (with correct proximity flags)
        for (_node_id, node) in nodes {
            graph.insert_node(node);
        }

        // Insert all ways
        for way in ways {
            graph
                .insert_way(way)
                .context("Failed to insert way into generation graph")?;
        }

        // Insert all relations (turn restrictions)
        for relation in relations {
            graph
                .insert_relation(relation)
                .context("Failed to insert relation into generation graph")?;
        }

        // Generate point hashes for spatial indexing
        graph.generate_point_hashes();

        Ok(graph)
    }

    /// Write RMDF tile file from generation graph
    fn write_rmdf_tile(&self, tile_id: TileId, graph: GenerationGraph) -> Result<()> {
        use crate::rmdf::generator::writer::RmdfWriter;

        let output_path = self.output_dir.join(tile_id.to_filename());
        info!("Writing RMDF tile to {:?}", output_path);

        // Create RmdfWriter (reusing existing implementation)
        let writer = RmdfWriter::new(self.tile_size_degrees);

        // Write tile directly from GenerationGraph
        writer
            .write_tile_from_graph(tile_id, graph, &output_path)
            .with_context(|| format!("Failed to write RMDF tile {:?}", tile_id))?;

        Ok(())
    }

    /// Validate tile data for consistency (warns but doesn't fail)
    fn validate_tile_data(&self, tile_data: &TileData) -> Result<()> {
        use tracing::warn;

        // Check for orphaned ways (ways referencing nodes not in tile)
        let node_ids: std::collections::HashSet<u64> = tile_data.nodes.keys().copied().collect();

        for way in &tile_data.ways {
            let missing_nodes: Vec<u64> = way
                .point_ids
                .iter()
                .filter(|id| !node_ids.contains(id))
                .copied()
                .collect();

            if !missing_nodes.is_empty() {
                warn!(
                    "Way {} references {} nodes not in tile {:?}: {:?}",
                    way.id,
                    missing_nodes.len(),
                    tile_data.tile_id,
                    &missing_nodes[..missing_nodes.len().min(5)] // Show first 5
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rmdf::format::TileBounds;

    #[test]
    fn test_pbf_bounds_new_empty() {
        let bounds = PbfBounds::new_empty();
        assert!(bounds.lat_min.is_none());
        assert!(bounds.lat_max.is_none());
        assert!(bounds.lon_min.is_none());
        assert!(bounds.lon_max.is_none());
        assert!(!bounds.is_valid());
    }

    #[test]
    fn test_pbf_bounds_update() {
        let mut bounds = PbfBounds::new_empty();

        // First update
        bounds.update(50.0, 10.0);
        assert_eq!(bounds.lat_min, Some(50.0));
        assert_eq!(bounds.lat_max, Some(50.0));
        assert_eq!(bounds.lon_min, Some(10.0));
        assert_eq!(bounds.lon_max, Some(10.0));

        // Second update (expand bounds)
        bounds.update(51.0, 11.0);
        assert_eq!(bounds.lat_min, Some(50.0));
        assert_eq!(bounds.lat_max, Some(51.0));
        assert_eq!(bounds.lon_min, Some(10.0));
        assert_eq!(bounds.lon_max, Some(11.0));

        // Third update (within existing bounds)
        bounds.update(50.5, 10.5);
        assert_eq!(bounds.lat_min, Some(50.0));
        assert_eq!(bounds.lat_max, Some(51.0));
        assert_eq!(bounds.lon_min, Some(10.0));
        assert_eq!(bounds.lon_max, Some(11.0));

        // Fourth update (expand in opposite direction)
        bounds.update(49.0, 9.0);
        assert_eq!(bounds.lat_min, Some(49.0));
        assert_eq!(bounds.lat_max, Some(51.0));
        assert_eq!(bounds.lon_min, Some(9.0));
        assert_eq!(bounds.lon_max, Some(11.0));
    }

    #[test]
    fn test_pbf_bounds_is_valid() {
        let mut bounds = PbfBounds::new_empty();
        assert!(!bounds.is_valid());

        bounds.update(50.0, 10.0);
        assert!(bounds.is_valid());
    }

    #[test]
    fn test_pbf_bounds_unwrap() {
        let mut bounds = PbfBounds::new_empty();
        bounds.update(50.0, 10.0);
        bounds.update(51.0, 11.0);

        let (lat_min, lat_max, lon_min, lon_max) = bounds.unwrap();
        assert_eq!(lat_min, 50.0);
        assert_eq!(lat_max, 51.0);
        assert_eq!(lon_min, 10.0);
        assert_eq!(lon_max, 11.0);
    }

    #[test]
    fn test_buffer_zone_calculation() {
        let streamer = PbfStreamer {
            input_file: PathBuf::from("dummy.pbf"),
            output_dir: PathBuf::from("output"),
            tile_size_degrees: 1.0,
        };

        let core_bounds = TileBounds {
            lat_min: 50.0,
            lat_max: 51.0,
            lon_min: 10.0,
            lon_max: 11.0,
        };

        let buffered = streamer.add_buffer_to_bounds(core_bounds);

        // Buffer should be ~0.005 degrees on all sides
        assert!((buffered.lat_min - 49.995).abs() < 0.001);
        assert!((buffered.lat_max - 51.005).abs() < 0.001);
        assert!((buffered.lon_min - 9.995).abs() < 0.001);
        assert!((buffered.lon_max - 11.005).abs() < 0.001);
    }

    #[test]
    fn test_buffer_zone_at_poles() {
        let streamer = PbfStreamer {
            input_file: PathBuf::from("dummy.pbf"),
            output_dir: PathBuf::from("output"),
            tile_size_degrees: 1.0,
        };

        // Test at north pole
        let north_pole_bounds = TileBounds {
            lat_min: 89.0,
            lat_max: 90.0,
            lon_min: 0.0,
            lon_max: 1.0,
        };

        let buffered = streamer.add_buffer_to_bounds(north_pole_bounds);

        // Should clamp to 90.0, not exceed it
        assert_eq!(buffered.lat_max, 90.0);
        assert!((buffered.lat_min - 88.995).abs() < 0.001);

        // Test at south pole
        let south_pole_bounds = TileBounds {
            lat_min: -90.0,
            lat_max: -89.0,
            lon_min: 0.0,
            lon_max: 1.0,
        };

        let buffered = streamer.add_buffer_to_bounds(south_pole_bounds);

        // Should clamp to -90.0, not exceed it
        assert_eq!(buffered.lat_min, -90.0);
        assert!((buffered.lat_max - -88.995).abs() < 0.001);
    }

    #[test]
    fn test_buffer_zone_at_dateline() {
        let streamer = PbfStreamer {
            input_file: PathBuf::from("dummy.pbf"),
            output_dir: PathBuf::from("output"),
            tile_size_degrees: 1.0,
        };

        // Test at date line (east)
        let dateline_east = TileBounds {
            lat_min: 0.0,
            lat_max: 1.0,
            lon_min: 179.0,
            lon_max: 180.0,
        };

        let buffered = streamer.add_buffer_to_bounds(dateline_east);

        // Should clamp to 180.0, not exceed it
        assert_eq!(buffered.lon_max, 180.0);
        assert!((buffered.lon_min - 178.995).abs() < 0.001);

        // Test at date line (west)
        let dateline_west = TileBounds {
            lat_min: 0.0,
            lat_max: 1.0,
            lon_min: -180.0,
            lon_max: -179.0,
        };

        let buffered = streamer.add_buffer_to_bounds(dateline_west);

        // Should clamp to -180.0, not exceed it
        assert_eq!(buffered.lon_min, -180.0);
        assert!((buffered.lon_max - -178.995).abs() < 0.001);
    }

    #[test]
    fn test_tile_boundary_calculation() {
        let streamer = PbfStreamer {
            input_file: PathBuf::from("dummy.pbf"),
            output_dir: PathBuf::from("output"),
            tile_size_degrees: 1.0,
        };

        // Test tile at origin
        let tile_0_0 = streamer.calculate_tile_bounds(TileId { col: 0, row: 0 });
        assert_eq!(tile_0_0.lon_min, -180.0);
        assert_eq!(tile_0_0.lon_max, -179.0);
        assert_eq!(tile_0_0.lat_min, -90.0);
        assert_eq!(tile_0_0.lat_max, -89.0);

        // Test tile at known location (Riga, Latvia: ~56.95°N, 24.1°E)
        // Should be tile col=204, row=146 (for 1.0 degree tiles)
        let riga_tile = streamer.calculate_tile_bounds(TileId { col: 204, row: 146 });
        assert!((riga_tile.lon_min - 24.0).abs() < 0.001);
        assert!((riga_tile.lon_max - 25.0).abs() < 0.001);
        assert!((riga_tile.lat_min - 56.0).abs() < 0.001);
        assert!((riga_tile.lat_max - 57.0).abs() < 0.001);
    }

    #[test]
    fn test_calculate_all_tiles_world_bounds() {
        let streamer = PbfStreamer {
            input_file: PathBuf::from("dummy.pbf"),
            output_dir: PathBuf::from("output"),
            tile_size_degrees: 1.0,
        };

        // World-spanning bounds (entire planet)
        let world_bounds = PbfBounds {
            lat_min: Some(-90.0),
            lat_max: Some(90.0),
            lon_min: Some(-180.0),
            lon_max: Some(180.0),
        };

        let tiles = streamer.calculate_all_tiles(world_bounds);

        // With 1.0 degree tiles: 360 cols × 180 rows = 64,800 tiles
        assert_eq!(tiles.len(), 64_800);

        // Verify first and last tiles
        assert_eq!(tiles[0], TileId { col: 0, row: 0 });
        assert_eq!(tiles[tiles.len() - 1], TileId { col: 359, row: 179 });
    }

    #[test]
    fn test_calculate_all_tiles_regional_bounds() {
        let streamer = PbfStreamer {
            input_file: PathBuf::from("dummy.pbf"),
            output_dir: PathBuf::from("output"),
            tile_size_degrees: 0.1,
        };

        // Montenegro-like bounds (roughly 42-43.5°N, 18.5-20.5°E)
        let montenegro_bounds = PbfBounds {
            lat_min: Some(42.0),
            lat_max: Some(43.5),
            lon_min: Some(18.5),
            lon_max: Some(20.5),
        };

        let tiles = streamer.calculate_all_tiles(montenegro_bounds);

        // Should generate significantly fewer tiles than world coverage
        // World coverage with 0.1° tiles: 3600 × 1800 = 6,480,000 tiles
        // Regional coverage: roughly (20.5-18.5+0.02)/0.1 × (43.5-42.0+0.02)/0.1 ≈ 21 × 16 ≈ 336 tiles
        assert!(tiles.len() < 1000, "Expected < 1000 tiles, got {}", tiles.len());
        assert!(tiles.len() > 100, "Expected > 100 tiles, got {}", tiles.len());
    }

    #[test]
    fn test_calculate_all_tiles_single_point() {
        let streamer = PbfStreamer {
            input_file: PathBuf::from("dummy.pbf"),
            output_dir: PathBuf::from("output"),
            tile_size_degrees: 1.0,
        };

        // Single point (with buffer, should generate one tile)
        let single_point = PbfBounds {
            lat_min: Some(50.5),
            lat_max: Some(50.5),
            lon_min: Some(10.5),
            lon_max: Some(10.5),
        };

        let tiles = streamer.calculate_all_tiles(single_point);

        // Should generate exactly 1 tile
        assert_eq!(tiles.len(), 1);
    }

    #[test]
    fn test_point_in_bounds() {
        let streamer = PbfStreamer {
            input_file: PathBuf::from("dummy.pbf"),
            output_dir: PathBuf::from("output"),
            tile_size_degrees: 1.0,
        };

        let bounds = TileBounds {
            lat_min: 50.0,
            lat_max: 51.0,
            lon_min: 10.0,
            lon_max: 11.0,
        };

        // Inside bounds
        assert!(streamer.point_in_bounds(50.5, 10.5, bounds));

        // On min edge (inclusive)
        assert!(streamer.point_in_bounds(50.0, 10.0, bounds));

        // On max edge (exclusive)
        assert!(!streamer.point_in_bounds(51.0, 11.0, bounds));

        // Outside bounds
        assert!(!streamer.point_in_bounds(49.0, 10.5, bounds));
        assert!(!streamer.point_in_bounds(50.5, 9.0, bounds));
        assert!(!streamer.point_in_bounds(52.0, 10.5, bounds));
        assert!(!streamer.point_in_bounds(50.5, 12.0, bounds));
    }
}
