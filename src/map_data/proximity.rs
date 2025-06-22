use std::{collections::HashMap, u16};

use geo::{BoundingRect, Contains, CoordsIter, Intersects, MultiPolygon, Point};
use serde::{Deserialize, Serialize};

type GpsCellId = (i16, i16);

// two decimal places 1.1km precision
pub const GRID_CALC_PRECISION: i16 = 100;

pub fn round_to_precision(v: f64) -> f64 {
    (v * GRID_CALC_PRECISION as f64).round() / GRID_CALC_PRECISION as f64
}

#[derive(Debug)]
pub struct AreaGrid {
    point_grid: PointGrid<MultiPolygon>,
}

impl AreaGrid {
    pub fn new() -> Self {
        Self {
            point_grid: PointGrid::new(),
        }
    }
    pub fn insert_multi_polygon(&mut self, multi_polygon: &MultiPolygon) -> Option<MultiPolygon> {
        if let Some(bounding_rect) = multi_polygon.bounding_rect() {
            let mut adjusted_multi_polygon = multi_polygon.clone();
            adjusted_multi_polygon.iter_mut().for_each(|p| {
                p.exterior_mut(|l| {
                    l.coords_mut().for_each(|c| {
                        c.x = round_to_precision(c.x);
                        c.y = round_to_precision(c.y);
                    });
                });

                p.interiors_mut(|lines| {
                    lines.iter_mut().for_each(|l| {
                        l.coords_mut().for_each(|c| {
                            c.x = round_to_precision(c.x);
                            c.y = round_to_precision(c.y);
                        });
                    })
                });
            });
            let x_max = round_to_precision(bounding_rect.max().x);
            let x_min = round_to_precision(bounding_rect.min().x);
            let mut x = x_min;
            let y_max = round_to_precision(bounding_rect.max().y);
            let y_min = round_to_precision(bounding_rect.min().y);
            let mut y = y_min;

            while x <= x_max {
                while y <= y_max {
                    let point = Point::new(x, y);
                    if adjusted_multi_polygon.contains(&point)
                        || adjusted_multi_polygon
                            .exterior_coords_iter()
                            .any(|c| c.intersects(&point))
                    {
                        self.point_grid.insert(y as f32, x as f32, &multi_polygon);
                    }
                    y += 1. / GRID_CALC_PRECISION as f64;
                }
                x += 1. / GRID_CALC_PRECISION as f64;
            }

            return Some(adjusted_multi_polygon);
        }
        None
    }
    pub fn find_closest_areas_refs(
        &self,
        lat: f32,
        lon: f32,
        steps: u16,
    ) -> Option<Vec<&MultiPolygon>> {
        self.point_grid.find_closest_point_refs(lat, lon, steps)
    }
    pub fn len(&self) -> usize {
        self.point_grid.len()
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct PointGrid<T: Clone> {
    grid: HashMap<GpsCellId, Vec<T>>,
}

impl<T: Clone> PointGrid<T> {
    pub fn new() -> PointGrid<T> {
        PointGrid {
            grid: HashMap::new(),
        }
    }

    pub fn get_cell_id(lat: f32, lon: f32) -> GpsCellId {
        let lat_rounded = (lat * GRID_CALC_PRECISION as f32).round() as i16;
        let lon_rounded = (lon * GRID_CALC_PRECISION as f32).round() as i16;
        (lat_rounded, lon_rounded)
    }

    pub fn len(&self) -> usize {
        self.grid.len()
    }

    pub fn insert(&mut self, lat: f32, lon: f32, point: &T) {
        let cell_id = PointGrid::<T>::get_cell_id(lat, lon);
        let maybe_points = self.grid.get_mut(&cell_id);
        if let Some(points) = maybe_points {
            points.push(point.to_owned());
        } else {
            self.grid.insert(cell_id, vec![point.to_owned()]);
        }
    }

    fn get_points_in_cells(&self, cell_ids: Vec<GpsCellId>) -> Vec<&T> {
        cell_ids
            .iter()
            .filter_map(|cell_id| self.grid.get(cell_id))
            .flatten()
            .collect()
    }

    fn get_outer_cell_ids(center: GpsCellId, offset: u16) -> Option<Vec<GpsCellId>> {
        let lat_rounded = center.0;
        let lon_rounded = center.1;
        let result = (-(offset as i16)..=(offset as i16))
            .flat_map(|lat_offset| {
                (-(offset as i16)..=(offset as i16))
                    .map(|lon_offset| {
                        if lat_offset.unsigned_abs() == offset
                            || lon_offset.unsigned_abs() == offset
                        {
                            let lat_new = lat_rounded - lat_offset;
                            let lat_new = if lat_new > 90 * GRID_CALC_PRECISION {
                                lat_new - 90 * GRID_CALC_PRECISION
                            } else if lat_new < -90 * GRID_CALC_PRECISION {
                                lat_new + 90 * GRID_CALC_PRECISION
                            } else {
                                lat_new
                            };
                            let lon_new = lon_rounded - lon_offset;
                            let lon_new = if lon_new > 180 * GRID_CALC_PRECISION {
                                lon_new - 180 * GRID_CALC_PRECISION
                            } else if lon_new < -180 * GRID_CALC_PRECISION {
                                lon_new + 180 * GRID_CALC_PRECISION
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

    // one square is rougly 1.1 km, so 10 steps will be center 1.1 + 2*steps*x1.1
    pub fn find_closest_point_refs(&self, lat: f32, lon: f32, steps: u16) -> Option<Vec<&T>> {
        let center_cell_id = PointGrid::<T>::get_cell_id(lat, lon);

        for step in 0..=steps {
            let cell_ids = PointGrid::<T>::get_outer_cell_ids(center_cell_id, step);
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

    use crate::map_data::graph::MapDataPointRef;

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
                let hash = PointGrid::<MapDataPointRef>::get_cell_id(test.0, test.1);
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
                    PointGrid::<MapDataPointRef>::get_outer_cell_ids((test.0, test.1), test.2);
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
