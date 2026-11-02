//! Near-duplicate route removal.
//!
//! Routes are visited best score first. A route is dropped when most of it runs within one grid
//! cell of a route that was already kept, so routes that differ only by a few turns or by a
//! parallel street collapse into the best-scoring one. Cells are `CELL_M` squares on a local
//! equirectangular projection; a kept route covers its own cells and their 8 neighbours.

use rustc_hash::FxHashMap;

const CELL_M: f64 = 250.;
/// Densify segments so a long straight segment still marks every cell it crosses.
const DENSIFY_STEP_M: f64 = CELL_M / 4.;
const SIMILARITY_THRESHOLD: f64 = 0.8;
const M_PER_DEG_LAT: f64 = 110_540.;
const M_PER_DEG_LON_EQUATOR: f64 = 111_320.;

/// Local projection to metres, shared by every route of one request so their cells line up.
#[derive(Debug, Clone, Copy)]
pub struct Projection {
    lat0: f64,
    lon0: f64,
    m_per_deg_lon: f64,
}

impl Projection {
    pub fn new(origin_lat: f32, origin_lon: f32) -> Self {
        let lat0 = origin_lat as f64;
        Self {
            lat0,
            lon0: origin_lon as f64,
            m_per_deg_lon: M_PER_DEG_LON_EQUATOR * lat0.to_radians().cos(),
        }
    }

    fn to_xy(self, lat: f32, lon: f32) -> (f64, f64) {
        (
            (lon as f64 - self.lon0) * self.m_per_deg_lon,
            (lat as f64 - self.lat0) * M_PER_DEG_LAT,
        )
    }
}

fn cell_key(ix: i32, iy: i32) -> u64 {
    ((ix as u32 as u64) << 32) | iy as u32 as u64
}

fn cell_coords(key: u64) -> (i32, i32) {
    ((key >> 32) as u32 as i32, key as u32 as i32)
}

fn cell_of((x, y): (f64, f64)) -> u64 {
    cell_key((x / CELL_M).floor() as i32, (y / CELL_M).floor() as i32)
}

/// Sorted, distinct grid cells a route passes through.
#[derive(Debug, Clone, Default)]
pub struct Corridor {
    cells: Vec<u64>,
}

impl Corridor {
    pub fn from_coords(
        projection: Projection,
        coords: impl IntoIterator<Item = (f32, f32)>,
    ) -> Self {
        let mut cells = Vec::new();
        let mut prev: Option<(f64, f64)> = None;
        for (lat, lon) in coords {
            let point = projection.to_xy(lat, lon);
            if let Some(prev) = prev {
                let len = ((point.0 - prev.0).powi(2) + (point.1 - prev.1).powi(2)).sqrt();
                let steps = (len / DENSIFY_STEP_M).ceil() as usize;
                for step in 1..steps {
                    let f = step as f64 / steps as f64;
                    cells.push(cell_of((
                        prev.0 + (point.0 - prev.0) * f,
                        prev.1 + (point.1 - prev.1) * f,
                    )));
                }
            }
            cells.push(cell_of(point));
            prev = Some(point);
        }
        cells.sort_unstable();
        cells.dedup();
        Self { cells }
    }

    fn len(&self) -> usize {
        self.cells.len()
    }
}

/// Share of `candidate` near `kept`, capped by how different their corridor sizes are, so a
/// short route is not swallowed by a much longer one that happens to contain it.
fn similarity(near_cells: usize, candidate_cells: usize, kept_cells: usize) -> f64 {
    let coverage = near_cells as f64 / candidate_cells as f64;
    let size_ratio =
        candidate_cells.min(kept_cells) as f64 / candidate_cells.max(kept_cells) as f64;
    coverage.min(size_ratio.sqrt())
}

/// Returns the indices of the routes to keep, in the order given by `best_first` (which must
/// list candidates best first). Routes with an empty corridor are dropped.
pub fn select_distinct(corridors: &[Corridor], best_first: &[usize]) -> Vec<usize> {
    // Kept routes are indexed by their dilated cells: cell -> kept slots that cover it.
    let mut covered_by: FxHashMap<u64, Vec<u32>> = FxHashMap::default();
    let mut kept: Vec<usize> = Vec::new();
    let mut near_cells: Vec<usize> = Vec::new();

    for &idx in best_first {
        let corridor = &corridors[idx];
        if corridor.len() == 0 {
            continue;
        }
        near_cells.clear();
        near_cells.resize(kept.len(), 0);
        for cell in &corridor.cells {
            if let Some(slots) = covered_by.get(cell) {
                for &slot in slots {
                    near_cells[slot as usize] += 1;
                }
            }
        }
        let is_duplicate = kept.iter().enumerate().any(|(slot, &kept_idx)| {
            similarity(near_cells[slot], corridor.len(), corridors[kept_idx].len())
                >= SIMILARITY_THRESHOLD
        });
        if is_duplicate {
            continue;
        }

        let slot = kept.len() as u32;
        for &cell in &corridor.cells {
            let (ix, iy) = cell_coords(cell);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let slots = covered_by.entry(cell_key(ix + dx, iy + dy)).or_default();
                    if slots.last() != Some(&slot) {
                        slots.push(slot);
                    }
                }
            }
        }
        kept.push(idx);
    }

    kept
}

/// `points` coordinates spread evenly by distance along the route (first and last included).
pub fn approximate_route(
    projection: Projection,
    coords: &[(f32, f32)],
    points: usize,
) -> Vec<(f32, f32)> {
    if coords.len() < 2 || points < 2 {
        return coords.iter().copied().take(points).collect();
    }
    let mut cumulative = Vec::with_capacity(coords.len());
    let mut total = 0.;
    let mut prev = projection.to_xy(coords[0].0, coords[0].1);
    for &(lat, lon) in coords {
        let point = projection.to_xy(lat, lon);
        total += ((point.0 - prev.0).powi(2) + (point.1 - prev.1).powi(2)).sqrt();
        cumulative.push(total);
        prev = point;
    }
    let mut idx = 0;
    (0..points)
        .map(|step| {
            let target = total * step as f64 / (points - 1) as f64;
            while idx + 1 < coords.len() && cumulative[idx] < target {
                idx += 1;
            }
            coords[idx]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAT0: f32 = 56.95;
    const LON0: f32 = 24.1;
    /// Degrees of latitude per metre.
    const DLAT: f32 = 1. / 110_540.;

    fn projection() -> Projection {
        Projection::new(LAT0, LON0)
    }

    /// Straight east-west line `len_m` long, `north_m` north of the origin, a point every 50 m.
    fn line(north_m: f32, len_m: f32) -> Vec<(f32, f32)> {
        let dlon = 1. / (111_320. * (LAT0 as f64).to_radians().cos()) as f32;
        (0..=(len_m / 50.) as usize)
            .map(|i| (LAT0 + north_m * DLAT, LON0 + i as f32 * 50. * dlon))
            .collect()
    }

    fn corridors(routes: &[Vec<(f32, f32)>]) -> Vec<Corridor> {
        routes
            .iter()
            .map(|r| Corridor::from_coords(projection(), r.iter().copied()))
            .collect()
    }

    #[test]
    fn identical_routes_collapse_to_the_first() {
        let route = line(0., 2_000.);
        let c = corridors(&[route.clone(), route.clone(), route]);
        assert_eq!(select_distinct(&c, &[2, 0, 1]), vec![2]);
    }

    #[test]
    fn parallel_street_counts_as_the_same_route() {
        let c = corridors(&[line(0., 10_000.), line(150., 10_000.)]);
        assert_eq!(select_distinct(&c, &[0, 1]), vec![0]);
    }

    #[test]
    fn distant_routes_are_both_kept() {
        let c = corridors(&[line(0., 10_000.), line(3_000., 10_000.)]);
        assert_eq!(select_distinct(&c, &[1, 0]), vec![1, 0]);
    }

    #[test]
    fn route_differing_by_a_short_detour_is_dropped() {
        let base = line(0., 20_000.);
        let mut detour = base.clone();
        // 1 km of the 20 km route moved 2 km north.
        for p in &mut detour[100..120] {
            p.0 += 2_000. * DLAT;
        }
        let c = corridors(&[base, detour]);
        assert_eq!(select_distinct(&c, &[0, 1]), vec![0]);
    }

    #[test]
    fn route_differing_for_half_its_length_is_kept() {
        let base = line(0., 20_000.);
        let mut other = base.clone();
        for p in &mut other[200..] {
            p.0 += 3_000. * DLAT;
        }
        let c = corridors(&[base, other]);
        assert_eq!(select_distinct(&c, &[0, 1]), vec![0, 1]);
    }

    #[test]
    fn short_route_inside_a_long_one_is_kept() {
        let c = corridors(&[line(0., 20_000.), line(0., 5_000.)]);
        assert_eq!(select_distinct(&c, &[0, 1]), vec![0, 1]);
    }

    #[test]
    fn empty_input_and_empty_routes() {
        assert!(select_distinct(&[], &[]).is_empty());
        let c = corridors(&[vec![], line(0., 1_000.)]);
        assert_eq!(select_distinct(&c, &[0, 1]), vec![1]);
    }

    #[test]
    fn single_point_route_is_kept() {
        let c = corridors(&[vec![(LAT0, LON0)]]);
        assert_eq!(select_distinct(&c, &[0]), vec![0]);
    }
}
