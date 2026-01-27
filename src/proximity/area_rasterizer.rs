//! Polygon to grid rasterization with SIMD acceleration.
//!
//! Converts residential and military polygons into the rasterized proximity grid
//! by computing values for each grid cell.

use geo::{Coord, Distance, GeodesicArea, Haversine, HaversineClosestPoint, MultiPolygon, Point};
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::info;

use crate::osm_data::in_memory_pbf::PbfBounds;
use crate::simd::haversine::haversine_batch;

use super::rasterized_grid::{
    RasterizedProximityGrid, MILITARY_INTERIOR_M, RESIDENTIAL_PROXIMITY_THRESHOLD_M,
};

/// Rasterizer that converts area polygons into grid cell values.
pub struct AreaRasterizer {
    grid: RasterizedProximityGrid,
}

impl AreaRasterizer {
    /// Create a new rasterizer with an empty grid covering the given bounds.
    pub fn new(bounds: &PbfBounds) -> Self {
        Self {
            grid: RasterizedProximityGrid::new(bounds),
        }
    }

    /// Rasterize residential polygons into the grid.
    ///
    /// For each grid cell, computes the total area of residential polygons
    /// that are within `RESIDENTIAL_PROXIMITY_THRESHOLD_M` meters of the cell center.
    ///
    /// Uses SIMD for distance calculations and parallel processing across cells.
    pub fn rasterize_residential(&mut self, polygons: &[MultiPolygon<f64>]) {
        if polygons.is_empty() {
            info!("No residential polygons to rasterize");
            return;
        }

        info!(
            "Rasterizing {} residential polygons into {} grid cells",
            polygons.len(),
            self.grid.cell_count()
        );

        let cell_count = self.grid.cell_count();
        let progress_counter = AtomicUsize::new(0);
        let progress_interval = (cell_count / 20).max(10000); // Report every 5% or 10k cells

        // Pre-compute polygon centroids and bounding boxes for quick filtering
        let polygon_data: Vec<PolygonData> = polygons
            .iter()
            .map(|mp| {
                let area = mp.geodesic_area_signed().abs() as f32;
                let bbox = compute_multipolygon_bbox(mp);
                PolygonData {
                    polygon: mp,
                    area,
                    bbox,
                }
            })
            .collect();

        // Process cells in parallel
        // We need to collect results and then apply them since we can't mutate grid in parallel
        let cell_results: Vec<(usize, f32)> = (0..cell_count)
            .into_par_iter()
            .filter_map(|cell_idx| {
                let (lat, lon) = self.grid.cell_center(cell_idx);

                // Compute total residential area within threshold distance
                let total_area = compute_residential_area_for_cell(
                    lat,
                    lon,
                    &polygon_data,
                    RESIDENTIAL_PROXIMITY_THRESHOLD_M,
                );

                // Update progress
                let count = progress_counter.fetch_add(1, Ordering::Relaxed) + 1;
                if count % progress_interval == 0 {
                    info!(
                        "Residential rasterization progress: {:.1}%",
                        (count as f64 / cell_count as f64) * 100.0
                    );
                }

                if total_area > 0.0 {
                    Some((cell_idx, total_area))
                } else {
                    None
                }
            })
            .collect();

        // Apply results to grid
        let cells = self.grid.cells_mut();
        for (idx, area) in cell_results {
            cells[idx].residential_area_m2 = area;
        }

        info!(
            "Residential rasterization complete: {} cells have residential area",
            self.grid.count_residential()
        );
    }

    /// Rasterize military polygons into the grid.
    ///
    /// Marks cells as nogo if their center is >100m inside a military polygon boundary.
    pub fn rasterize_military(&mut self, polygons: &[MultiPolygon<f64>]) {
        if polygons.is_empty() {
            info!("No military polygons to rasterize");
            return;
        }

        info!(
            "Rasterizing {} military polygons into {} grid cells",
            polygons.len(),
            self.grid.cell_count()
        );

        let cell_count = self.grid.cell_count();
        let progress_counter = AtomicUsize::new(0);
        let progress_interval = (cell_count / 20).max(10000);

        // Pre-compute polygon bounding boxes
        let polygon_bboxes: Vec<(&MultiPolygon<f64>, BBox)> = polygons
            .iter()
            .map(|mp| (mp, compute_multipolygon_bbox(mp)))
            .collect();

        // Process cells in parallel
        let nogo_cells: Vec<usize> = (0..cell_count)
            .into_par_iter()
            .filter_map(|cell_idx| {
                let (lat, lon) = self.grid.cell_center(cell_idx);

                // Check if this cell is inside a military area >100m from boundary
                let is_interior =
                    is_military_interior(lat, lon, &polygon_bboxes, MILITARY_INTERIOR_M);

                // Update progress
                let count = progress_counter.fetch_add(1, Ordering::Relaxed) + 1;
                if count % progress_interval == 0 {
                    info!(
                        "Military rasterization progress: {:.1}%",
                        (count as f64 / cell_count as f64) * 100.0
                    );
                }

                if is_interior {
                    Some(cell_idx)
                } else {
                    None
                }
            })
            .collect();

        // Apply results to grid
        let cells = self.grid.cells_mut();
        for idx in nogo_cells {
            cells[idx].is_military_interior = true;
        }

        info!(
            "Military rasterization complete: {} cells marked as nogo",
            self.grid.count_nogo()
        );
    }

    /// Consume the rasterizer and return the completed grid.
    pub fn into_grid(self) -> RasterizedProximityGrid {
        self.grid
    }

    /// Get a reference to the grid for inspection.
    pub fn grid(&self) -> &RasterizedProximityGrid {
        &self.grid
    }
}

/// Pre-computed data for efficient polygon processing.
struct PolygonData<'a> {
    polygon: &'a MultiPolygon<f64>,
    area: f32,
    bbox: BBox,
}

/// Simple bounding box for quick spatial filtering.
#[derive(Clone, Copy, Debug)]
struct BBox {
    lat_min: f32,
    lat_max: f32,
    lon_min: f32,
    lon_max: f32,
}

impl BBox {
    /// Check if a point could be within distance_m of this bbox.
    /// Uses a rough approximation: 1 degree ≈ 111km.
    #[inline]
    fn could_be_within(&self, lat: f32, lon: f32, distance_m: f32) -> bool {
        // Convert distance to degrees (conservative estimate)
        let deg_buffer = distance_m / 111_000.0 * 1.5; // 1.5x safety margin

        lat >= self.lat_min - deg_buffer
            && lat <= self.lat_max + deg_buffer
            && lon >= self.lon_min - deg_buffer
            && lon <= self.lon_max + deg_buffer
    }
}

/// Compute bounding box for a multipolygon.
fn compute_multipolygon_bbox(mp: &MultiPolygon<f64>) -> BBox {
    let mut lat_min = f64::MAX;
    let mut lat_max = f64::MIN;
    let mut lon_min = f64::MAX;
    let mut lon_max = f64::MIN;

    for polygon in mp.iter() {
        for coord in polygon.exterior().coords() {
            lat_min = lat_min.min(coord.y);
            lat_max = lat_max.max(coord.y);
            lon_min = lon_min.min(coord.x);
            lon_max = lon_max.max(coord.x);
        }
    }

    BBox {
        lat_min: lat_min as f32,
        lat_max: lat_max as f32,
        lon_min: lon_min as f32,
        lon_max: lon_max as f32,
    }
}

/// Compute total residential area within threshold distance of a cell center.
fn compute_residential_area_for_cell(
    cell_lat: f32,
    cell_lon: f32,
    polygons: &[PolygonData],
    threshold_m: f32,
) -> f32 {
    let geo_point = Point::new(cell_lon as f64, cell_lat as f64);
    let mut total_area = 0.0f32;

    for poly_data in polygons {
        // Quick bbox check first
        if !poly_data.bbox.could_be_within(cell_lat, cell_lon, threshold_m) {
            continue;
        }

        // Compute actual distance to polygon
        let distance = match poly_data.polygon.haversine_closest_point(&geo_point) {
            geo::Closest::Intersection(_) => 0.0, // Point is inside polygon
            geo::Closest::SinglePoint(p) => Haversine.distance(geo_point, p),
            geo::Closest::Indeterminate => {
                // Fall back to SIMD vertex distance
                let vertices: Vec<(f32, f32)> = poly_data
                    .polygon
                    .iter()
                    .flat_map(|p| p.exterior().coords())
                    .map(|c| (c.y as f32, c.x as f32))
                    .collect();

                if vertices.is_empty() {
                    continue;
                }

                let distances = haversine_batch(cell_lat, cell_lon, &vertices);
                distances.into_iter().fold(f64::MAX, |min, d| {
                    if (d as f64) < min {
                        d as f64
                    } else {
                        min
                    }
                })
            }
        };

        if distance <= threshold_m as f64 {
            total_area += poly_data.area;
        }
    }

    total_area
}

/// Check if a point is inside a military polygon >100m from the boundary.
fn is_military_interior(
    lat: f32,
    lon: f32,
    polygon_bboxes: &[(&MultiPolygon<f64>, BBox)],
    interior_threshold_m: f32,
) -> bool {
    let geo_point = Point::new(lon as f64, lat as f64);

    for (polygon, bbox) in polygon_bboxes {
        // Quick bbox check - must be well inside for interior check
        if !bbox.could_be_within(lat, lon, 0.0) {
            continue;
        }

        // Check if point is inside and far enough from boundary
        match polygon.haversine_closest_point(&geo_point) {
            geo::Closest::Intersection(closest_boundary_point) => {
                // Point is inside - check distance to boundary
                let distance_to_boundary = Haversine.distance(geo_point, closest_boundary_point);
                if distance_to_boundary > interior_threshold_m as f64 {
                    return true;
                }
            }
            geo::Closest::SinglePoint(_) => {
                // Point is outside polygon
            }
            geo::Closest::Indeterminate => {
                // Can't determine - be conservative and don't mark as nogo
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::{LineString, Polygon};

    fn make_test_bounds() -> PbfBounds {
        PbfBounds {
            lat_min: Some(50.0),
            lat_max: Some(50.1),
            lon_min: Some(10.0),
            lon_max: Some(10.1),
        }
    }

    fn make_square_polygon(center_lat: f64, center_lon: f64, half_size_deg: f64) -> MultiPolygon<f64> {
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
            }, // Close the ring
        ];
        let polygon = Polygon::new(LineString::new(coords), vec![]);
        MultiPolygon::new(vec![polygon])
    }

    #[test]
    fn test_rasterizer_creation() {
        let bounds = make_test_bounds();
        let rasterizer = AreaRasterizer::new(&bounds);

        assert!(rasterizer.grid.cell_count() > 0);
    }

    #[test]
    fn test_rasterize_empty_polygons() {
        let bounds = make_test_bounds();
        let mut rasterizer = AreaRasterizer::new(&bounds);

        rasterizer.rasterize_residential(&[]);
        rasterizer.rasterize_military(&[]);

        assert_eq!(rasterizer.grid.count_residential(), 0);
        assert_eq!(rasterizer.grid.count_nogo(), 0);
    }

    #[test]
    fn test_bbox_could_be_within() {
        let bbox = BBox {
            lat_min: 50.0,
            lat_max: 50.1,
            lon_min: 10.0,
            lon_max: 10.1,
        };

        // Inside bbox
        assert!(bbox.could_be_within(50.05, 10.05, 1000.0));

        // Just outside but within distance
        assert!(bbox.could_be_within(50.15, 10.05, 100000.0)); // 100km

        // Far outside
        assert!(!bbox.could_be_within(60.0, 20.0, 1000.0));
    }

    #[test]
    fn test_compute_multipolygon_bbox() {
        let mp = make_square_polygon(50.05, 10.05, 0.01);
        let bbox = compute_multipolygon_bbox(&mp);

        assert!((bbox.lat_min - 50.04).abs() < 0.001);
        assert!((bbox.lat_max - 50.06).abs() < 0.001);
        assert!((bbox.lon_min - 10.04).abs() < 0.001);
        assert!((bbox.lon_max - 10.06).abs() < 0.001);
    }

    #[test]
    fn test_rasterize_single_residential_polygon() {
        let bounds = PbfBounds {
            lat_min: Some(50.0),
            lat_max: Some(50.02),
            lon_min: Some(10.0),
            lon_max: Some(10.02),
        };

        let mut rasterizer = AreaRasterizer::new(&bounds);

        // Create a polygon at the center
        let polygon = make_square_polygon(50.01, 10.01, 0.005);
        rasterizer.rasterize_residential(&[polygon]);

        // Some cells near the polygon should have residential area
        // The exact count depends on grid resolution and polygon size
        let residential_count = rasterizer.grid.count_residential();
        info!("Residential cells: {}", residential_count);

        // Should have at least some residential cells
        assert!(residential_count > 0, "Expected some residential cells");
    }

    #[test]
    fn test_into_grid() {
        let bounds = make_test_bounds();
        let rasterizer = AreaRasterizer::new(&bounds);

        let grid = rasterizer.into_grid();
        assert!(grid.cell_count() > 0);
    }
}
