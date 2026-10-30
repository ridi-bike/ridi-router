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
