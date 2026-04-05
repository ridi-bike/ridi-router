pub mod intermediate;
pub mod manifest;
mod pbf_streamer;
mod writer;

pub use manifest::ManifestGenerator;
pub use pbf_streamer::PbfStreamer;

use crate::generation::GenerationGraph;
use crate::osm_data::{in_memory_pbf::InMemoryPbf, OsmNode, OsmRelationMemberType};
use crate::proximity::RasterizedProximityGrid;
use crate::rmdf::format::{TileBounds, TileId};
use anyhow::{Context, Result};
use geo::MultiPolygon;
#[cfg(feature = "debug-polygons")]
use geo::{Intersects, LineString, Polygon};
use intermediate::GridStorage;
use rayon::prelude::*;
#[cfg(feature = "debug-polygons")]
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

pub struct TileGenerator {
    input_file: PathBuf,
    output_dir: PathBuf,
    tile_size_degrees: f32,
}

#[cfg(feature = "debug-polygons")]
fn extract_debug_military_polygons(pbf_data: &InMemoryPbf) -> Vec<MultiPolygon<f64>> {
    pbf_data.extract_military_polygons()
}

#[cfg(not(feature = "debug-polygons"))]
fn extract_debug_military_polygons(_pbf_data: &InMemoryPbf) -> Vec<MultiPolygon<f64>> {
    Vec::new()
}

#[cfg(feature = "debug-polygons")]
fn maybe_export_military_geojson_tiles(
    output_dir: &Path,
    tile_size_degrees: f32,
    tile_ids: &[TileId],
    military_polygons: &[MultiPolygon<f64>],
) -> Result<()> {
    export_military_geojson_tiles(output_dir, tile_size_degrees, tile_ids, military_polygons)
}

#[cfg(not(feature = "debug-polygons"))]
fn maybe_export_military_geojson_tiles(
    _output_dir: &Path,
    _tile_size_degrees: f32,
    _tile_ids: &[TileId],
    _military_polygons: &[MultiPolygon<f64>],
) -> Result<()> {
    Ok(())
}

impl TileGenerator {
    pub fn new(input_file: PathBuf, output_dir: PathBuf, tile_size_degrees: f32) -> Result<Self> {
        // Validate tile size
        if tile_size_degrees <= 0.0 || tile_size_degrees > 180.0 {
            anyhow::bail!("Tile size must be between 0 and 180 degrees");
        }

        // Create output directory
        std::fs::create_dir_all(&output_dir)?;

        Ok(Self {
            input_file,
            output_dir,
            tile_size_degrees,
        })
    }

    pub fn generate(&self) -> Result<()> {
        info!("Starting RMDF tile generation");

        // Build in-memory PBF representation with pre-computed proximity flags
        // This uses SIMD-accelerated rasterized grid for O(1) flag lookups
        info!("Loading PBF file into memory with pre-computed proximity flags...");
        let pbf_data = InMemoryPbf::from_pbf_file_with_flags(&self.input_file)
            .context("Failed to load PBF file into memory with flags")?;
        let military_polygons = extract_debug_military_polygons(&pbf_data);

        // Create PBF streamer with reference to in-memory data
        let streamer = PbfStreamer::new(&pbf_data, &self.output_dir, self.tile_size_degrees);

        // Process all tiles in parallel (queries in-memory data)
        streamer.partition_parallel()?;

        // Generate manifest by discovering tiles from filesystem
        info!("Generating manifest");
        let manifest_gen = ManifestGenerator::new(self.tile_size_degrees);
        let tile_ids = manifest_gen
            .discover_tiles(&self.output_dir)
            .context("Failed to discover tiles from filesystem")?;

        if tile_ids.is_empty() {
            tracing::warn!("No tiles were generated - all tiles may have been empty");
        } else {
            maybe_export_military_geojson_tiles(
                &self.output_dir,
                self.tile_size_degrees,
                &tile_ids,
                &military_polygons,
            )
            .context("Failed to export military GeoJSON tiles")?;

            manifest_gen
                .generate(
                    &self.output_dir,
                    &tile_ids,
                    self.input_file.to_str().unwrap_or("unknown"),
                )
                .context("Failed to generate manifest")?;
        }

        info!("RMDF generation complete");
        Ok(())
    }
}

/// Discover all PBF files in a directory
fn discover_pbf_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut pbf_files = Vec::new();

    if !directory.exists() {
        anyhow::bail!("Directory does not exist: {:?}", directory);
    }

    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();

        // Check if it's a .pbf or .osm.pbf file
        if let Some(extension) = path.extension() {
            if extension == "pbf" {
                pbf_files.push(path);
            }
        }
    }

    // Sort for deterministic processing order
    pbf_files.sort();

    info!(
        "Discovered {} PBF files in {:?}",
        pbf_files.len(),
        directory
    );
    Ok(pbf_files)
}

#[cfg(feature = "debug-polygons")]
fn compute_tile_bounds(tile_id: TileId, tile_size_degrees: f32) -> TileBounds {
    let lon_min = (tile_id.col as f32 * tile_size_degrees) - 180.0;
    let lon_max = lon_min + tile_size_degrees;
    let lat_min = (tile_id.row as f32 * tile_size_degrees) - 90.0;
    let lat_max = lat_min + tile_size_degrees;

    TileBounds {
        lat_min,
        lat_max,
        lon_min,
        lon_max,
    }
}

#[cfg(feature = "debug-polygons")]
fn tile_bounds_polygon(bounds: TileBounds) -> Polygon<f64> {
    Polygon::new(
        LineString::from(vec![
            (bounds.lon_min as f64, bounds.lat_min as f64),
            (bounds.lon_max as f64, bounds.lat_min as f64),
            (bounds.lon_max as f64, bounds.lat_max as f64),
            (bounds.lon_min as f64, bounds.lat_max as f64),
            (bounds.lon_min as f64, bounds.lat_min as f64),
        ]),
        vec![],
    )
}

#[cfg(feature = "debug-polygons")]
fn multi_polygon_to_geojson_coordinates(
    multi_polygon: &MultiPolygon<f64>,
) -> Vec<Vec<Vec<Vec<f64>>>> {
    multi_polygon
        .0
        .iter()
        .map(|polygon| {
            std::iter::once(polygon.exterior())
                .chain(polygon.interiors().iter())
                .map(|ring| ring.0.iter().map(|coord| vec![coord.x, coord.y]).collect())
                .collect()
        })
        .collect()
}

#[cfg(feature = "debug-polygons")]
fn export_military_geojson_tiles(
    output_dir: &Path,
    tile_size_degrees: f32,
    tile_ids: &[TileId],
    military_polygons: &[MultiPolygon<f64>],
) -> Result<()> {
    let mut written_geojson_tiles = 0usize;

    for tile_id in tile_ids {
        let tile_bounds = compute_tile_bounds(*tile_id, tile_size_degrees);
        let tile_polygon = tile_bounds_polygon(tile_bounds);
        let geojson_filename = manifest::military_geojson_filename(*tile_id);
        let geojson_path = output_dir.join(&geojson_filename);

        let features: Vec<_> = military_polygons
            .iter()
            .enumerate()
            .filter(|(_, polygon)| polygon.intersects(&tile_polygon))
            .map(|(polygon_index, polygon)| {
                json!({
                    "type": "Feature",
                    "properties": {
                        "polygon_index": polygon_index,
                        "source": "military"
                    },
                    "geometry": {
                        "type": "MultiPolygon",
                        "coordinates": multi_polygon_to_geojson_coordinates(polygon)
                    }
                })
            })
            .collect();

        if features.is_empty() {
            if geojson_path.exists() {
                std::fs::remove_file(&geojson_path)?;
            }
            continue;
        }

        let feature_collection = json!({
            "type": "FeatureCollection",
            "features": features,
        });

        let file = std::fs::File::create(&geojson_path)
            .with_context(|| format!("Failed to create GeoJSON file: {:?}", geojson_path))?;
        serde_json::to_writer_pretty(file, &feature_collection)
            .with_context(|| format!("Failed to write GeoJSON file: {:?}", geojson_path))?;
        written_geojson_tiles += 1;
    }

    info!(
        "Exported military GeoJSON for {} tiles from {} polygons",
        written_geojson_tiles,
        military_polygons.len()
    );

    Ok(())
}

/// Generator for processing multiple PBF files with tile stitching
pub struct MultiPbfGenerator {
    input_directory: PathBuf,
    output_dir: PathBuf,
    tile_size_degrees: f32,
    db_path: PathBuf,
}

impl MultiPbfGenerator {
    /// Create a new multi-PBF generator
    pub fn new(
        input_directory: PathBuf,
        output_dir: PathBuf,
        tile_size_degrees: f32,
        db_path: PathBuf,
    ) -> Result<Self> {
        if tile_size_degrees <= 0.0 || tile_size_degrees > 180.0 {
            anyhow::bail!("Tile size must be between 0 and 180 degrees");
        }

        std::fs::create_dir_all(&output_dir)?;
        std::fs::create_dir_all(db_path.parent().unwrap_or(Path::new(".")))?;

        Ok(Self {
            input_directory,
            output_dir,
            tile_size_degrees,
            db_path,
        })
    }

    /// Generate tiles from all PBF files in the input directory
    ///
    /// This implements the multi-PBF processing pipeline:
    /// 1. Process each PBF independently (build grid, evaluate nodes)
    /// 2. Store grids and flagged nodes to redb
    /// 3. Identify overlap zones
    /// 4. Re-evaluate nodes in overlap zones with combined grids
    /// 5. Deduplicate and write final RMDF tiles
    pub fn generate(&self) -> Result<()> {
        info!(
            "Starting multi-PBF tile generation from {:?}",
            self.input_directory
        );

        // Discover all PBF files
        let pbf_files = discover_pbf_files(&self.input_directory)?;
        if pbf_files.is_empty() {
            anyhow::bail!("No PBF files found in {:?}", self.input_directory);
        }

        // Open or create the redb database
        let db =
            redb::Database::create(&self.db_path).context("Failed to create/open redb database")?;

        // Phase 1: Process each PBF file independently
        info!(
            "Phase 1: Processing {} PBF files independently",
            pbf_files.len()
        );
        let mut all_military_polygons = Vec::new();
        for (pbf_id, pbf_path) in pbf_files.iter().enumerate() {
            let military_polygons = self.process_single_pbf(&db, pbf_id as u64, pbf_path)?;
            all_military_polygons.extend(military_polygons);
        }

        // Phase 2: Identify overlapping regions and affected zones
        info!("Phase 2: Identifying overlap zones");
        let overlap_zones = self.identify_overlap_zones(&db)?;
        info!("Found {} overlap zones", overlap_zones.len());

        // Phase 3: Re-evaluate nodes in overlap zones with combined grids
        if !overlap_zones.is_empty() {
            info!("Phase 3: Re-evaluating nodes in overlap zones");
            self.reevaluate_overlap_zones(&db, &overlap_zones)?;
        }

        // Phase 4: Deduplicate and write final tiles
        info!("Phase 4: Writing final RMDF tiles");
        self.write_final_tiles(&db, &pbf_files, &all_military_polygons)?;

        info!("Multi-PBF tile generation complete");
        Ok(())
    }

    /// Process a single PBF file and store results in redb
    fn process_single_pbf(
        &self,
        db: &redb::Database,
        pbf_id: u64,
        pbf_path: &Path,
    ) -> Result<Vec<MultiPolygon<f64>>> {
        info!("Processing PBF {} ({:?})", pbf_id, pbf_path);

        // Load PBF with pre-computed flags AND get the proximity grid
        let (pbf_data, proximity_grid) = InMemoryPbf::from_pbf_file_with_grid(pbf_path)
            .with_context(|| format!("Failed to load PBF file: {:?}", pbf_path))?;
        let military_polygons = extract_debug_military_polygons(&pbf_data);

        // Store grid bounds
        let bounds = &pbf_data.bounds;
        let grid_bounds = intermediate::GridBounds {
            lon_min: bounds.lon_min.unwrap_or(0.0) as f32,
            lat_min: bounds.lat_min.unwrap_or(0.0) as f32,
            lon_max: bounds.lon_max.unwrap_or(0.0) as f32,
            lat_max: bounds.lat_max.unwrap_or(0.0) as f32,
        };

        // Store grid bytes (actual grid, not placeholder)
        let grid_bytes = proximity_grid.to_bytes();
        GridStorage::store_grid(
            db,
            pbf_id,
            pbf_path.to_str().unwrap_or(""),
            &grid_bytes,
            &grid_bounds,
        )?;

        // Save tiles to redb (intermediate storage for multi-PBF processing)
        self.save_tiles_to_redb(db, &pbf_data)?;

        info!("Completed processing PBF {}", pbf_id);
        Ok(military_polygons)
    }

    /// Save tiles from PBF data to redb for intermediate storage
    ///
    /// This method extracts tile data and saves it to redb instead of writing RMDF files.
    /// The data can then be merged, deduplicated, and written as final RMDF tiles later.
    fn save_tiles_to_redb(&self, db: &redb::Database, pbf_data: &InMemoryPbf) -> Result<()> {
        let start_time = Instant::now();
        info!("Extracting and saving tiles to intermediate storage");

        // Calculate tiles that intersect with PBF bounds
        let tiles = self.calculate_tiles_from_bounds(&pbf_data.bounds);
        let total_tiles = tiles.len();
        info!("Processing {} tiles", total_tiles);

        if total_tiles == 0 {
            info!("No tiles to process");
            return Ok(());
        }

        // Progress counter (shared across threads)
        let completed = Arc::new(AtomicUsize::new(0));

        // Process tiles in parallel using Rayon
        tiles.par_iter().try_for_each(|tile_id| -> Result<()> {
            // Extract data for this tile
            let tile_data = self.extract_tile_data_for_redb(pbf_data, *tile_id)?;

            // Skip empty tiles
            if tile_data.nodes.is_empty() && tile_data.ways.is_empty() {
                return Ok(());
            }

            // Save to redb
            tile_data.save_to_redb(db)?;

            // Update progress
            let count = completed.fetch_add(1, Ordering::Relaxed) + 1;
            if count % 100 == 0 || count == total_tiles {
                info!(
                    "Tile progress: {}/{} ({:.1}%)",
                    count,
                    total_tiles,
                    (count as f64 / total_tiles as f64) * 100.0
                );
            }

            Ok(())
        })?;

        info!(
            "Tile extraction complete - {} tiles saved in {:.1}s",
            completed.load(Ordering::Relaxed),
            start_time.elapsed().as_secs_f64()
        );

        Ok(())
    }

    /// Calculate tile positions from PBF bounds
    fn calculate_tiles_from_bounds(
        &self,
        bounds: &crate::osm_data::in_memory_pbf::PbfBounds,
    ) -> Vec<TileId> {
        let lat_min = bounds.lat_min.unwrap_or(0.0);
        let lat_max = bounds.lat_max.unwrap_or(0.0);
        let lon_min = bounds.lon_min.unwrap_or(0.0);
        let lon_max = bounds.lon_max.unwrap_or(0.0);

        // Add buffer to ensure edge tiles are included
        let buffer = 0.01f64;
        let lat_min_buffered = (lat_min - buffer).max(-90.0);
        let lat_max_buffered = (lat_max + buffer).min(90.0);
        let lon_min_buffered = (lon_min - buffer).max(-180.0);
        let lon_max_buffered = (lon_max + buffer).min(180.0);

        // Calculate tile indices
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

        info!("Calculated {} tiles for bounded region", tiles.len());
        tiles
    }

    /// Extract tile data for intermediate storage
    fn extract_tile_data_for_redb(
        &self,
        pbf_data: &InMemoryPbf,
        tile_id: TileId,
    ) -> Result<intermediate::IntermediateTile> {
        // Calculate bounds for this tile
        let core_bounds = self.calculate_tile_bounds(tile_id);
        let buffered_bounds = self.add_buffer_to_bounds(core_bounds);

        // Query in-memory structure
        let nodes_in_bounds = pbf_data.query_nodes_in_bounds(&buffered_bounds);
        let ways_in_bounds = pbf_data.query_ways_in_bounds(&buffered_bounds);
        let relations_in_bounds = pbf_data.query_relations_in_bounds(&buffered_bounds);

        // Create intermediate tile
        let mut tile = intermediate::IntermediateTile::new(tile_id);

        // Add nodes
        for node in nodes_in_bounds {
            tile.add_node(node.clone());
        }

        // Early exit if no nodes
        if tile.nodes.is_empty() {
            return Ok(tile);
        }

        // Build set of node IDs for way filtering
        let node_ids: HashSet<u64> = tile.nodes.keys().copied().collect();

        // Add ways (highway ways only for routing)
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

        for way_with_bounds in ways_in_bounds {
            let way = &way_with_bounds.way;

            // Skip ways that don't have nodes in the tile
            let has_nodes_in_tile = way.point_ids.iter().any(|id| node_ids.contains(id));
            if !has_nodes_in_tile {
                continue;
            }

            // Check if this is a highway way
            if let Some(ref tags) = way.tags {
                if let Some(highway_value) = tags.get("highway") {
                    let is_highway = ALLOWED_HIGHWAY_VALUES.contains(&highway_value.as_str())
                        || (highway_value == "path"
                            && tags.get("motorcycle").map(|v| v.as_str()) == Some("yes"));

                    if is_highway {
                        tile.add_way(way.clone());
                    }
                }
            }
        }

        // Build set of way IDs for relation filtering
        let way_ids: HashSet<u64> = tile.ways.iter().map(|w| w.id).collect();

        // Add relations (restriction relations only)
        for rel_with_bounds in relations_in_bounds {
            let relation = &rel_with_bounds.relation;

            // Check if relation has any members in the tile
            let has_members_in_tile =
                relation
                    .members
                    .iter()
                    .any(|member| match member.member_type {
                        OsmRelationMemberType::Node => node_ids.contains(&member.member_ref),
                        OsmRelationMemberType::Way => way_ids.contains(&member.member_ref),
                        OsmRelationMemberType::Relation => true,
                    });

            if !has_members_in_tile {
                continue;
            }

            // Only collect restriction relations
            if relation
                .tags
                .get("type")
                .map(|v| v.starts_with("restriction"))
                .unwrap_or(false)
            {
                tile.add_relation(relation.clone());
            }
        }

        Ok(tile)
    }

    /// Calculate geographic bounds for a tile
    fn calculate_tile_bounds(&self, tile_id: TileId) -> TileBounds {
        let lon_min = (tile_id.col as f32 * self.tile_size_degrees) - 180.0;
        let lon_max = lon_min + self.tile_size_degrees;
        let lat_min = (tile_id.row as f32 * self.tile_size_degrees) - 90.0;
        let lat_max = lat_min + self.tile_size_degrees;

        TileBounds {
            lat_min,
            lat_max,
            lon_min,
            lon_max,
        }
    }

    /// Add buffer zone to tile bounds (500m = RESIDENTIAL_PROXIMITY_THRESHOLD_METERS)
    fn add_buffer_to_bounds(&self, bounds: TileBounds) -> TileBounds {
        let buffer_degrees = 0.005f32;

        TileBounds {
            lat_min: (bounds.lat_min - buffer_degrees).max(-90.0),
            lat_max: (bounds.lat_max + buffer_degrees).min(90.0),
            lon_min: (bounds.lon_min - buffer_degrees).max(-180.0),
            lon_max: (bounds.lon_max + buffer_degrees).min(180.0),
        }
    }
    /// Identify zones where PBF grids overlap
    fn identify_overlap_zones(&self, db: &redb::Database) -> Result<Vec<intermediate::GridBounds>> {
        let all_bounds = GridStorage::load_all_bounds(db)?;
        let mut overlap_zones = Vec::new();

        // Find all pairwise overlaps
        for i in 0..all_bounds.len() {
            for j in (i + 1)..all_bounds.len() {
                let (_, bounds_a) = &all_bounds[i];
                let (_, bounds_b) = &all_bounds[j];

                if bounds_a.overlaps(bounds_b) {
                    // Calculate intersection bounds
                    let intersection = intermediate::GridBounds {
                        lon_min: bounds_a.lon_min.max(bounds_b.lon_min),
                        lat_min: bounds_a.lat_min.max(bounds_b.lat_min),
                        lon_max: bounds_a.lon_max.min(bounds_b.lon_max),
                        lat_max: bounds_a.lat_max.min(bounds_b.lat_max),
                    };

                    // Add 500m buffer for proximity check radius
                    let buffer_deg = 500.0 / 111_000.0; // ~500m in degrees
                    let buffered = intermediate::GridBounds {
                        lon_min: intersection.lon_min - buffer_deg as f32,
                        lat_min: intersection.lat_min - buffer_deg as f32,
                        lon_max: intersection.lon_max + buffer_deg as f32,
                        lat_max: intersection.lat_max + buffer_deg as f32,
                    };

                    overlap_zones.push(buffered);
                }
            }
        }

        Ok(overlap_zones)
    }

    /// Re-evaluate nodes in overlap zones with combined grids
    ///
    /// This method:
    /// 1. Loads all grids covering each overlap zone
    /// 2. Merges grids using MAX for sectors, OR for military
    /// 3. Queries nodes in overlap zones from redb
    /// 4. Re-evaluates flags with combined grid
    /// 5. Updates nodes in redb
    fn reevaluate_overlap_zones(
        &self,
        db: &redb::Database,
        overlap_zones: &[intermediate::GridBounds],
    ) -> Result<()> {
        if overlap_zones.is_empty() {
            info!("No overlap zones to re-evaluate");
            return Ok(());
        }

        info!("Re-evaluating {} overlap zones", overlap_zones.len());

        for (zone_idx, zone) in overlap_zones.iter().enumerate() {
            info!(
                "Processing overlap zone {}/{}",
                zone_idx + 1,
                overlap_zones.len()
            );

            // Step 1: Load grids covering this zone
            let grid_data = GridStorage::load_grids_for_region(db, zone)?;
            if grid_data.len() < 2 {
                info!(
                    "Zone {} has only {} overlapping PBF(s), skipping",
                    zone_idx,
                    grid_data.len()
                );
                continue;
            }

            // Step 2: Deserialize grid bytes to RasterizedProximityGrid
            let grids: Vec<(u64, intermediate::GridBounds, RasterizedProximityGrid)> = grid_data
                .into_iter()
                .filter_map(|(pbf_id, bounds, bytes)| {
                    RasterizedProximityGrid::from_bytes(&bytes)
                        .ok()
                        .map(|grid| (pbf_id, bounds, grid))
                })
                .collect();

            if grids.len() < 2 {
                info!(
                    "Zone {} has only {} valid grid(s), skipping",
                    zone_idx,
                    grids.len()
                );
                continue;
            }

            // Step 3: Build combined grid
            let combined_grid = intermediate::build_combined_grid(&grids, zone);
            info!(
                "Zone {}: Combined grid covers {:.3}..{:.3} lat, {:.3}..{:.3} lon",
                zone_idx, zone.lat_min, zone.lat_max, zone.lon_min, zone.lon_max
            );

            // Step 4: Calculate tile positions that intersect with this zone
            let tile_ids = self.tiles_intersecting_bounds(zone);
            info!(
                "Zone {}: Checking {} tiles for nodes to re-evaluate",
                zone_idx,
                tile_ids.len()
            );

            // Step 5: Process each tile, update nodes in overlap zone
            for tile_id in tile_ids {
                self.reevaluate_tile_nodes(db, tile_id, zone, &combined_grid)?;
            }
        }

        info!("Overlap zone re-evaluation complete");
        Ok(())
    }

    /// Calculate tile positions that intersect with a geographic region
    fn tiles_intersecting_bounds(&self, bounds: &intermediate::GridBounds) -> Vec<TileId> {
        let lat_min = bounds.lat_min as f64;
        let lat_max = bounds.lat_max as f64;
        let lon_min = bounds.lon_min as f64;
        let lon_max = bounds.lon_max as f64;

        let col_min = ((lon_min + 180.0) / self.tile_size_degrees as f64).floor() as u16;
        let col_max = ((lon_max + 180.0) / self.tile_size_degrees as f64).ceil() as u16;
        let row_min = ((lat_min + 90.0) / self.tile_size_degrees as f64).floor() as u16;
        let row_max = ((lat_max + 90.0) / self.tile_size_degrees as f64).ceil() as u16;

        let mut tiles = Vec::new();
        for col in col_min..col_max {
            for row in row_min..row_max {
                tiles.push(TileId { col, row });
            }
        }
        tiles
    }

    /// Re-evaluate nodes in a tile that fall within the overlap zone bounds
    fn reevaluate_tile_nodes(
        &self,
        db: &redb::Database,
        tile_id: TileId,
        zone: &intermediate::GridBounds,
        combined_grid: &RasterizedProximityGrid,
    ) -> Result<()> {
        // Load the tile from redb
        let tile = intermediate::IntermediateTile::load_from_redb(db, tile_id)?;

        // Filter nodes that are actually in the overlap zone
        let nodes_in_zone: HashMap<u64, OsmNode> = tile
            .nodes
            .iter()
            .filter(|(_, node)| {
                node.lat >= zone.lat_min as f64
                    && node.lat <= zone.lat_max as f64
                    && node.lon >= zone.lon_min as f64
                    && node.lon <= zone.lon_max as f64
            })
            .map(|(id, node)| (*id, node.clone()))
            .collect();

        if nodes_in_zone.is_empty() {
            return Ok(());
        }

        info!(
            "Tile {:?}: Re-evaluating {} nodes in overlap zone",
            tile_id,
            nodes_in_zone.len()
        );

        // Apply combined grid to these nodes
        let mut nodes_to_update = nodes_in_zone;
        crate::proximity::apply_grid_to_nodes(&mut nodes_to_update, combined_grid);

        // Count how many nodes changed
        let mut changed_count = 0;
        for (osm_id, updated_node) in &nodes_to_update {
            if let Some(original_node) = tile.nodes.get(osm_id) {
                if original_node.residential_in_proximity != updated_node.residential_in_proximity
                    || original_node.nogo_area != updated_node.nogo_area
                {
                    changed_count += 1;
                }
            }
        }

        if changed_count > 0 {
            info!(
                "Tile {:?}: {} nodes changed flags after re-evaluation",
                tile_id, changed_count
            );

            // Create updated tile and save
            let mut updated_tile = tile;
            for (osm_id, updated_node) in nodes_to_update {
                updated_tile.nodes.insert(osm_id, updated_node);
            }
            updated_tile.save_to_redb(db)?;
        }

        Ok(())
    }

    /// Write final deduplicated tiles
    ///
    /// This method:
    /// 1. Discovers all tile positions from redb
    /// 2. For each tile: loads, deduplicates, builds graph, writes RMDF
    /// 3. Generates manifest with all source filenames
    fn write_final_tiles(
        &self,
        db: &redb::Database,
        pbf_files: &[PathBuf],
        military_polygons: &[MultiPolygon<f64>],
    ) -> Result<()> {
        info!("Writing final RMDF tiles");

        // Discover all tile positions from redb
        let tile_ids = intermediate::TileBuffers::discover_tiles_from_redb(db)?;
        info!("Discovered {} tiles to write", tile_ids.len());

        if tile_ids.is_empty() {
            info!("No tiles to write");
            return Ok(());
        }

        // Progress counter
        let completed = Arc::new(AtomicUsize::new(0));
        let total_tiles = tile_ids.len();

        // Process tiles in parallel
        tile_ids.par_iter().try_for_each(|tile_id| -> Result<()> {
            // Load tile from redb (contains data from all PBFs)
            let tile = intermediate::IntermediateTile::load_from_redb(db, *tile_id)?;

            // Skip empty tiles
            if tile.is_empty() {
                return Ok(());
            }

            // Deduplicate (ways/relations may have duplicates from multiple PBFs)
            let mut tile = tile;
            tile.deduplicate();

            // Build GenerationGraph
            let graph = self.build_generation_graph_from_tile(tile);

            // Write RMDF tile
            self.write_rmdf_tile(*tile_id, graph)?;

            // Update progress
            let count = completed.fetch_add(1, Ordering::Relaxed) + 1;
            if count % 100 == 0 || count == total_tiles {
                info!(
                    "Tile writing progress: {}/{} ({:.1}%)",
                    count,
                    total_tiles,
                    (count as f64 / total_tiles as f64) * 100.0
                );
            }

            Ok(())
        })?;

        info!("Wrote {} RMDF tiles", completed.load(Ordering::Relaxed));

        // Generate manifest
        self.generate_manifest(
            db,
            pbf_files,
            completed.load(Ordering::Relaxed),
            military_polygons,
        )?;

        // Clean up intermediate storage
        self.cleanup_intermediate_storage()?;

        info!("Final tile writing complete");
        Ok(())
    }

    /// Build GenerationGraph from an IntermediateTile
    fn build_generation_graph_from_tile(
        &self,
        tile: intermediate::IntermediateTile,
    ) -> GenerationGraph {
        let intermediate::IntermediateTile {
            nodes,
            ways,
            relations,
            ..
        } = tile;
        let mut graph = GenerationGraph::new();

        // Insert all nodes (with correct proximity flags from re-evaluation)
        let mut sorted_nodes: Vec<_> = nodes.into_values().collect();
        sorted_nodes.sort_unstable_by_key(|node| node.id);
        for node in sorted_nodes {
            graph.insert_node(node);
        }

        // Insert all ways
        for way in ways {
            graph.insert_way(way);
        }

        // Materialize supported restriction relations into the generation graph.
        for relation in relations {
            graph.insert_relation(relation);
        }
        graph.log_restriction_skip_summary();

        // Generate point hashes for spatial indexing
        graph.generate_point_hashes();

        graph
    }

    /// Write RMDF tile file from generation graph
    fn write_rmdf_tile(&self, tile_id: TileId, graph: GenerationGraph) -> Result<()> {
        use crate::rmdf::generator::writer::RmdfWriter;

        let output_path = self.output_dir.join(tile_id.to_filename());

        // Create RmdfWriter
        let writer = RmdfWriter::new(self.tile_size_degrees);

        // Write tile directly from GenerationGraph
        writer
            .write_tile_from_graph(tile_id, graph, &output_path)
            .with_context(|| format!("Failed to write RMDF tile {:?}", tile_id))?;

        Ok(())
    }

    /// Generate manifest for the output tiles
    fn generate_manifest(
        &self,
        _db: &redb::Database,
        pbf_files: &[PathBuf],
        tile_count: usize,
        military_polygons: &[MultiPolygon<f64>],
    ) -> Result<()> {
        // Load all PBF filenames from storage
        let source_files: Vec<String> = pbf_files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .map(String::from)
            .collect();

        // Generate manifest using the existing generator
        let manifest_gen = ManifestGenerator::new(self.tile_size_degrees);
        let tile_ids = manifest_gen
            .discover_tiles(&self.output_dir)
            .context("Failed to discover tiles from filesystem")?;

        if tile_ids.is_empty() {
            tracing::warn!("No tiles discovered for manifest");
            return Ok(());
        }

        maybe_export_military_geojson_tiles(
            &self.output_dir,
            self.tile_size_degrees,
            &tile_ids,
            military_polygons,
        )
        .context("Failed to export military GeoJSON tiles")?;

        // Use the first source file as the primary source (for manifest compatibility)
        let primary_source = source_files
            .first()
            .map(|s| s.as_str())
            .unwrap_or("unknown");

        manifest_gen
            .generate(&self.output_dir, &tile_ids, primary_source)
            .context("Failed to generate manifest")?;

        info!(
            "Generated manifest for {} tiles from {} source files",
            tile_count,
            source_files.len()
        );

        Ok(())
    }

    /// Clean up intermediate storage
    fn cleanup_intermediate_storage(&self) -> Result<()> {
        if self.db_path.exists() {
            std::fs::remove_file(&self.db_path)?;
            info!("Cleaned up intermediate storage: {:?}", self.db_path);
        }
        Ok(())
    }
}
