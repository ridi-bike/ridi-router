use std::{cmp::Ordering, collections::HashMap, sync::atomic::Ordering, u16};

use geo::{HaversineDistance, Point};
use serde::{Deserialize, Serialize};

use super::graph::MapDataPointRef;

type GpsCellId = (i16, i16);

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct GpsProximityLookup {
    grid: HashMap<GpsCellId, Vec<MapDataPointRef>>,
}

impl GpsProximityLookup {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_cell_id(lat: f32, lon: f32) -> GpsCellId {
        let lat_rounded = (lat * 100.0) as i16;
        let lon_rounded = (lon * 100.0) as i16;
        (lat_rounded, lon_rounded)
    }

    pub fn len(&self) -> usize {
        self.grid.len()
    }

    pub fn insert(&mut self, point_ref: MapDataPointRef) -> () {
        let point = point_ref.borrow();
        let cell_id = GpsProximityLookup::get_cell_id(point.lat, point.lon);
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
            .map(|cell_id| match self.grid.get(&cell_id) {
                Some(points) => points.clone(),
                None => Vec::new(),
            })
            .flatten()
            .collect()
    }

    fn get_outer_cell_ids(center: GpsCellId, offset: u16) -> Option<Vec<GpsCellId>> {
        let mut lat_wrapped = false;
        let mut lon_wrapped = false;

        let lat_rounded = center.0;
        let lon_rounded = center.1;
        let result = (-(offset as i16)..=(offset as i16))
            .map(|lat_offset| {
                (-(offset as i16)..=(offset as i16))
                    .map(|lon_offset| {
                        if lat_offset.abs() as u16 == offset || lon_offset.abs() as u16 == offset {
                            let lat_new = lat_rounded - lat_offset;
                            let lat_new = if lat_new > 9000 {
                                lat_wrapped = true;
                                lat_new - 9000
                            } else if lat_new < -9000 {
                                lat_wrapped = true;
                                lat_new + 9000
                            } else {
                                lat_new
                            };
                            let lon_new = lon_rounded - lon_offset;
                            let lon_new = if lon_new > 18000 {
                                lon_wrapped = true;
                                lon_new - 18000
                            } else if lon_new < -18000 {
                                lon_wrapped = true;
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
            .filter_map(|hash| hash)
            .collect();

        if lat_wrapped && lon_wrapped {
            return None;
        }

        Some(result)
    }

    pub fn find_closest(&self, lat: f32, lon: f32) -> Option<MapDataPointRef> {
        let center_cell_id = GpsProximityLookup::get_cell_id(lat, lon);

        let mut points_in_cell: Vec<MapDataPointRef> = Vec::new();

        let mut step = 1;
        while points_in_cell.is_empty() {
            let cell_ids = GpsProximityLookup::get_outer_cell_ids(center_cell_id, step);
            let mut cell_ids = match cell_ids {
                Some(ids) => ids,
                None => return None,
            };
            if step == 1 {
                cell_ids.push(center_cell_id);
            }
            points_in_cell = self.get_points_in_cells(cell_ids);

            step += 1;
        }

        let mut distances = points_in_cell
            .iter()
            .map(|p| {
                let point = p.borrow();
                let geo_point = Point::new(point.lon, point.lat);
                let geo_lookup_point = Point::new(lon, lat);
                (p, geo_point.haversine_distance(&geo_lookup_point))
            })
            .collect::<Vec<(&MapDataPointRef, f32)>>();
        distances.sort_by(|el1, el2| {
            if el1.1 > el2.1 {
                Ordering::Less
            } else if el1.1 < el2.1 {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        });

        distances.get(0).map_or(None, |v| Some(v.0.clone()))
    }
}

mod test {
    use super::GpsProximityLookup;

    #[test]
    fn cell_id() {
        let tests = [
            (21.211, 54.1113, (2121, 5411)),
            (21.21123, 54.111343524, (2121, 5411)),
            (21.21, 54.11, (2121, 5411)),
            (0.0, 0.0, (0, 0)),
            (-90.0, -180.0, (-9000, -18000)),
            (90.0, 180.0, (9000, 18000)),
        ];
        for (idx, test) in tests.iter().enumerate() {
            let hash = GpsProximityLookup::get_cell_id(test.0, test.1);
            eprintln!("test {idx}, hash, expected");
            eprintln!("{hash:?}");
            eprintln!("{:?}", test.2);
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
        (
            1231,
            1231,
            18001,
            vec![]
        )
        ];

        for (idx, test) in tests.iter().enumerate() {
            let adjacent_cell_ids =
                GpsProximityLookup::get_outer_cell_ids((test.0, test.1), test.2);
            eprintln!("test {idx}");
            eprintln!("adjacent {adjacent_cell_ids:?}");
            if test.3.is_empty() {
                assert!(adjacent_cell_ids.is_none());
            } else {
                let ids = adjacent_cell_ids.unwrap();
                assert_eq!(test.3.len(), ids.len());
                assert!(test
                    .3
                    .iter()
                    .all(|test_id| ids.iter().find(|id| *id == test_id).is_some()));
            }
        }
    }
}
