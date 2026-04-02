//! Polygon to grid rasterization with SIMD acceleration.
//!
//! Converts residential and military polygons into the rasterized proximity grid
//! by computing values for each grid cell using directional sectors.

use geo::{Contains, Distance, GeodesicArea, Haversine, HaversineClosestPoint, MultiPolygon, Point};
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::info;

use crate::osm_data::in_memory_pbf::PbfBounds;
use crate::simd::haversine::min_distance_to_vertices;

use super::rasterized_grid::{
    bearing_to_sector, MilitaryStatus, RasterizedProximityGrid, MILITARY_GRID_CELL_SIZE_DEG,
    RESIDENTIAL_PROXIMITY_THRESHOLD_M,
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
    /// that are within `RESIDENTIAL_PROXIMITY_THRESHOLD_M` meters of the cell center,
    /// distributed into 8 directional sectors.
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

        // Pre-compute polygon centroids, areas, and bounding boxes for quick filtering
        let polygon_data: Vec<PolygonData> = polygons
            .iter()
            .map(|mp| {
                let area = mp.geodesic_area_signed().abs() as f32;
                let bbox = compute_multipolygon_bbox(mp);
                let centroid = compute_multipolygon_centroid(mp);
                PolygonData {
                    polygon: mp,
                    area,
                    bbox,
                    centroid,
                }
            })
            .collect();

        // Process cells in parallel
        // Returns sector data for each cell that has residential area
        let cell_results: Vec<(usize, [f32; 8])> = (0..cell_count)
            .into_par_iter()
            .filter_map(|cell_idx| {
                let (lat, lon) = self.grid.cell_center(cell_idx);

                // Compute residential area by sector
                let sectors = compute_residential_sectors_for_cell(
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

                // Only return if any sector has data
                if sectors.iter().any(|&a| a > 0.0) {
                    Some((cell_idx, sectors))
                } else {
                    None
                }
            })
            .collect();

        // Apply results to grid
        let cells = self.grid.cells_mut();
        for (idx, sectors) in cell_results {
            cells[idx].residential_sectors = sectors;
        }

        info!(
            "Residential rasterization complete: {} cells have residential area",
            self.grid.count_residential()
        );
    }

    /// Rasterize military polygons into the grid.
    ///
    /// Marks cells as nogo if all 5 sample points in the cell are inside a military polygon.
    pub fn rasterize_military(&mut self, polygons: &[MultiPolygon<f64>]) {
        if polygons.is_empty() {
            info!("No military polygons to rasterize");
            return;
        }

        info!(
            "Rasterizing {} military polygons into {} grid cells",
            polygons.len(),
            self.grid.military_cell_count()
        );

        let cell_count = self.grid.military_cell_count();
        let progress_counter = AtomicUsize::new(0);
        let progress_interval = (cell_count / 20).max(10000);

        // Pre-compute polygon bounding boxes
        let polygon_bboxes: Vec<(&MultiPolygon<f64>, BBox)> = polygons
            .iter()
            .map(|mp| (mp, compute_multipolygon_bbox(mp)))
            .collect();

        // Debug: Test if a specific point inside polygon 334 returns true
        // Polygon 334 bbox: lat 57.1847-57.2226, lon 24.4875-24.5443
        let test_point = geo::Point::new(24.5, 57.2); // Should be inside polygon 334
        use geo::prelude::{Centroid, Contains};
        for (i, (mp, bbox)) in polygon_bboxes.iter().enumerate() {
            if bbox.lat_min < 57.2
                && bbox.lat_max > 57.2
                && bbox.lon_min < 24.5
                && bbox.lon_max > 24.5
            {
                let contains = mp.contains(&test_point);
                let closest = mp.haversine_closest_point(&test_point);

                // Also test the centroid of the polygon
                let centroid = mp.centroid();
                let centroid_contains = centroid.map(|c| mp.contains(&c)).unwrap_or(false);

                // Count interior rings (holes)
                let num_holes: usize = mp.iter().map(|p| p.interiors().len()).sum();

                info!(
                    "Polygon {}: contains={}, closest={:?}, centroid_contains={}, holes={}, bbox: [{:.4},{:.4}] to [{:.4},{:.4}]",
                    i, contains, matches!(closest, geo::Closest::Intersection(_)),
                    centroid_contains, num_holes,
                    bbox.lat_min, bbox.lon_min, bbox.lat_max, bbox.lon_max
                );
            }
        }

        // Debug: Log information about first few military polygons
        for (i, (mp, bbox)) in polygon_bboxes.iter().enumerate().take(5) {
            let total_coords: usize = mp.iter().map(|p| p.exterior().0.len()).sum();
            let num_polygons = mp.0.len();
            info!(
                "Military polygon {}: {} constituent polygons, {} total coords, bbox: [{:.4}, {:.4}] to [{:.4}, {:.4}]",
                i, num_polygons, total_coords, bbox.lat_min, bbox.lon_min, bbox.lat_max, bbox.lon_max
            );
        }
        // Debug: Count polygons that overlap with tile_2044_1471 (lat 57.1-57.2, lon 24.4-24.5)
        let tile_lat_min = 57.1f32;
        let tile_lat_max = 57.2f32;
        let tile_lon_min = 24.4f32;
        let tile_lon_max = 24.5f32;
        let mut overlapping_count = 0;
        for (i, (mp, bbox)) in polygon_bboxes.iter().enumerate() {
            if bbox.lat_max >= tile_lat_min
                && bbox.lat_min <= tile_lat_max
                && bbox.lon_max >= tile_lon_min
                && bbox.lon_min <= tile_lon_max
            {
                overlapping_count += 1;
                let total_coords: usize = mp.iter().map(|p| p.exterior().0.len()).sum();
                info!(
                    "Overlapping military polygon {}: {} coords, bbox: [{:.4}, {:.4}] to [{:.4}, {:.4}]",
                    i, total_coords, bbox.lat_min, bbox.lon_min, bbox.lat_max, bbox.lon_max
                );
            }
        }
        info!(
            "Military polygons overlapping tile_2044_1471 (lat 57.1-57.2, lon 24.4-24.5): {} of {}",
            overlapping_count,
            polygon_bboxes.len()
        );

        // Process cells in parallel using 5-point sampling
        let nogo_cells: Vec<(usize, MilitaryStatus)> = (0..cell_count)
            .into_par_iter()
            .filter_map(|cell_idx| {
                // Check military status using 5-point sampling
                let status = check_military_status(&self.grid, cell_idx, &polygon_bboxes);

                // Update progress
                let count = progress_counter.fetch_add(1, Ordering::Relaxed) + 1;
                if count % progress_interval == 0 {
                    info!(
                        "Military rasterization progress: {:.1}%",
                        (count as f64 / cell_count as f64) * 100.0
                    );
                }

                // Only return Full status cells as nogo
                if status == MilitaryStatus::Full {
                    Some((cell_idx, status))
                } else {
                    None
                }
            })
            .collect();

        // Apply results to military grid
        let cells = self.grid.military_cells_mut();
        for (idx, status) in nogo_cells {
            cells[idx] = status;
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
}

/// Pre-computed data for efficient polygon processing.
struct PolygonData<'a> {
    polygon: &'a MultiPolygon<f64>,
    area: f32,
    bbox: BBox,
    centroid: (f64, f64), // (lat, lon)
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

/// Compute centroid of a multipolygon (simple average of all polygon centroids).
fn compute_multipolygon_centroid(mp: &MultiPolygon<f64>) -> (f64, f64) {
    let mut total_lat = 0.0;
    let mut total_lon = 0.0;
    let mut count = 0;

    for polygon in mp.iter() {
        for coord in polygon.exterior().coords() {
            total_lat += coord.y;
            total_lon += coord.x;
            count += 1;
        }
    }

    if count > 0 {
        (total_lat / count as f64, total_lon / count as f64)
    } else {
        (0.0, 0.0)
    }
}

/// Calculate bearing from one point to another (in degrees).
/// Returns angle where 0° = North, 90° = East, 180° = South, 270° = West.
fn calculate_bearing(from_lat: f32, from_lon: f32, to_lat: f64, to_lon: f64) -> f32 {
    let from_lat = from_lat as f64;
    let from_lon = from_lon as f64;

    let delta_lon = (to_lon - from_lon).to_radians();
    let from_lat_rad = from_lat.to_radians();
    let to_lat_rad = to_lat.to_radians();

    let x = delta_lon.sin() * to_lat_rad.cos();
    let y = from_lat_rad.cos() * to_lat_rad.sin()
        - from_lat_rad.sin() * to_lat_rad.cos() * delta_lon.cos();

    let bearing = x.atan2(y).to_degrees(); // Note: x, y order for compass bearing (0° = North)
                                           // Normalize to [0, 360)
    (((bearing % 360.0) + 360.0) % 360.0) as f32
}

/// Compute residential area by directional sector within threshold distance of a cell center.
/// Compute residential area by directional sector within threshold distance of a cell center.
fn compute_residential_sectors_for_cell(
    cell_lat: f32,
    cell_lon: f32,
    polygons: &[PolygonData],
    threshold_m: f32,
) -> [f32; 8] {
    let geo_point = Point::new(cell_lon as f64, cell_lat as f64);
    let mut sectors = [0.0f32; 8];

    for poly_data in polygons {
        // Quick bbox check first
        if !poly_data
            .bbox
            .could_be_within(cell_lat, cell_lon, threshold_m)
        {
            continue;
        }

        // Compute actual distance to polygon
        let distance = match poly_data.polygon.haversine_closest_point(&geo_point) {
            geo::Closest::Intersection(_) => 0.0, // Point is inside polygon
            geo::Closest::SinglePoint(p) => Haversine.distance(geo_point, p),
            geo::Closest::Indeterminate => {
                // Fall back to SIMD vertex distance
                // Fall back to SIMD vertex distance
                let vertices: Vec<(f32, f32)> = poly_data
                    .polygon
                    .iter()
                    .flat_map(|p| p.exterior().coords())
                    .map(|c| (c.y as f32, c.x as f32))
                    .collect();

                min_distance_to_vertices(cell_lat, cell_lon, &vertices) as f64
            }
        };

        if distance <= threshold_m as f64 {
            // Calculate bearing from cell to polygon centroid
            let bearing = calculate_bearing(
                cell_lat,
                cell_lon,
                poly_data.centroid.0,
                poly_data.centroid.1,
            );

            // Assign to appropriate sector
            let sector_idx = bearing_to_sector(bearing);
            sectors[sector_idx] += poly_data.area;
        }
    }

    sectors
}

/// Get 5 sample points for a military cell (center + 4 corners)
///
/// Returns: [(lat, lon); 5] where indices are: 0=Center, 1=SW, 2=SE, 3=NW, 4=NE
fn sample_cell_points(grid: &RasterizedProximityGrid, cell_idx: usize) -> [(f32, f32); 5] {
    let (center_lat, center_lon) = grid.military_cell_center(cell_idx);
    let half_size = MILITARY_GRID_CELL_SIZE_DEG / 2.0;

    [
        (center_lat, center_lon),                         // Center
        (center_lat - half_size, center_lon - half_size), // SW
        (center_lat - half_size, center_lon + half_size), // SE
        (center_lat + half_size, center_lon - half_size), // NW
        (center_lat + half_size, center_lon + half_size), // NE
    ]
}

/// Check military status of a cell using 5-point sampling
///
/// Returns MilitaryStatus based on how many of the 5 sample points
/// are inside a military polygon:
/// - 0 points: None
/// - 1-4 points: Partial
/// - 5 points: Full
fn check_military_status(
    grid: &RasterizedProximityGrid,
    cell_idx: usize,
    polygon_bboxes: &[(&MultiPolygon<f64>, BBox)],
) -> MilitaryStatus {
    let points = sample_cell_points(grid, cell_idx);
    let mut interior_count = 0;

    for (lat, lon) in points {
        if is_military_interior(lat, lon, polygon_bboxes) {
            interior_count += 1;
        }
    }

    match interior_count {
        0 => MilitaryStatus::None,
        5 => MilitaryStatus::Full,
        _ => MilitaryStatus::Partial,
    }
}

/// Check if a point is inside a military polygon.
fn is_military_interior(
    lat: f32,
    lon: f32,
    polygon_bboxes: &[(&MultiPolygon<f64>, BBox)],
) -> bool {
    let geo_point = Point::new(lon as f64, lat as f64);

    for (polygon, bbox) in polygon_bboxes {
        // Quick bbox check
        if !bbox.could_be_within(lat, lon, 0.0) {
            continue;
        }

        if polygon.contains(&geo_point) {
            return true;
        }
    }

    false
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
    fn test_rasterize_sector_assignment() {
        // Create a small grid with polygons in specific directions
        let bounds = PbfBounds {
            lat_min: Some(50.0),
            lat_max: Some(50.01),
            lon_min: Some(10.0),
            lon_max: Some(10.01),
        };

        let mut rasterizer = AreaRasterizer::new(&bounds);

        // Create polygon NORTH of center (higher latitude)
        // Center is at 50.005, 10.005
        let polygon_north = make_square_polygon(50.008, 10.005, 0.001);
        rasterizer.rasterize_residential(&[polygon_north]);

        let grid = rasterizer.into_grid();

        // Find cells that have residential area and check sectors
        let mut found_north_sector = false;
        for cell in grid.cells().iter() {
            let total: f32 = cell.residential_sectors.iter().sum();
            if total > 0.0 {
                // Check that sector 0 (N) has the most area
                if cell.residential_sectors[0] > 0.0 {
                    found_north_sector = true;
                    break;
                }
            }
        }

        assert!(
            found_north_sector,
            "Expected some cells with data in North sector"
        );
    }

    #[test]
    fn test_into_grid() {
        let bounds = make_test_bounds();
        let rasterizer = AreaRasterizer::new(&bounds);

        let grid = rasterizer.into_grid();
        assert!(grid.cell_count() > 0);
    }

    #[test]
    fn test_calculate_bearing() {
        // Test cardinal directions from (50.0, 10.0)
        let lat = 50.0f32;
        let lon = 10.0f32;

        // North: higher latitude
        let bearing_n = calculate_bearing(lat, lon, 51.0, 10.0);
        assert!(
            bearing_n < 10.0 || bearing_n > 350.0,
            "North bearing should be ~0°, got {}",
            bearing_n
        );

        // East: higher longitude
        let bearing_e = calculate_bearing(lat, lon, 50.0, 11.0);
        assert!(
            (bearing_e - 90.0).abs() < 10.0,
            "East bearing should be ~90°, got {}",
            bearing_e
        );

        // South: lower latitude
        let bearing_s = calculate_bearing(lat, lon, 49.0, 10.0);
        assert!(
            (bearing_s - 180.0).abs() < 10.0,
            "South bearing should be ~180°, got {}",
            bearing_s
        );

        // West: lower longitude
        let bearing_w = calculate_bearing(lat, lon, 50.0, 9.0);
        assert!(
            (bearing_w - 270.0).abs() < 10.0,
            "West bearing should be ~270°, got {}",
            bearing_w
        );
    }

    #[test]
    fn test_calculate_bearing_diagonals() {
        let lat = 50.0f32;
        let lon = 10.0f32;

        // NE: higher lat and lon
        let bearing_ne = calculate_bearing(lat, lon, 51.0, 11.0);
        assert!(
            (bearing_ne - 32.0).abs() < 5.0,
            "NE bearing should be ~32°, got {}",
            bearing_ne
        );

        // SE: lower lat, higher lon
        let bearing_se = calculate_bearing(lat, lon, 49.0, 11.0);
        assert!(
            (bearing_se - 148.0).abs() < 5.0,
            "SE bearing should be ~148°, got {}",
            bearing_se
        );

        // SW: lower lat and lon
        let bearing_sw = calculate_bearing(lat, lon, 49.0, 9.0);
        assert!(
            (bearing_sw - 212.0).abs() < 5.0,
            "SW bearing should be ~212°, got {}",
            bearing_sw
        );

        // NW: higher lat, lower lon
        let bearing_nw = calculate_bearing(lat, lon, 51.0, 9.0);
        assert!(
            (bearing_nw - 328.0).abs() < 5.0,
            "NW bearing should be ~328°, got {}",
            bearing_nw
        );
    }

    #[test]
    fn test_calculate_bearing_same_point() {
        // Same point should still give a result (degenerate case)
        let bearing = calculate_bearing(50.0, 10.0, 50.0, 10.0);
        // Result is undefined but shouldn't panic
        assert!(bearing >= 0.0 && bearing < 360.0);
    }

    #[test]
    fn test_calculate_bearing_across_antimeridian() {
        // Test bearing calculation across the antimeridian
        // From 179°E to 179°W (crossing 180°)
        let bearing = calculate_bearing(0.0, 179.0, 0.0, -179.0);
        // Should be ~90° (east)
        assert!(
            (bearing - 90.0).abs() < 10.0,
            "Bearing east across antimeridian should be ~90°, got {}",
            bearing
        );

        // From 179°W to 179°E
        let bearing = calculate_bearing(0.0, -179.0, 0.0, 179.0);
        // Should be ~270° (west)
        assert!(
            (bearing - 270.0).abs() < 10.0,
            "Bearing west across antimeridian should be ~270°, got {}",
            bearing
        );
    }
}
