//! Flag computation orchestration.
//!
//! Coordinates the building of the proximity grid and application of flags to nodes.

use geo::MultiPolygon;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::info;

use crate::map_data::osm::OsmNode;
use crate::osm_data::in_memory_pbf::PbfBounds;

use super::area_rasterizer::AreaRasterizer;
use super::rasterized_grid::RasterizedProximityGrid;

/// Build proximity grid and apply flags to nodes.
///
/// This is the main entry point for the pre-computation pipeline:
/// 1. Build rasterized grid from bounds
/// 2. Rasterize residential polygons into grid
/// 3. Rasterize military polygons into grid
/// 4. Apply grid flags to all nodes (parallel)
///
/// # Arguments
///
/// * `nodes` - Mutable reference to node map (flags will be updated in place)
/// * `residential_polygons` - Residential area polygons
/// * `military_polygons` - Military area polygons
/// * `bounds` - Geographic bounds of the data
pub fn compute_proximity_flags(
    nodes: &mut HashMap<u64, OsmNode>,
    residential_polygons: &[MultiPolygon<f64>],
    military_polygons: &[MultiPolygon<f64>],
    bounds: &PbfBounds,
) {
    info!(
        "Computing proximity flags for {} nodes ({} residential, {} military polygons)",
        nodes.len(),
        residential_polygons.len(),
        military_polygons.len()
    );

    // Step 1: Build rasterized grid
    let mut rasterizer = AreaRasterizer::new(bounds);

    // Step 2: Rasterize residential polygons
    rasterizer.rasterize_residential(residential_polygons);

    // Step 3: Rasterize military polygons
    rasterizer.rasterize_military(military_polygons);

    // Step 4: Get completed grid
    let grid = rasterizer.into_grid();

    // Step 5: Apply flags to nodes
    apply_flags_to_nodes(nodes, &grid);
}

/// Apply pre-computed grid flags to nodes.
///
/// This function updates the `residential_in_proximity` and `nogo_area` flags
/// on each node based on the grid cell values at the node's location.
fn apply_flags_to_nodes(nodes: &mut HashMap<u64, OsmNode>, grid: &RasterizedProximityGrid) {
    let total_nodes = nodes.len();
    let progress_counter = AtomicUsize::new(0);
    let progress_interval = (total_nodes / 10).max(10000); // Report every 10%

    info!("Applying grid flags to {} nodes", total_nodes);

    // Collect node updates in parallel
    let updates: Vec<(u64, bool, bool)> = nodes
        .par_iter()
        .map(|(&node_id, node)| {
            let lat = node.lat as f32;
            let lon = node.lon as f32;

            let residential = grid.is_residential_proximity(lat, lon);
            let nogo = grid.is_nogo_area(lat, lon);

            // Update progress
            let count = progress_counter.fetch_add(1, Ordering::Relaxed) + 1;
            if count % progress_interval == 0 {
                info!(
                    "Flag application progress: {:.1}%",
                    (count as f64 / total_nodes as f64) * 100.0
                );
            }

            (node_id, residential, nogo)
        })
        .collect();

    // Apply updates
    let mut residential_count = 0;
    let mut nogo_count = 0;

    for (node_id, residential, nogo) in updates {
        if let Some(node) = nodes.get_mut(&node_id) {
            node.residential_in_proximity = residential;
            node.nogo_area = nogo;

            if residential {
                residential_count += 1;
            }
            if nogo {
                nogo_count += 1;
            }
        }
    }

    info!(
        "Flag application complete: {} nodes with residential proximity, {} nodes in nogo areas",
        residential_count, nogo_count
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::{Coord, LineString, Polygon};

    fn make_test_bounds() -> PbfBounds {
        PbfBounds {
            lat_min: Some(50.0),
            lat_max: Some(50.1),
            lon_min: Some(10.0),
            lon_max: Some(10.1),
        }
    }

    fn make_test_nodes() -> HashMap<u64, OsmNode> {
        let mut nodes = HashMap::new();

        // Add some nodes within the bounds
        for i in 0..100 {
            nodes.insert(
                i,
                OsmNode {
                    id: i,
                    lat: 50.0 + 0.001 * (i as f64),
                    lon: 10.0 + 0.001 * (i as f64),
                    residential_in_proximity: false,
                    nogo_area: false,
                },
            );
        }

        nodes
    }

    fn make_square_polygon(
        center_lat: f64,
        center_lon: f64,
        half_size_deg: f64,
    ) -> MultiPolygon<f64> {
        let coords = vec![
            Coord {
                x: center_lon - half_size_deg,
                y: center_lat - half_size_deg,
            },
            Coord {
                x: center_lon + half_size_deg,
                y: center_lat - half_size_deg,
            },
            Coord {
                x: center_lon + half_size_deg,
                y: center_lat + half_size_deg,
            },
            Coord {
                x: center_lon - half_size_deg,
                y: center_lat + half_size_deg,
            },
            Coord {
                x: center_lon - half_size_deg,
                y: center_lat - half_size_deg,
            },
        ];
        let polygon = Polygon::new(LineString::new(coords), vec![]);
        MultiPolygon::new(vec![polygon])
    }

    #[test]
    fn test_compute_proximity_flags_empty() {
        let mut nodes = make_test_nodes();
        let bounds = make_test_bounds();

        compute_proximity_flags(&mut nodes, &[], &[], &bounds);

        // With no polygons, all flags should be false
        for node in nodes.values() {
            assert!(!node.residential_in_proximity);
            assert!(!node.nogo_area);
        }
    }

    #[test]
    fn test_compute_proximity_flags_with_residential() {
        let mut nodes = make_test_nodes();
        let bounds = make_test_bounds();

        // Create a large residential polygon
        let residential = make_square_polygon(50.05, 10.05, 0.04);

        compute_proximity_flags(&mut nodes, &[residential], &[], &bounds);

        // Some nodes should have residential proximity
        let residential_count = nodes
            .values()
            .filter(|n| n.residential_in_proximity)
            .count();

        // With a large polygon, most nodes should be marked
        assert!(
            residential_count > 0,
            "Expected some nodes to have residential proximity"
        );
    }

    #[test]
    fn test_apply_flags_to_nodes_empty_grid() {
        let mut nodes = make_test_nodes();
        let bounds = make_test_bounds();
        let grid = RasterizedProximityGrid::new(&bounds);

        apply_flags_to_nodes(&mut nodes, &grid);

        // Empty grid means no flags set
        for node in nodes.values() {
            assert!(!node.residential_in_proximity);
            assert!(!node.nogo_area);
        }
    }

    #[test]
    fn test_nodes_outside_grid_not_affected() {
        let mut nodes = HashMap::new();

        // Add a node outside the grid bounds
        nodes.insert(
            1,
            OsmNode {
                id: 1,
                lat: 60.0, // Far outside 50.0-50.1 range
                lon: 20.0, // Far outside 10.0-10.1 range
                residential_in_proximity: true, // Pre-set to true
                nogo_area: true,
            },
        );

        let bounds = make_test_bounds();
        let grid = RasterizedProximityGrid::new(&bounds);

        apply_flags_to_nodes(&mut nodes, &grid);

        // Node outside grid should have flags set to false (grid lookup returns false for out-of-bounds)
        let node = nodes.get(&1).unwrap();
        assert!(!node.residential_in_proximity);
        assert!(!node.nogo_area);
    }
}
