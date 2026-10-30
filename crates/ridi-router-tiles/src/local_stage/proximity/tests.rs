use super::*;
use geo::{LineString, Polygon};

/// Deterministic xorshift for test geometry.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Star-shaped polygon around (lat, lon) with `n` vertices and radius up to `r` degrees,
/// optional hole.
fn star(rng: &mut Rng, lat: f64, lon: f64, n: usize, r: f64, hole: bool) -> MultiPolygon<f64> {
    let ring = |rng: &mut Rng, scale: f64| {
        let mut pts: Vec<Coord<f64>> = (0..n)
            .map(|i| {
                let a = i as f64 / n as f64 * std::f64::consts::TAU;
                let rr = r * scale * (0.3 + 0.7 * rng.next());
                Coord {
                    x: lon + rr * a.cos() / lat.to_radians().cos(),
                    y: lat + rr * a.sin(),
                }
            })
            .collect();
        pts.push(pts[0]);
        LineString::new(pts)
    };
    let ext = ring(rng, 1.0);
    let holes = if hole { vec![ring(rng, 0.2)] } else { vec![] };
    MultiPolygon::new(vec![Polygon::new(ext, holes)])
}

#[test]
fn within_threshold_matches_reference_on_hard_shapes() {
    let mut rng = Rng(0x9e3779b97f4a7c15);
    let mut checked = 0;
    let mut within = 0;
    // (latitude, vertices, radius deg): city blocks, huge low-vertex polygons (long segments ->
    // great-circle bulge), very complex polygons, high latitudes, near the equator.
    for &(lat, n, r) in &[
        (52.0, 12, 0.003),
        (52.0, 5, 0.6),
        (56.0, 4000, 0.05),
        (70.0, 6, 1.5),
        (78.0, 8, 2.0),
        (84.0, 10, 1.0),
        (-45.0, 300, 0.02),
        (0.5, 7, 3.0),
    ] {
        for k in 0..6 {
            let mp = star(&mut rng, lat, 20.0, n, r, k % 2 == 1);
            let p = prepare_polygon(&mp);
            for _ in 0..400 {
                // points within ~2r + 0.02 deg of the centre, f32 like cell centres; half of
                // them near a vertex (the hard band around 500 m)
                let (plat, plon) = if rng.next() < 0.5 {
                    let v = mp.0[0].exterior().0[(rng.next() * n as f64) as usize];
                    let d = (rng.next() - 0.5) * 0.02;
                    ((v.y + d) as f32, (v.x + d / lat.to_radians().cos()) as f32)
                } else {
                    let dlat = (rng.next() - 0.5) * (4.0 * r + 0.04);
                    let dlon = (rng.next() - 0.5) * (4.0 * r + 0.04) / lat.to_radians().cos();
                    ((lat + dlat) as f32, (20.0 + dlon) as f32)
                };
                let reference = polygon_distance_m(&mp, plat, plon) <= 500.0;
                assert_eq!(
                    within_threshold(&p, plat, plon, 500.0),
                    reference,
                    "lat {lat} n {n} r {r} point ({plat}, {plon})"
                );
                let inside = mp.0[0].contains(&Point::new(plon as f64, plat as f64));
                assert_eq!(
                    polygon_contains(
                        &p,
                        Coord {
                            x: plon as f64,
                            y: plat as f64
                        }
                    ),
                    inside
                );
                checked += 1;
                within += reference as usize;
            }
        }
    }
    assert!(checked > 10_000);
    assert!(
        within > 1_000 && within < checked - 1_000,
        "within {within} of {checked}"
    );
}

/// Square polygon of `half` degrees (latitude) around (lat, lon), square on the ground.
fn square(lat: f64, lon: f64, half: f64) -> MultiPolygon<f64> {
    let hl = half / lat.to_radians().cos();
    MultiPolygon::new(vec![Polygon::new(
        LineString::from(vec![
            (lon - hl, lat - half),
            (lon + hl, lat - half),
            (lon + hl, lat + half),
            (lon - hl, lat + half),
            (lon - hl, lat - half),
        ]),
        vec![],
    )])
}

#[test]
fn global_cells_clamp_at_antimeridian_and_pole() {
    let anchor = GridAnchor::global();
    let key = anchor.residential_cell(10.0, 180.0).unwrap();
    assert_eq!(key as u32 as i32, GLOBAL_COLS - 1);
    let key = anchor.residential_cell(90.0, 0.0).unwrap();
    assert_eq!((key >> 32) as i32, GLOBAL_ROWS - 1);
    let key = anchor.military_cell(90.0, 180.0).unwrap();
    assert_eq!(key as u32 as i32, GLOBAL_MILITARY_COLS - 1);
    assert_eq!((key >> 32) as i32, GLOBAL_MILITARY_ROWS - 1);
    // the last cell's centre stays inside the world
    let (lat, lon) = anchor.residential_center(anchor.residential_cell(90.0, 180.0).unwrap());
    assert!(lon < 180.0 && lat < 90.0, "({lat}, {lon})");
    // -180 and +180 are different cells (no wrap), as tiles do not wrap either
    assert_eq!(anchor.residential_cell(10.0, -180.0).unwrap() as u32, 0);
}

#[test]
fn prefilter_keeps_500m_east_west_at_high_latitudes() {
    for lat in [0.0f64, 48.0, 57.0, 60.0, 70.0, 78.0] {
        // 100 m half-size polygon, cell centre 480 m (polygon edge distance) due east
        let mp = square(lat, 20.0, 0.0009);
        let edge_lon = 20.0 + 0.0009 / lat.to_radians().cos();
        let dlon = 480.0 / (METERS_PER_DEG * lat.to_radians().cos());
        let (plat, plon) = (lat as f32, (edge_lon + dlon) as f32);
        let d = polygon_distance_m(&mp, plat, plon);
        assert!((d - 480.0).abs() < 2.0, "lat {lat}: distance {d}");
        let b = compute_multipolygon_bbox(&mp);
        let prefilter = prefilter_box(&b, 500.0).unwrap();
        assert!(in_box(&prefilter, plat, plon), "prefilter drops lat {lat}");
    }
}

#[test]
fn widened_prefilter_matches_reference_maths() {
    // Every polygon within 500 m of a cell centre must survive the widened prefilter.
    let mut rng = Rng(0x2545f4914f6cdd1d);
    for &lat in &[45.0, 57.0, 60.0, 70.0, 78.0, 85.0, -60.0] {
        for _ in 0..20 {
            let mp = star(&mut rng, lat, 20.0, 9, 0.003, false);
            let b = prefilter_box(&compute_multipolygon_bbox(&mp), 500.0).unwrap();
            for _ in 0..300 {
                let dlat = (rng.next() - 0.5) * 0.03;
                let dlon = (rng.next() - 0.5) * 0.03 / (lat as f64).to_radians().cos();
                let (plat, plon) = ((lat + dlat) as f32, (20.0 + dlon) as f32);
                if polygon_distance_m(&mp, plat, plon) <= 500.0 {
                    assert!(in_box(&b, plat, plon), "lat {lat} point ({plat}, {plon})");
                }
            }
        }
    }
}
