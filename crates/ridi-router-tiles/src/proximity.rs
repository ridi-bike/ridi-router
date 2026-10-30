//! Proximity geometry used by the local stage: grid constants, polygon bbox/centroid,
//! bearings, the reference polygon distance and military containment.

use geo::{Contains, Distance, Haversine, HaversineClosestPoint, MultiPolygon, Point};

use crate::simd::haversine::min_distance_to_vertices;

/// Residential grid cell size: ~240m (0.0024 degrees at equator)
/// This preserves the original residential raster resolution and tile sizes.
pub const GRID_CELL_SIZE_DEG: f32 = 0.0024;

/// Military grid cell size: half the residential resolution.
/// This improves military edge accuracy without inflating residential sector storage.
pub const MILITARY_GRID_CELL_SIZE_DEG: f32 = 0.0012;

/// Search radius for residential proximity check (meters)
pub const RESIDENTIAL_PROXIMITY_THRESHOLD_M: f32 = 500.0;

/// Minimum fraction of search circle that must be covered by residential areas
pub const RESIDENTIAL_PART_COVERED: f32 = 0.10;

/// Computed threshold area (m²) = π * r² * coverage_fraction
/// This is approximately 78,540 m²
pub const THRESHOLD_AREA_M2: f32 = std::f32::consts::PI
    * RESIDENTIAL_PROXIMITY_THRESHOLD_M
    * RESIDENTIAL_PROXIMITY_THRESHOLD_M
    * RESIDENTIAL_PART_COVERED;

/// Convert a bearing angle (degrees) to sector index (0-7)
///
/// Bearing 0° = North, 90° = East, 180° = South, 270° = West
/// Sectors are centered on cardinal/ordinal directions:
/// - N (0): 337.5° to 22.5°
/// - NE (1): 22.5° to 67.5°
/// - etc.
#[inline]
pub fn bearing_to_sector(bearing_deg: f32) -> usize {
    // Normalize bearing to 0-360
    let b = ((bearing_deg % 360.0) + 360.0) % 360.0;
    // Add 22.5 to center sectors on cardinal directions, then divide by 45
    (((b + 22.5) / 45.0) as usize) % 8
}

/// Simple bounding box for quick spatial filtering.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BBox {
    pub(crate) lat_min: f32,
    pub(crate) lat_max: f32,
    pub(crate) lon_min: f32,
    pub(crate) lon_max: f32,
}

impl BBox {
    /// Check if a point could be within distance_m of this bbox.
    /// Uses a rough approximation: 1 degree ≈ 111km.
    #[inline]
    pub(crate) fn could_be_within(&self, lat: f32, lon: f32, distance_m: f32) -> bool {
        // Convert distance to degrees (conservative estimate)
        let deg_buffer = distance_m / 111_000.0 * 1.5; // 1.5x safety margin

        lat >= self.lat_min - deg_buffer
            && lat <= self.lat_max + deg_buffer
            && lon >= self.lon_min - deg_buffer
            && lon <= self.lon_max + deg_buffer
    }
}

/// Compute bounding box for a multipolygon.
pub(crate) fn compute_multipolygon_bbox(mp: &MultiPolygon<f64>) -> BBox {
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
pub(crate) fn compute_multipolygon_centroid(mp: &MultiPolygon<f64>) -> (f64, f64) {
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
pub(crate) fn calculate_bearing(from_lat: f32, from_lon: f32, to_lat: f64, to_lon: f64) -> f32 {
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

/// Reference distance (m) from a cell centre to a polygon: 0 inside, haversine closest point
/// otherwise, SIMD vertex distance when the closest point is indeterminate.
pub(crate) fn polygon_distance_m(polygon: &MultiPolygon<f64>, cell_lat: f32, cell_lon: f32) -> f64 {
    let geo_point = Point::new(cell_lon as f64, cell_lat as f64);
    match polygon.haversine_closest_point(&geo_point) {
        geo::Closest::Intersection(_) => 0.0, // Point is inside polygon
        geo::Closest::SinglePoint(p) => Haversine.distance(geo_point, p),
        geo::Closest::Indeterminate => {
            // Fall back to SIMD vertex distance
            let vertices: Vec<(f32, f32)> = polygon
                .iter()
                .flat_map(|p| p.exterior().coords())
                .map(|c| (c.y as f32, c.x as f32))
                .collect();

            min_distance_to_vertices(cell_lat, cell_lon, &vertices) as f64
        }
    }
}

/// Check if a point is inside a military polygon.
pub(crate) fn is_military_interior(
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
    fn test_calculate_bearing() {
        // Test cardinal directions from (50.0, 10.0)
        let lat = 50.0f32;
        let lon = 10.0f32;

        // North: higher latitude
        let bearing_n = calculate_bearing(lat, lon, 51.0, 10.0);
        assert!(
            !(10.0..=350.0).contains(&bearing_n),
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
        assert!((0.0..360.0).contains(&bearing));
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

    #[test]
    fn test_bearing_to_sector() {
        // Test cardinal directions
        assert_eq!(bearing_to_sector(0.0), 0); // N
        assert_eq!(bearing_to_sector(45.0), 1); // NE
        assert_eq!(bearing_to_sector(90.0), 2); // E
        assert_eq!(bearing_to_sector(135.0), 3); // SE
        assert_eq!(bearing_to_sector(180.0), 4); // S
        assert_eq!(bearing_to_sector(225.0), 5); // SW
        assert_eq!(bearing_to_sector(270.0), 6); // W
        assert_eq!(bearing_to_sector(315.0), 7); // NW

        // Test boundary cases
        assert_eq!(bearing_to_sector(22.4), 0); // Just before N/NE boundary
        assert_eq!(bearing_to_sector(22.6), 1); // Just after N/NE boundary
        assert_eq!(bearing_to_sector(337.4), 7); // Just before NW/N boundary
        assert_eq!(bearing_to_sector(337.6), 0); // Just after NW/N boundary

        // Test negative and >360 values normalize correctly
        assert_eq!(bearing_to_sector(-45.0), 7); // -45° = 315° = NW
        assert_eq!(bearing_to_sector(405.0), 1); // 405° = 45° = NE
    }

    #[test]
    fn test_bearing_to_sector_all_ordinals() {
        // Test ordinal directions (NE, SE, SW, NW)
        assert_eq!(bearing_to_sector(45.0), 1); // NE
        assert_eq!(bearing_to_sector(135.0), 3); // SE
        assert_eq!(bearing_to_sector(225.0), 5); // SW
        assert_eq!(bearing_to_sector(315.0), 7); // NW

        // Test boundaries between all sectors
        // N/NE boundary at 22.5
        assert_eq!(bearing_to_sector(22.49), 0);
        assert_eq!(bearing_to_sector(22.51), 1);

        // NE/E boundary at 67.5
        assert_eq!(bearing_to_sector(67.49), 1);
        assert_eq!(bearing_to_sector(67.51), 2);

        // E/SE boundary at 112.5
        assert_eq!(bearing_to_sector(112.49), 2);
        assert_eq!(bearing_to_sector(112.51), 3);

        // SE/S boundary at 157.5
        assert_eq!(bearing_to_sector(157.49), 3);
        assert_eq!(bearing_to_sector(157.51), 4);

        // S/SW boundary at 202.5
        assert_eq!(bearing_to_sector(202.49), 4);
        assert_eq!(bearing_to_sector(202.51), 5);

        // SW/W boundary at 247.5
        assert_eq!(bearing_to_sector(247.49), 5);
        assert_eq!(bearing_to_sector(247.51), 6);

        // W/NW boundary at 292.5
        assert_eq!(bearing_to_sector(292.49), 6);
        assert_eq!(bearing_to_sector(292.51), 7);

        // NW/N boundary at 337.5
        assert_eq!(bearing_to_sector(337.49), 7);
        assert_eq!(bearing_to_sector(337.51), 0);
    }

    #[test]
    fn test_bearing_to_sector_large_values() {
        // Values > 720 degrees
        assert_eq!(bearing_to_sector(720.0), 0); // 720 = 2 * 360 = 0 = N
        assert_eq!(bearing_to_sector(765.0), 1); // 765 = 720 + 45 = 45 = NE
        assert_eq!(bearing_to_sector(810.0), 2); // 810 = 720 + 90 = 90 = E
                                                 // Actually 900 % 360 = 180, which is S (sector 4)
        assert_eq!(bearing_to_sector(900.0), 4);

        // Very negative values
        assert_eq!(bearing_to_sector(-360.0), 0); // -360 = 0 = N
        assert_eq!(bearing_to_sector(-405.0), 7); // -405 = -360 - 45 = -45 -> 315 = NW
    }
}
