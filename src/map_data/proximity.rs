use std::{collections::HashMap, u16};

use serde::{Deserialize, Serialize};

use super::graph::MapDataPointRef;

type GpsCellId = (i16, i16);

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct PointGrid {
    grid: HashMap<GpsCellId, Vec<MapDataPointRef>>,
}

impl PointGrid {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_cell_id(lat: f32, lon: f32) -> GpsCellId {
        let lat_rounded = (lat * 100.0).round() as i16;
        let lon_rounded = (lon * 100.0).round() as i16;
        (lat_rounded, lon_rounded)
    }

    pub fn len(&self) -> usize {
        self.grid.len()
    }

    pub fn insert(&mut self, lat: f32, lon: f32, point_ref: MapDataPointRef) {
        let cell_id = PointGrid::get_cell_id(lat, lon);
        let maybe_points = self.grid.get_mut(&cell_id);
        if let Some(points) = maybe_points {
            points.push(point_ref.clone());
        } else {
            self.grid.insert(cell_id, vec![point_ref.clone()]);
        }
    }

    fn get_points_in_cells(&self, cell_ids: Vec<GpsCellId>) -> Vec<MapDataPointRef> {
        cell_ids
            .iter()
            .flat_map(|cell_id| match self.grid.get(cell_id) {
                Some(points) => points.clone(),
                None => Vec::new(),
            })
            .collect()
    }

    fn get_outer_cell_ids(center: GpsCellId, offset: u16) -> Option<Vec<GpsCellId>> {
        let lat_rounded = center.0;
        let lon_rounded = center.1;
        let result = (-(offset as i16)..=(offset as i16))
            .flat_map(|lat_offset| {
                (-(offset as i16)..=(offset as i16))
                    .map(|lon_offset| {
                        if lat_offset.unsigned_abs() == offset || lon_offset.unsigned_abs() == offset {
                            let lat_new = lat_rounded - lat_offset;
                            let lat_new = if lat_new > 9000 {
                                lat_new - 9000
                            } else if lat_new < -9000 {
                                lat_new + 9000
                            } else {
                                lat_new
                            };
                            let lon_new = lon_rounded - lon_offset;
                            let lon_new = if lon_new > 18000 {
                                lon_new - 18000
                            } else if lon_new < -18000 {
                                lon_new + 18000
                            } else {
                                lon_new
                            };
                            Some((lat_new, lon_new))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<Option<GpsCellId>>>()
            })
            .flatten()
            .collect();

        Some(result)
    }

    pub fn find_closest_point_refs(&self, lat: f32, lon: f32) -> Option<Vec<MapDataPointRef>> {
        let center_cell_id = PointGrid::get_cell_id(lat, lon);

        for step in 0..=10 {
            let cell_ids = PointGrid::get_outer_cell_ids(center_cell_id, step);
            let cell_ids = match cell_ids {
                Some(ids) => ids,
                None => return None,
            };
            let points_in_cell = self.get_points_in_cells(cell_ids);
            if !points_in_cell.is_empty() {
                return Some(points_in_cell);
            }
        }

        None
    }
}

#[cfg(test)]
mod test {
    use rusty_fork::rusty_fork_test;
    use tracing::info;

    use super::PointGrid;

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn cell_id() {
            let tests = [
                (21.211, 54.1113, (2121, 5411)),
                (21.21123, 54.111_343, (2121, 5411)),
                (21.21, 54.11, (2121, 5411)),
                (0.0, 0.0, (0, 0)),
                (-90.0, -180.0, (-9000, -18000)),
                (90.0, 180.0, (9000, 18000)),
            ];
            for test in tests.iter() {
                let hash = PointGrid::get_cell_id(test.0, test.1);
                assert_eq!(hash, test.2);
            }
        }

        #[test]
        #[rustfmt::skip]
        fn get_outer_cell_ids() {
            let tests = [
            (
                2121,
                5411,
                1,
                vec![
        (2122, 5410),   (2122, 5411),       (2122, 5412),
        (2121, 5410),   /*(2121, 5411)*/    (2121, 5412),
        (2120, 5410),   (2120, 5411),       (2120, 5412),
                ],
            ),
            (
                2121,
                5411,
                2,
                vec![
    (2123, 5409),   (2123, 5410), (2123, 5411), (2123, 5412),   (2123, 5413),
    (2122, 5409), /*(2122, 5410), (2122, 5411), (2122, 5412),*/ (2122, 5413),
    (2121, 5409), /*(2121, 5410), (2121, 5411), (2121, 5412),*/ (2121, 5413),
    (2120, 5409), /*(2120, 5410), (2120, 5411), (2120, 5412),*/ (2120, 5413),
    (2119, 5409),   (2119, 5410), (2119, 5411), (2119, 5412),   (2119, 5413)
                ],
            ),
            ];

            for (idx, test) in tests.iter().enumerate() {
                let adjacent_cell_ids =
                    PointGrid::get_outer_cell_ids((test.0, test.1), test.2);
                info!("test {idx}");
                info!("adjacent {adjacent_cell_ids:?}");
                if test.3.is_empty() {
                    assert!(adjacent_cell_ids.is_none());
                } else {
                    let ids = adjacent_cell_ids.unwrap();
                    assert_eq!(test.3.len(), ids.len());
                    assert!(test
                        .3
                        .iter()
                        .all(|test_id| ids.iter().any(|id| id == test_id)));
                }
            }
        }
    }
}
