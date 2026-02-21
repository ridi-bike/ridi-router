//! SIMD-accelerated haversine distance calculations.
//!
//! Provides vectorized haversine distance computations using AVX2 (f32x8)
//! and optionally AVX-512 (f32x16) for calculating distances between
//! geographic coordinates.
//!
//! The haversine formula calculates the great-circle distance between two points
//! on a sphere given their longitudes and latitudes.

use wide::f32x8;

use super::trig_approx::{asin_f32x8, cos_f32x8, sin_f32x8};

/// Earth's mean radius in meters
pub const EARTH_RADIUS_M: f32 = 6_371_000.0;

/// Degrees to radians conversion factor
const DEG_TO_RAD: f32 = std::f32::consts::PI / 180.0;

/// Compute haversine distance from one reference point to 8 target points.
///
/// Uses the haversine formula:
/// ```text
/// a = sin²(Δlat/2) + cos(lat1) * cos(lat2) * sin²(Δlon/2)
/// c = 2 * asin(sqrt(a))
/// d = R * c
/// ```
///
/// # Arguments
///
/// * `ref_lat` - Reference point latitude in degrees
/// * `ref_lon` - Reference point longitude in degrees
/// * `target_lats` - 8 target latitudes in degrees
/// * `target_lons` - 8 target longitudes in degrees
///
/// # Returns
///
/// 8 distances in meters
#[inline]
pub fn haversine_f32x8(
    ref_lat: f32,
    ref_lon: f32,
    target_lats: f32x8,
    target_lons: f32x8,
) -> f32x8 {
    let deg_to_rad = f32x8::splat(DEG_TO_RAD);
    let two = f32x8::splat(2.0);
    let earth_radius = f32x8::splat(EARTH_RADIUS_M);

    // Convert to radians
    let ref_lat_rad = f32x8::splat(ref_lat * DEG_TO_RAD);
    let ref_lon_rad = f32x8::splat(ref_lon * DEG_TO_RAD);
    let target_lats_rad = target_lats * deg_to_rad;
    let target_lons_rad = target_lons * deg_to_rad;

    // Compute deltas
    let dlat = target_lats_rad - ref_lat_rad;
    let dlon = target_lons_rad - ref_lon_rad;

    // Haversine formula: a = sin²(Δlat/2) + cos(lat1) * cos(lat2) * sin²(Δlon/2)
    let half_dlat = dlat / two;
    let half_dlon = dlon / two;

    let sin_half_dlat = sin_f32x8(half_dlat);
    let sin_half_dlon = sin_f32x8(half_dlon);

    let cos_ref_lat = cos_f32x8(ref_lat_rad);
    let cos_target_lats = cos_f32x8(target_lats_rad);

    let a = sin_half_dlat * sin_half_dlat
        + cos_ref_lat * cos_target_lats * sin_half_dlon * sin_half_dlon;

    // c = 2 * asin(sqrt(a))
    // Clamp a to [0, 1] to handle numerical errors
    let a_clamped = a.max(f32x8::splat(0.0)).min(f32x8::splat(1.0));
    let sqrt_a = a_clamped.sqrt();
    let c = two * asin_f32x8(sqrt_a);

    // d = R * c
    earth_radius * c
}

/// Compute haversine distance from one reference point to 16 target points (AVX-512).
#[cfg(target_feature = "avx512f")]
#[inline]
pub fn haversine_f32x16(
    ref_lat: f32,
    ref_lon: f32,
    target_lats: wide::f32x16,
    target_lons: wide::f32x16,
) -> wide::f32x16 {
    use super::trig_approx::{asin_f32x16, cos_f32x16, sin_f32x16};
    use wide::f32x16;

    let deg_to_rad = f32x16::splat(DEG_TO_RAD);
    let two = f32x16::splat(2.0);
    let earth_radius = f32x16::splat(EARTH_RADIUS_M);

    let ref_lat_rad = f32x16::splat(ref_lat * DEG_TO_RAD);
    let ref_lon_rad = f32x16::splat(ref_lon * DEG_TO_RAD);
    let target_lats_rad = target_lats * deg_to_rad;
    let target_lons_rad = target_lons * deg_to_rad;

    let dlat = target_lats_rad - ref_lat_rad;
    let dlon = target_lons_rad - ref_lon_rad;

    let half_dlat = dlat / two;
    let half_dlon = dlon / two;

    let sin_half_dlat = sin_f32x16(half_dlat);
    let sin_half_dlon = sin_f32x16(half_dlon);

    let cos_ref_lat = cos_f32x16(ref_lat_rad);
    let cos_target_lats = cos_f32x16(target_lats_rad);

    let a = sin_half_dlat * sin_half_dlat
        + cos_ref_lat * cos_target_lats * sin_half_dlon * sin_half_dlon;

    let a_clamped = a.max(f32x16::splat(0.0)).min(f32x16::splat(1.0));
    let sqrt_a = a_clamped.sqrt();
    let c = two * asin_f32x16(sqrt_a);

    earth_radius * c
}

/// Compute haversine distances from one point to multiple targets with automatic chunking.
///
/// This function handles any number of targets by automatically batching into
/// SIMD-width chunks and handling remainders.
///
/// # Arguments
///
/// * `ref_lat` - Reference point latitude in degrees
/// * `ref_lon` - Reference point longitude in degrees
/// * `targets` - Slice of (latitude, longitude) pairs in degrees
///
/// # Returns
///
/// Vector of distances in meters, one per target
pub fn haversine_batch(ref_lat: f32, ref_lon: f32, targets: &[(f32, f32)]) -> Vec<f32> {
    if targets.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::with_capacity(targets.len());

    // Process in chunks of 8
    let chunks = targets.chunks_exact(8);
    let remainder = chunks.remainder();

    for chunk in chunks {
        let lats = f32x8::from([
            chunk[0].0,
            chunk[1].0,
            chunk[2].0,
            chunk[3].0,
            chunk[4].0,
            chunk[5].0,
            chunk[6].0,
            chunk[7].0,
        ]);
        let lons = f32x8::from([
            chunk[0].1,
            chunk[1].1,
            chunk[2].1,
            chunk[3].1,
            chunk[4].1,
            chunk[5].1,
            chunk[6].1,
            chunk[7].1,
        ]);

        let distances: [f32; 8] = haversine_f32x8(ref_lat, ref_lon, lats, lons).into();
        results.extend_from_slice(&distances);
    }

    // Handle remainder with padding
    if !remainder.is_empty() {
        let mut lats = [0.0f32; 8];
        let mut lons = [0.0f32; 8];

        for (i, &(lat, lon)) in remainder.iter().enumerate() {
            lats[i] = lat;
            lons[i] = lon;
        }
        // Pad with first element to avoid NaN issues
        for i in remainder.len()..8 {
            lats[i] = remainder[0].0;
            lons[i] = remainder[0].1;
        }

        let distances: [f32; 8] =
            haversine_f32x8(ref_lat, ref_lon, f32x8::from(lats), f32x8::from(lons)).into();
        results.extend_from_slice(&distances[..remainder.len()]);
    }

    results
}

/// Find minimum distance from a point to a set of polygon vertices.
///
/// Uses SIMD acceleration to compute distances to all vertices and find the minimum.
///
/// # Arguments
///
/// * `lat` - Query point latitude in degrees
/// * `lon` - Query point longitude in degrees
/// * `vertices` - Slice of (latitude, longitude) pairs representing polygon vertices
///
/// # Returns
///
/// Minimum distance in meters to any vertex
pub fn min_distance_to_vertices(lat: f32, lon: f32, vertices: &[(f32, f32)]) -> f32 {
    if vertices.is_empty() {
        return f32::MAX;
    }

    let distances = haversine_batch(lat, lon, vertices);
    distances
        .into_iter()
        .fold(f32::MAX, |min, d| if d < min { d } else { min })
}

/// Scalar haversine for single distance calculation (used for testing).
#[cfg(test)]
pub fn haversine_scalar(lat1: f32, lon1: f32, lat2: f32, lon2: f32) -> f32 {
    let lat1_rad = lat1 * DEG_TO_RAD;
    let lon1_rad = lon1 * DEG_TO_RAD;
    let lat2_rad = lat2 * DEG_TO_RAD;
    let lon2_rad = lon2 * DEG_TO_RAD;

    let dlat = lat2_rad - lat1_rad;
    let dlon = lon2_rad - lon1_rad;

    let a = (dlat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (dlon / 2.0).sin().powi(2);

    let c = 2.0 * a.sqrt().asin();

    EARTH_RADIUS_M * c
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Maximum allowed relative error for distance calculations
    const MAX_DISTANCE_ERROR_PERCENT: f32 = 1.0; // 1%

    #[test]
    fn test_haversine_zero_distance() {
        let lat = 50.0f32;
        let lon = 10.0f32;

        let lats = f32x8::splat(lat);
        let lons = f32x8::splat(lon);

        let distances: [f32; 8] = haversine_f32x8(lat, lon, lats, lons).into();

        for (i, &d) in distances.iter().enumerate() {
            assert!(
                d.abs() < 1.0, // Should be essentially 0, allow 1m tolerance
                "Zero distance test failed at index {}: got {} meters",
                i,
                d
            );
        }
    }

    #[test]
    fn test_haversine_known_distances() {
        // Known distances (approximate):
        // Riga (56.95, 24.1) to Tallinn (59.44, 24.75) ≈ 280 km
        // London (51.5, -0.12) to Paris (48.86, 2.35) ≈ 344 km
        // New York (40.71, -74.01) to Los Angeles (34.05, -118.24) ≈ 3944 km

        let ref_lats = [56.95f32, 51.5, 40.71];
        let ref_lons = [24.1f32, -0.12, -74.01];
        let target_lats = [59.44f32, 48.86, 34.05];
        let target_lons = [24.75f32, 2.35, -118.24];
        let expected_km = [280.0f32, 344.0, 3944.0];

        for i in 0..3 {
            let targets = [(target_lats[i], target_lons[i])];
            let distances = haversine_batch(ref_lats[i], ref_lons[i], &targets);
            let distance_km = distances[0] / 1000.0;

            let error_percent = ((distance_km - expected_km[i]) / expected_km[i]).abs() * 100.0;
            assert!(
                error_percent < MAX_DISTANCE_ERROR_PERCENT,
                "Known distance test {}: expected ~{}km, got {}km (error {}%)",
                i,
                expected_km[i],
                distance_km,
                error_percent
            );
        }
    }

    #[test]
    fn test_haversine_batch_various_sizes() {
        let ref_lat = 50.0f32;
        let ref_lon = 10.0f32;

        // Test various batch sizes
        for size in [1, 3, 7, 8, 9, 15, 16, 17, 100] {
            let targets: Vec<(f32, f32)> = (0..size)
                .map(|i| (50.0 + 0.01 * i as f32, 10.0 + 0.01 * i as f32))
                .collect();

            let results = haversine_batch(ref_lat, ref_lon, &targets);

            assert_eq!(
                results.len(),
                size,
                "Batch size {} should return {} results, got {}",
                size,
                size,
                results.len()
            );

            // First result should be ~0 (same point)
            if size > 0 {
                // Results should be positive and increasing (roughly)
                for d in &results {
                    assert!(*d >= 0.0, "Distance should be non-negative, got {}", d);
                }
            }
        }
    }

    #[test]
    fn test_haversine_batch_empty() {
        let results = haversine_batch(50.0, 10.0, &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_min_distance_to_vertices() {
        let query_lat = 50.0f32;
        let query_lon = 10.0f32;

        // One vertex is the same point
        let vertices = vec![
            (50.0, 10.0), // Same point
            (51.0, 11.0),
            (52.0, 12.0),
            (49.0, 9.0),
        ];

        let min_dist = min_distance_to_vertices(query_lat, query_lon, &vertices);
        assert!(
            min_dist < 1.0, // Should be ~0
            "Min distance should be ~0 for same point, got {}",
            min_dist
        );
    }

    #[test]
    fn test_min_distance_empty_vertices() {
        let min_dist = min_distance_to_vertices(50.0, 10.0, &[]);
        assert_eq!(min_dist, f32::MAX);
    }

    #[test]
    fn test_simd_vs_scalar_consistency() {
        // Compare SIMD results against scalar implementation
        let ref_lat = 56.95f32;
        let ref_lon = 24.1f32;

        let targets = vec![
            (59.44, 24.75),
            (51.5, -0.12),
            (40.71, -74.01),
            (35.68, 139.69), // Tokyo
            (-33.87, 151.21), // Sydney
            (48.86, 2.35),
            (55.75, 37.62), // Moscow
            (52.52, 13.41), // Berlin
        ];

        let simd_results = haversine_batch(ref_lat, ref_lon, &targets);

        for (i, &(lat, lon)) in targets.iter().enumerate() {
            let scalar_result = haversine_scalar(ref_lat, ref_lon, lat, lon);
            let error_percent =
                ((simd_results[i] - scalar_result) / scalar_result).abs() * 100.0;

            assert!(
                error_percent < 1.0, // Allow 1% error between implementations
                "SIMD vs scalar mismatch at index {}: SIMD={}, scalar={}, error={}%",
                i,
                simd_results[i],
                scalar_result,
                error_percent
            );
        }
    }

    #[test]
    fn test_antipodal_points() {
        // Antipodal points (opposite sides of Earth)
        // Should be approximately half the Earth's circumference
        let ref_lat = 0.0f32;
        let ref_lon = 0.0f32;
        let target_lat = 0.0f32;
        let target_lon = 180.0f32;

        let targets = [(target_lat, target_lon)];
        let distances = haversine_batch(ref_lat, ref_lon, &targets);

        // Half circumference ≈ π * R ≈ 20015 km
        let expected_km = std::f32::consts::PI * EARTH_RADIUS_M / 1000.0;
        let actual_km = distances[0] / 1000.0;

        let error_percent = ((actual_km - expected_km) / expected_km).abs() * 100.0;
        assert!(
            error_percent < 1.0,
            "Antipodal distance: expected ~{}km, got {}km (error {}%)",
            expected_km,
            actual_km,
            error_percent
        );
    }

    #[test]
    fn test_poles() {
        // North to South pole
        let ref_lat = 90.0f32;
        let ref_lon = 0.0f32;
        let target_lat = -90.0f32;
        let target_lon = 0.0f32;

        let targets = [(target_lat, target_lon)];
        let distances = haversine_batch(ref_lat, ref_lon, &targets);

        // Pole to pole ≈ π * R ≈ 20015 km
        let expected_km = std::f32::consts::PI * EARTH_RADIUS_M / 1000.0;
        let actual_km = distances[0] / 1000.0;

        let error_percent = ((actual_km - expected_km) / expected_km).abs() * 100.0;
        assert!(
            error_percent < 1.0,
            "Pole to pole distance: expected ~{}km, got {}km (error {}%)",
            expected_km,
            actual_km,
            error_percent
        );
    }
}
