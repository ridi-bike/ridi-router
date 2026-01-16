use anyhow::{Context, Result};
use osmpbfreader::{OsmPbfReader, OsmObj};
use std::fs::File;
use std::path::{Path, PathBuf};
use tracing::info;
use redb::{Database, TableDefinition, ReadableTable};
use rayon::prelude::*;
use geo::{Point, Distance, Haversine, HaversineClosestPoint, CoordsIter, GeodesicArea};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::map_data::osm::{OsmNode, OsmWay, OsmRelation, OsmRelationMember, OsmRelationMemberRole, OsmRelationMemberType};
use crate::map_data::proximity::AreaGrid;
use crate::osm_data::pbf_area_reader::PbfAreaReader;
use crate::rmdf::format::TileId;
// TODO: Move this constant somewhere accessible or re-export from osm_data
// use crate::osm_data::data_reader::ALLOWED_HIGHWAY_VALUES;

// Temporarily define locally until we re-organize modules
const ALLOWED_HIGHWAY_VALUES: [&str; 17] = [
    "motorway", "trunk", "primary", "secondary", "tertiary",
    "unclassified", "residential", "motorway_link", "trunk_link",
    "primary_link", "secondary_link", "tertiary_link",
    "living_street", "track", "escape", "raceway", "road",
];

// Constants from src/osm_data/pbf_reader.rs
const RESIDENTIAL_PROXIMITY_THRESHOLD_METERS: f64 = 500.0;
const RESIDENTIAL_PART_COVERED: f64 = 0.10;
const THRESHOLD_AREA: f64 = (RESIDENTIAL_PROXIMITY_THRESHOLD_METERS
    * RESIDENTIAL_PROXIMITY_THRESHOLD_METERS
    * std::f64::consts::PI)
    * RESIDENTIAL_PART_COVERED;
const MILITARY_ENTRY_MAX_M: f64 = 100.0;

use super::intermediate::TileBuffers;

// redb table definition for node coordinates with proximity flags
// Format: (lat, lon, residential_in_proximity, nogo_area)
const NODE_COORDS_TABLE: TableDefinition<u64, (f32, f32, bool, bool)> = TableDefinition::new("node_coords");

// Flush tile buffers to disk every N ways to prevent memory accumulation
const FLUSH_INTERVAL: usize = 10_000;

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

    pub fn partition(&self) -> Result<(TileBuffers, Database)> {
        info!("Starting PBF streaming partitioning");

        // Create redb database for node coordinates (disk-backed storage)
        let node_coords_db_path = self.output_dir.join("node_coords.redb");
        let node_coords_db = Database::create(&node_coords_db_path)
            .context("Failed to create node coordinates database")?;

        // Create redb database for intermediate tiles
        let tiles_db_path = self.output_dir.join("intermediate_tiles.redb");
        let tiles_db = Database::create(&tiles_db_path)
            .context("Failed to create intermediate tiles database")?;

        // First pass: collect all nodes with their coordinates in redb
        info!("First pass: collecting node coordinates to disk-backed database");
        {
            let file = File::open(&self.input_file)
                .context("Failed to open PBF file")?;
            let mut pbf = OsmPbfReader::new(file);

            let write_txn = node_coords_db.begin_write()
                .context("Failed to begin write transaction")?;
            {
                let mut table = write_txn.open_table(NODE_COORDS_TABLE)
                    .context("Failed to open node coords table")?;

                let mut node_count = 0;
                for obj_result in pbf.iter() {
                    let obj = obj_result.context("Failed to read PBF object")?;

                    if let OsmObj::Node(node) = obj {
                        table.insert(node.id.0 as u64, (node.lat() as f32, node.lon() as f32, false, false))
                            .context("Failed to insert node coordinates")?;
                        node_count += 1;

                        if node_count % 1_000_000 == 0 {
                            info!("Collected {} million node coordinates", node_count / 1_000_000);
                        }
                    }
                }

                info!("Collected {} total node coordinates", node_count);
            }
            write_txn.commit()
                .context("Failed to commit node coordinates")?;
        }

        // Phase 2A: Compute proximity flags for all nodes in parallel
        info!("Computing proximity flags for all nodes");
        self.compute_proximity_parallel(&node_coords_db)?;

        // Second pass: partition elements by tile with incremental flushing
        info!("Second pass: partitioning elements");
        let mut tile_buffers = TileBuffers::new();
        {
            let file = File::open(&self.input_file)
                .context("Failed to reopen PBF file for second pass")?;
            let mut pbf = OsmPbfReader::new(file);

            let read_txn = node_coords_db.begin_read()
                .context("Failed to begin read transaction")?;
            let table = read_txn.open_table(NODE_COORDS_TABLE)
                .context("Failed to open node coords table")?;

            let mut processed_count = 0;
            let mut way_count = 0;

            for obj_result in pbf.iter() {
                let obj = obj_result.context("Failed to read PBF object")?;

                match obj {
                    OsmObj::Node(node) => {
                        self.partition_node(&mut tile_buffers, node, &table)?;
                    }
                    OsmObj::Way(way) => {
                        self.partition_way_redb(&mut tile_buffers, way, &table)?;
                        way_count += 1;

                        // Flush tiles periodically to prevent memory accumulation
                        if way_count % FLUSH_INTERVAL == 0 {
                            info!("Flushing tiles after {} ways", way_count);
                            tile_buffers.flush_to_redb(&tiles_db)?;
                        }
                    }
                    OsmObj::Relation(relation) => {
                        self.partition_relation_redb(&mut tile_buffers, relation, &table)?;
                    }
                }

                processed_count += 1;
            }

            info!("Processed {} total objects, {} ways", processed_count, way_count);

            // Final flush of remaining tiles
            info!("Final flush of remaining tiles");
            tile_buffers.flush_to_redb(&tiles_db)?;
        }

        // Clean up node coordinates database
        drop(node_coords_db);
        std::fs::remove_file(&node_coords_db_path)
            .context("Failed to remove node coordinates database")?;
        info!("Cleaned up temporary node coordinates database");

        // Return the database handle to keep it alive for subsequent phases
        Ok((tile_buffers, tiles_db))
    }

    /// New partition method using tile-based parallel processing
    pub fn partition_parallel(&self) -> Result<()> {
        info!("Starting tile-based parallel partitioning");

        // Calculate all tile boundaries upfront
        let tiles = self.calculate_all_tiles();
        let total_tiles = tiles.len();
        info!("Processing {} tiles in parallel", total_tiles);

        // Progress counter (shared across threads)
        let completed = Arc::new(AtomicUsize::new(0));

        // Process tiles in parallel using Rayon
        tiles.par_iter()
            .try_for_each(|tile_id| -> Result<()> {
                self.process_tile(*tile_id)?;

                // Update progress
                let count = completed.fetch_add(1, Ordering::Relaxed) + 1;
                if count % 10 == 0 || count == total_tiles {
                    info!("Processed {}/{} tiles", count, total_tiles);
                }

                Ok(())
            })?;

        info!("Tile-based partitioning complete");
        Ok(())
    }

    /// Process a single tile (stub for now)
    fn process_tile(&self, tile_id: TileId) -> Result<()> {
        // Calculate bounds
        let core_bounds = self.calculate_tile_bounds(tile_id);
        let buffered_bounds = self.add_buffer_to_bounds(core_bounds);

        info!("Processing tile {:?} (bounds: {:?})", tile_id, buffered_bounds);

        // TODO: Implement tile processing in subsequent phases

        Ok(())
    }

    fn partition_node(
        &self,
        buffers: &mut TileBuffers,
        node: osmpbfreader::Node,
        node_coords_table: &impl ReadableTable<u64, (f32, f32, bool, bool)>
    ) -> Result<()> {
        // Read proximity flags from database (computed in Phase 2A)
        let (residential, nogo) = if let Some(coords) = node_coords_table.get(&(node.id.0 as u64))? {
            let (_, _, residential, nogo) = coords.value();
            (residential, nogo)
        } else {
            (false, false) // Default if missing
        };

        let osm_node = OsmNode {
            id: node.id.0 as u64,
            lat: node.lat(),
            lon: node.lon(),
            residential_in_proximity: residential,
            nogo_area: nogo,
        };

        // Determine which tile(s) this node belongs to
        let tile_ids = self.get_tiles_for_point(osm_node.lat as f32, osm_node.lon as f32);

        for tile_id in tile_ids {
            buffers.get_or_create(tile_id).add_node(osm_node.clone());
        }

        Ok(())
    }

    fn partition_way_redb<T: ReadableTable<u64, (f32, f32, bool, bool)>>(
        &self,
        buffers: &mut TileBuffers,
        way: osmpbfreader::Way,
        node_coords_table: &T
    ) -> Result<()> {
        // Only include ways with highway tags (matching current behavior)
        let has_highway = way.tags.iter().any(|(k, v)| {
            k == "highway" && (
                ALLOWED_HIGHWAY_VALUES.contains(&v.as_str()) ||
                (v == "path" && way.tags.iter().any(|(k2, v2)| k2 == "motorcycle" && v2 == "yes"))
            )
        });

        if !has_highway {
            return Ok(());
        }

        let osm_way = OsmWay {
            id: way.id.0 as u64,
            point_ids: way.nodes.iter().map(|n| n.0 as u64).collect(),
            tags: Some(way.tags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()),
        };

        // Determine which tiles this way crosses
        // Add to all tiles that contain any of the way's nodes
        let mut tile_set = std::collections::HashSet::new();

        for node_id in &osm_way.point_ids {
            if let Some(coords_guard) = node_coords_table.get(node_id)
                .context("Failed to lookup node coordinates")? {
                let (lat, lon, _, _) = coords_guard.value();
                let tile_ids = self.get_tiles_for_point(lat, lon);
                for tile_id in tile_ids {
                    tile_set.insert(tile_id);
                }
            }
        }

        // Add way to all relevant tiles
        for tile_id in tile_set {
            buffers.get_or_create(tile_id).add_way(osm_way.clone());
        }

        Ok(())
    }

    fn partition_relation_redb<T: ReadableTable<u64, (f32, f32, bool, bool)>>(
        &self,
        buffers: &mut TileBuffers,
        relation: osmpbfreader::Relation,
        node_coords_table: &T
    ) -> Result<()> {
        // Only include restriction relations (matching current behavior)
        let is_restriction = relation.tags.iter().any(|(k, v)| {
            k == "type" && v.starts_with("restriction")
        });

        if !is_restriction {
            return Ok(());
        }

        let osm_relation = OsmRelation {
            id: relation.id.0 as u64,
            members: relation.refs.iter().filter_map(|r| {
                let role = match r.role.as_str() {
                    "from" => OsmRelationMemberRole::From,
                    "to" => OsmRelationMemberRole::To,
                    "via" => OsmRelationMemberRole::Via,
                    _ => return None,
                };

                let (member_ref, member_type) = match r.member {
                    osmpbfreader::OsmId::Way(id) => (id.0 as u64, OsmRelationMemberType::Way),
                    osmpbfreader::OsmId::Node(id) => (id.0 as u64, OsmRelationMemberType::Node),
                    osmpbfreader::OsmId::Relation(_id) => return None, // Skip nested relations
                };

                Some(OsmRelationMember {
                    member_ref,
                    role,
                    member_type,
                })
            }).collect(),
            tags: relation.tags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        };

        // Determine which tiles this relation belongs to
        // Use the "via" member location if possible
        let mut tile_set = std::collections::HashSet::new();

        // For now, add to all tiles that contain any member nodes
        for member in &osm_relation.members {
            if member.member_type == OsmRelationMemberType::Node {
                if let Some(coords_guard) = node_coords_table.get(&member.member_ref)
                    .context("Failed to lookup node coordinates")? {
                    let (lat, lon, _, _) = coords_guard.value();
                    let tile_ids = self.get_tiles_for_point(lat, lon);
                    for tile_id in tile_ids {
                        tile_set.insert(tile_id);
                    }
                }
            }
        }

        // Add relation to all relevant tiles
        for tile_id in tile_set {
            buffers.get_or_create(tile_id).add_relation(osm_relation.clone());
        }

        Ok(())
    }

    /// Compute proximity flags for all nodes in parallel
    fn compute_proximity_parallel(&self, node_coords_db: &Database) -> Result<()> {
        // 1. Extract area grids (residential and military)
        info!("Extracting area grids from PBF");
        let residential_areas = self.extract_residential_areas()?;
        let military_areas = self.extract_military_areas()?;
        info!("Area grids extracted");

        // 2. Load ALL node coordinates into memory
        let read_txn = node_coords_db.begin_read()?;
        let table = read_txn.open_table(NODE_COORDS_TABLE)?;

        let nodes: Vec<(u64, f32, f32)> = table.iter()?
            .map(|entry| {
                let entry = entry?;
                let key = entry.0.value();
                let (lat, lon, _, _) = entry.1.value();
                Ok((key, lat, lon))
            })
            .collect::<Result<Vec<_>>>()?;
        drop(read_txn);

        info!("Computing proximity for {} nodes in parallel", nodes.len());

        // 3. Compute proximity in parallel (RESTORE ORIGINAL PATTERN)
        let results: Vec<(u64, bool, bool)> = nodes.par_iter()
            .map(|(node_id, lat, lon)| {
                let residential = Self::compute_residential_proximity(
                    *lat as f64, *lon as f64, &residential_areas
                );
                let nogo = Self::compute_nogo_area(
                    *lat as f64, *lon as f64, &military_areas
                );
                (*node_id, residential, nogo)
            })
            .collect();

        // 4. Write results back to database
        info!("Writing proximity results back to database");
        let write_txn = node_coords_db.begin_write()?;
        {
            let mut table = write_txn.open_table(NODE_COORDS_TABLE)?;
            for (node_id, residential, nogo) in results {
                // Extract coordinates before inserting to avoid borrow checker issues
                let coords = if let Some(coords_guard) = table.get(&node_id)? {
                    let (lat, lon, _, _) = coords_guard.value();
                    Some((lat, lon))
                } else {
                    None
                };

                if let Some((lat, lon)) = coords {
                    table.insert(node_id, (lat, lon, residential, nogo))?;
                }
            }
        }
        write_txn.commit()?;

        info!("Proximity computation complete");
        Ok(())
    }

    /// Extract residential areas from PBF
    fn extract_residential_areas(&self) -> Result<AreaGrid> {
        let file = File::open(&self.input_file)
            .context("Failed to open PBF file")?;
        let mut pbf = OsmPbfReader::new(file);

        let mut boundary_reader = PbfAreaReader::new(&mut pbf);
        boundary_reader.read(&|obj| {
            (obj.is_way() || obj.is_relation()) && obj.tags().contains("landuse", "residential")
        })?;
        Ok(boundary_reader.get_area_grid())
    }

    /// Extract military areas from PBF
    fn extract_military_areas(&self) -> Result<AreaGrid> {
        let file = File::open(&self.input_file)
            .context("Failed to open PBF file")?;
        let mut pbf = OsmPbfReader::new(file);

        let mut boundary_reader = PbfAreaReader::new(&mut pbf);
        boundary_reader.read(&|obj| {
            (obj.is_way() || obj.is_relation()) && obj.tags().contains("landuse", "military")
        })?;
        Ok(boundary_reader.get_area_grid())
    }

    /// Compute residential proximity flag (from src/osm_data/pbf_reader.rs:90-126)
    fn compute_residential_proximity(lat: f64, lon: f64, residential_areas: &AreaGrid) -> bool {
        let tot_area = match residential_areas.find_closest_areas_refs(
            lat as f32,
            lon as f32,
            1,  // Search 1 grid step (~1.1km)
        ) {
            Some(areas) => areas.iter().fold(0., |tot, multi_polygon| {
                let geo_point = Point::new(lon, lat);
                let distance = match multi_polygon.haversine_closest_point(&geo_point) {
                    geo::Closest::Intersection(_) => 0.,
                    geo::Closest::SinglePoint(p) => {
                        Haversine.distance(p, geo_point)
                    }
                    geo::Closest::Indeterminate => multi_polygon
                        .coords_iter()
                        .fold(10000., |min, coords| {
                            let dist = Haversine.distance(geo_point, Point::from(coords));
                            if dist < min { dist } else { min }
                        }),
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
        match military_areas.find_closest_areas_refs(
            lat as f32,
            lon as f32,
            1,
        ) {
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

    /// Get tile ID(s) for a point
    /// Returns multiple tiles if point is on a border
    fn get_tiles_for_point(&self, lat: f32, lon: f32) -> Vec<TileId> {
        let mut tiles = Vec::new();

        // Primary tile
        let primary_col = ((lon + 180.0) / self.tile_size_degrees).floor() as u16;
        let primary_row = ((lat + 90.0) / self.tile_size_degrees).floor() as u16;

        tiles.push(TileId { col: primary_col, row: primary_row });

        // Check if on horizontal border (longitude)
        let lon_offset = (lon + 180.0) % self.tile_size_degrees;
        if lon_offset < 0.0001 && primary_col > 0 {
            // On western border, also add to western neighbor
            tiles.push(TileId { col: primary_col - 1, row: primary_row });
        } else if (self.tile_size_degrees - lon_offset) < 0.0001 {
            // On eastern border, also add to eastern neighbor
            tiles.push(TileId { col: primary_col + 1, row: primary_row });
        }

        // Check if on vertical border (latitude)
        let lat_offset = (lat + 90.0) % self.tile_size_degrees;
        if lat_offset < 0.0001 && primary_row > 0 {
            // On southern border, also add to southern neighbor
            tiles.push(TileId { col: primary_col, row: primary_row - 1 });

            // Corner case: also add to southwestern neighbor
            if lon_offset < 0.0001 && primary_col > 0 {
                tiles.push(TileId { col: primary_col - 1, row: primary_row - 1 });
            } else if (self.tile_size_degrees - lon_offset) < 0.0001 {
                tiles.push(TileId { col: primary_col + 1, row: primary_row - 1 });
            }
        } else if (self.tile_size_degrees - lat_offset) < 0.0001 {
            // On northern border, also add to northern neighbor
            tiles.push(TileId { col: primary_col, row: primary_row + 1 });

            // Corner case: also add to northwestern neighbor
            if lon_offset < 0.0001 && primary_col > 0 {
                tiles.push(TileId { col: primary_col - 1, row: primary_row + 1 });
            } else if (self.tile_size_degrees - lon_offset) < 0.0001 {
                tiles.push(TileId { col: primary_col + 1, row: primary_row + 1 });
            }
        }

        tiles
    }

    /// Calculate all tiles that cover the world for the given tile size
    fn calculate_all_tiles(&self) -> Vec<TileId> {
        let mut tiles = Vec::new();

        // Longitude: -180 to +180 (360 degrees)
        // Latitude: -90 to +90 (180 degrees)
        let cols = (360.0 / self.tile_size_degrees).ceil() as u16;
        let rows = (180.0 / self.tile_size_degrees).ceil() as u16;

        for col in 0..cols {
            for row in 0..rows {
                tiles.push(TileId { col, row });
            }
        }

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
    fn add_buffer_to_bounds(&self, bounds: crate::rmdf::format::TileBounds) -> crate::rmdf::format::TileBounds {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_assignment_no_border() {
        let streamer = PbfStreamer::new(
            Path::new("dummy.pbf"),
            Path::new("output"),
            1.0
        ).unwrap();

        let tiles = streamer.get_tiles_for_point(56.5, 24.5);
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0], TileId { col: 204, row: 146 });
    }

    #[test]
    fn test_tile_assignment_on_border() {
        let streamer = PbfStreamer::new(
            Path::new("dummy.pbf"),
            Path::new("output"),
            1.0
        ).unwrap();

        // Exactly on 57°N border
        let tiles = streamer.get_tiles_for_point(57.0, 24.5);
        assert!(tiles.len() >= 2); // Should be in both tiles
    }

    #[test]
    fn test_tile_assignment_corner() {
        let streamer = PbfStreamer::new(
            Path::new("dummy.pbf"),
            Path::new("output"),
            1.0
        ).unwrap();

        // Exactly on corner (57°N, 25°E)
        let tiles = streamer.get_tiles_for_point(57.0, 25.0);
        assert_eq!(tiles.len(), 4); // Should be in all 4 corner tiles
    }
}
