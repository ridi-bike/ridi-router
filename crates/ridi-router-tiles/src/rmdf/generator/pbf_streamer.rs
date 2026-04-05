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
use rayon::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

use crate::generation::GenerationGraph;
use crate::osm_data::in_memory_pbf::{InMemoryPbf, PbfBounds};
use crate::osm_data::{OsmNode, OsmRelation, OsmRelationMemberType, OsmWay};
use crate::rmdf::format::TileId;
use std::collections::HashMap;

// Allowed highway values for routing
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

/// Data extracted from PBF for a single tile
struct TileData {
    nodes: HashMap<u64, OsmNode>, // OSM ID -> Node with coordinates (flags pre-computed)
    ways: Vec<OsmWay>,
    relations: Vec<OsmRelation>,
}

impl TileData {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            ways: Vec::new(),
            relations: Vec::new(),
        }
    }
}

pub struct PbfStreamer<'a> {
    pbf_data: &'a InMemoryPbf,
    output_dir: PathBuf,
    tile_size_degrees: f32,
}

impl<'a> PbfStreamer<'a> {
    pub fn new(pbf_data: &'a InMemoryPbf, output_dir: &PathBuf, tile_size_degrees: f32) -> Self {
        Self {
            pbf_data,
            output_dir: output_dir.clone(),
            tile_size_degrees,
        }
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

        // Use pre-computed bounds from in-memory PBF (no scan needed!)
        let pbf_bounds = self.pbf_data.bounds;

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
    ///
    /// With pre-computed flags, this method is simplified:
    /// 1. Calculate bounds
    /// 2. Extract PBF data (nodes already have correct flags)
    /// 3. Build generation graph
    /// 4. Write RMDF tile
    fn process_tile(&self, tile_id: TileId) -> Result<()> {
        // Wrap entire tile processing in context for better error messages
        (|| -> Result<()> {
            // Step 1: Calculate bounds
            let core_bounds = self.calculate_tile_bounds(tile_id);
            let buffered_bounds = self.add_buffer_to_bounds(core_bounds);

            // Step 2: Extract PBF data for this tile
            // Nodes already have residential_in_proximity and nogo_area flags pre-computed
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

            // Step 3: Build generation graph
            // Nodes already have correct flags from pre-computation
            let graph = self.build_generation_graph(
                tile_data.nodes,
                tile_data.ways,
                tile_data.relations,
            );

            // Step 4: Write RMDF tile file
            self.write_rmdf_tile(tile_id, graph)
                .with_context(|| format!(
                    "Failed to write RMDF tile {:?} to disk",
                    tile_id
                ))?;

            Ok(())
        })().with_context(|| format!("Failed to process tile {:?}", tile_id))
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
        let lat_min = pbf_bounds.lat_min.expect("PBF bounds should have lat_min");
        let lat_max = pbf_bounds.lat_max.expect("PBF bounds should have lat_max");
        let lon_min = pbf_bounds.lon_min.expect("PBF bounds should have lon_min");
        let lon_max = pbf_bounds.lon_max.expect("PBF bounds should have lon_max");

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

        info!(
            "Calculated {} tiles for bounded region (lat=[{:.3}, {:.3}], lon=[{:.3}, {:.3}])",
            tiles.len(),
            lat_min_buffered,
            lat_max_buffered,
            lon_min_buffered,
            lon_max_buffered,
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
    /// Extract nodes, ways, and relations within buffered bounds using in-memory PBF
    ///
    /// Queries the pre-loaded in-memory data structure (NO PBF READS!)
    fn extract_tile_data(
        &self,
        tile_id: TileId,
        buffered_bounds: crate::rmdf::format::TileBounds,
    ) -> Result<TileData> {
        let mut tile_data = TileData::new();

        // Query in-memory structure (O(log n) spatial lookups - FAST!)
        let nodes_in_bounds = self.pbf_data.query_nodes_in_bounds(&buffered_bounds);
        let ways_in_bounds = self.pbf_data.query_ways_in_bounds(&buffered_bounds);
        let relations_in_bounds = self.pbf_data.query_relations_in_bounds(&buffered_bounds);

        // Early exit if no nodes in bounds
        if nodes_in_bounds.is_empty() {
            return Ok(tile_data);
        }

        // Build node map
        for node in nodes_in_bounds {
            tile_data.nodes.insert(node.id, node.clone());
        }

        // Build set of node IDs for way filtering
        let node_ids: std::collections::HashSet<u64> = tile_data.nodes.keys().copied().collect();

        // Process ways - only collect highway ways (area ways handled during PBF load)
        for way_with_bounds in ways_in_bounds {
            let way = &way_with_bounds.way;

            // Skip ways that don't have nodes in the tile
            let has_nodes_in_tile = way.point_ids.iter().any(|id| node_ids.contains(id));
            if !has_nodes_in_tile {
                continue;
            }

            // Check if this is a highway way (area ways no longer needed per-tile)
            if let Some(ref tags) = way.tags {
                if let Some(highway_value) = tags.get("highway") {
                    let is_highway = ALLOWED_HIGHWAY_VALUES.contains(&highway_value.as_str())
                        || (highway_value == "path"
                            && tags.get("motorcycle").map(|v| v.as_str()) == Some("yes"));

                    if is_highway {
                        tile_data.ways.push(way.clone());
                    }
                }
            }
        }

        // Build set of way IDs for relation filtering
        let way_ids: std::collections::HashSet<u64> = tile_data.ways.iter().map(|w| w.id).collect();

        // Process relations - only collect restriction relations (area relations handled during PBF load)
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

            // Only collect restriction relations (area relations handled during PBF load)
            if relation
                .tags
                .get("type")
                .map(|v| v.starts_with("restriction"))
                .unwrap_or(false)
            {
                tile_data.relations.push(relation.clone());
            }
        }

        // Log tile statistics
        if tile_data.nodes.is_empty() && tile_data.ways.is_empty() {
            info!(
                "Tile {:?} is empty (likely ocean or unpopulated area)",
                tile_id
            );
        } else {
            info!(
                "Extracted tile {:?}: {} nodes, {} ways, {} relations (flags pre-computed)",
                tile_id,
                tile_data.nodes.len(),
                tile_data.ways.len(),
                tile_data.relations.len()
            );
        }

        Ok(tile_data)
    }

    /// Build GenerationGraph from tile data
    /// Nodes already have correct residential_in_proximity and nogo_area flags from pre-computation
    fn build_generation_graph(
        &self,
        nodes: HashMap<u64, OsmNode>,
        ways: Vec<OsmWay>,
        relations: Vec<OsmRelation>,
    ) -> GenerationGraph {
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
        info!("Writing RMDF tile to {:?}", output_path);

        // Create the RMDF writer for the generation model.
        let writer = RmdfWriter::new(self.tile_size_degrees);

        // Write tile directly from the generation graph.
        writer
            .write_tile_from_graph(tile_id, graph, &output_path)
            .with_context(|| format!("Failed to write RMDF tile {:?}", tile_id))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osm_data::in_memory_pbf::InMemoryPbf;
    use crate::rmdf::format::TileBounds;

    fn create_test_streamer(tile_size: f32) -> PbfStreamer<'static> {
        static DUMMY_PBF: std::sync::OnceLock<InMemoryPbf> = std::sync::OnceLock::new();
        let pbf = DUMMY_PBF.get_or_init(InMemoryPbf::default);
        PbfStreamer::new(pbf, &PathBuf::from("output"), tile_size)
    }
    #[test]
    fn test_pbf_bounds_new_empty() {
        let bounds = PbfBounds::empty();
        assert!(bounds.lat_min.is_none());
        assert!(bounds.lat_max.is_none());
        assert!(bounds.lon_min.is_none());
        assert!(bounds.lon_max.is_none());
        assert!(!bounds.is_valid());
    }

    #[test]
    fn test_pbf_bounds_update() {
        let mut bounds = PbfBounds::empty();

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
        let mut bounds = PbfBounds::empty();
        assert!(!bounds.is_valid());

        bounds.update(50.0, 10.0);
        assert!(bounds.is_valid());
    }

    #[test]
    fn test_pbf_bounds_unwrap() {
        let mut bounds = PbfBounds::empty();
        bounds.update(50.0, 10.0);
        bounds.update(51.0, 11.0);

        let (lat_min, lat_max, lon_min, lon_max) = bounds.extract();
        assert_eq!(lat_min, 50.0);
        assert_eq!(lat_max, 51.0);
        assert_eq!(lon_min, 10.0);
        assert_eq!(lon_max, 11.0);
    }

    #[test]
    fn test_buffer_zone_calculation() {
        let streamer = create_test_streamer(1.0);

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
        let streamer = create_test_streamer(1.0);

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
        let streamer = create_test_streamer(1.0);

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
        let streamer = create_test_streamer(1.0);

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
        let streamer = create_test_streamer(1.0);

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
        let streamer = create_test_streamer(0.1);

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
        assert!(
            tiles.len() < 1000,
            "Expected < 1000 tiles, got {}",
            tiles.len()
        );
        assert!(
            tiles.len() > 100,
            "Expected > 100 tiles, got {}",
            tiles.len()
        );
    }

    #[test]
    fn test_calculate_all_tiles_single_point() {
        let streamer = create_test_streamer(1.0);

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
    fn test_tile_data_validation_no_orphaned_ways() {
        let mut tile_data = TileData::new();

        // Add nodes
        tile_data.nodes.insert(
            1,
            OsmNode {
                id: 1,
                lat: 50.0,
                lon: 10.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
        );
        tile_data.nodes.insert(
            2,
            OsmNode {
                id: 2,
                lat: 50.1,
                lon: 10.1,
                residential_in_proximity: false,
                nogo_area: false,
            },
        );

        // Add way that references existing nodes
        tile_data.ways.push(OsmWay {
            id: 100,
            point_ids: vec![1, 2],
            tags: Some(HashMap::new()),
        });

        // Validate no orphaned nodes
        let node_ids: std::collections::HashSet<u64> = tile_data.nodes.keys().copied().collect();
        for way in &tile_data.ways {
            let missing_nodes: Vec<u64> = way
                .point_ids
                .iter()
                .filter(|id| !node_ids.contains(id))
                .copied()
                .collect();
            assert!(
                missing_nodes.is_empty(),
                "Way {} should not have orphaned nodes, but found {:?}",
                way.id,
                missing_nodes
            );
        }
    }

    #[test]
    fn test_tile_data_validation_detects_orphaned_ways() {
        let mut tile_data = TileData::new();

        // Add only node 1
        tile_data.nodes.insert(
            1,
            OsmNode {
                id: 1,
                lat: 50.0,
                lon: 10.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
        );

        // Add way that references node 1 and non-existent node 2
        tile_data.ways.push(OsmWay {
            id: 100,
            point_ids: vec![1, 2],
            tags: Some(HashMap::new()),
        });

        // Validate orphaned nodes are detected
        let node_ids: std::collections::HashSet<u64> = tile_data.nodes.keys().copied().collect();
        for way in &tile_data.ways {
            let missing_nodes: Vec<u64> = way
                .point_ids
                .iter()
                .filter(|id| !node_ids.contains(id))
                .copied()
                .collect();

            if way.id == 100 {
                assert_eq!(
                    missing_nodes,
                    vec![2],
                    "Way 100 should have orphaned node 2"
                );
            }
        }
    }
}
