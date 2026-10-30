//! Ring assembly for multipolygon relations (legacy semantics: only naturally closed rings).

use geo::{Coord, LineString};

/// Connect OSM ways into complete polygon rings.
///
/// Compared to the current broken implementation, we only keep *naturally closed*
/// rings. Main branch does the same: incomplete chains are discarded instead of
/// being force-closed into bogus polygons.
pub(crate) fn connect_ways_into_rings(ways: Vec<LineString<f64>>) -> Vec<LineString<f64>> {
    use std::collections::VecDeque;

    let mut rings = Vec::new();
    let mut remaining: VecDeque<LineString<f64>> = ways.into_iter().collect();

    while let Some(first) = remaining.pop_front() {
        let mut current_ring = first.into_inner();
        if current_ring.len() < 2 {
            continue;
        }

        let mut changed = true;
        while changed {
            changed = false;

            let mut i = 0;
            while i < remaining.len() {
                let way_coords: Vec<Coord<f64>> = remaining[i].coords().cloned().collect();
                if way_coords.len() < 2 {
                    i += 1;
                    continue;
                }

                let way_start = way_coords.first().unwrap();
                let way_end = way_coords.last().unwrap();
                let ring_start = current_ring.first().unwrap();
                let ring_end = current_ring.last().unwrap();

                if coords_match(way_start, ring_end) {
                    current_ring.extend(way_coords.iter().skip(1).cloned());
                    remaining.remove(i);
                    changed = true;
                } else if coords_match(way_end, ring_end) {
                    current_ring.extend(way_coords.iter().rev().skip(1).cloned());
                    remaining.remove(i);
                    changed = true;
                } else if coords_match(way_end, ring_start) {
                    let mut new_ring: Vec<Coord<f64>> = way_coords
                        .iter()
                        .take(way_coords.len() - 1)
                        .cloned()
                        .collect();
                    new_ring.extend(current_ring);
                    current_ring = new_ring;
                    remaining.remove(i);
                    changed = true;
                } else if coords_match(way_start, ring_start) {
                    let mut new_ring: Vec<Coord<f64>> = way_coords
                        .iter()
                        .rev()
                        .take(way_coords.len() - 1)
                        .cloned()
                        .collect();
                    new_ring.extend(current_ring);
                    current_ring = new_ring;
                    remaining.remove(i);
                    changed = true;
                } else {
                    i += 1;
                }
            }
        }

        if is_closed_ring(&current_ring) {
            rings.push(LineString::new(current_ring));
        }
    }

    rings
}

fn coords_match(a: &Coord<f64>, b: &Coord<f64>) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
}

fn is_closed_ring(coords: &[Coord<f64>]) -> bool {
    coords.len() >= 4
        && coords
            .first()
            .zip(coords.last())
            .map(|(first, last)| coords_match(first, last))
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ls(coords: &[(f64, f64)]) -> LineString<f64> {
        LineString::new(coords.iter().map(|(x, y)| Coord { x: *x, y: *y }).collect())
    }

    #[test]
    fn connect_ways_discards_incomplete_chains() {
        let rings = connect_ways_into_rings(vec![
            ls(&[(0.0, 0.0), (1.0, 0.0)]),
            ls(&[(1.0, 0.0), (1.0, 1.0)]),
            ls(&[(1.0, 1.0), (0.0, 1.0)]),
        ]);

        assert!(rings.is_empty());
    }

    #[test]
    fn connect_ways_handles_prepend_reversed_case() {
        let rings = connect_ways_into_rings(vec![
            ls(&[(1.0, 0.0), (1.0, 1.0)]),
            ls(&[(1.0, 0.0), (0.0, 0.0)]),
            ls(&[(0.0, 1.0), (0.0, 0.0)]),
            ls(&[(1.0, 1.0), (0.0, 1.0)]),
        ]);

        assert_eq!(rings.len(), 1);
        let ring = &rings[0].0;
        assert_eq!(ring.first(), ring.last());
        assert!(ring
            .iter()
            .any(|coord| coords_match(coord, &Coord { x: 0.0, y: 0.0 })));
        assert!(ring
            .iter()
            .any(|coord| coords_match(coord, &Coord { x: 1.0, y: 0.0 })));
        assert!(ring
            .iter()
            .any(|coord| coords_match(coord, &Coord { x: 1.0, y: 1.0 })));
        assert!(ring
            .iter()
            .any(|coord| coords_match(coord, &Coord { x: 0.0, y: 1.0 })));
    }
}
