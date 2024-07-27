use geo::{point, HaversineBearing, HaversineDistance, Point};
use std::{
    cell::{RefCell, RefMut},
    collections::{BTreeMap, HashMap},
    rc::Rc,
    slice::Iter,
    u64, usize,
};

use crate::gps_hash::{get_gps_coords_hash, HashOffset};

#[derive(Debug, PartialEq, Clone)]
pub enum MapDataError {
    MissingPoint { point_id: u64 },
    MissingWay { way_id: u64 },
}

pub type MapDataWayRef = Rc<RefCell<MapDataWay>>;
pub type MapDataPointRef = Rc<RefCell<MapDataPoint>>;
pub type MapDataLineRef = Rc<RefCell<MapDataLine>>;

#[derive(Clone, Debug, PartialEq)]
pub struct OsmNode {
    pub id: u64,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OsmWay {
    pub id: u64,
    pub point_ids: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapDataPoint {
    pub id: u64,
    pub lat: f64,
    pub lon: f64,
    pub part_of_ways: Vec<MapDataWayRef>,
    pub lines: Vec<MapDataLineRef>,
    pub fork: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapDataWayPoints {
    points: Vec<MapDataPointRef>,
}
impl MapDataWayPoints {
    pub fn new() -> Self {
        Self { points: Vec::new() }
    }
    pub fn from_vec(points: Vec<MapDataPointRef>) -> Self {
        Self { points }
    }
    pub fn is_first_or_last(&self, point: MapDataPointRef) -> bool {
        let is_first = if let Some(ref first) = self.points.first() {
            first.borrow().id == point.borrow().id
        } else {
            false
        };
        let is_last = if let Some(ref last) = self.points.last() {
            last.borrow().id == point.borrow().id
        } else {
            false
        };

        is_first || is_last
    }

    pub fn get_after(&self, idx: usize) -> Option<&MapDataPointRef> {
        self.points.get(idx + 1)
    }

    pub fn get_before(&self, idx: usize) -> Option<&MapDataPointRef> {
        if idx == 0 {
            return None;
        }
        self.points.get(idx - 1)
    }

    pub fn iter(&self) -> Iter<'_, MapDataPointRef> {
        self.points.iter()
    }

    pub fn add(&mut self, point: MapDataPointRef) -> () {
        self.points.push(point);
    }
}

impl<'a> IntoIterator for &'a MapDataWayPoints {
    type Item = &'a MapDataPointRef;

    type IntoIter = std::slice::Iter<'a, MapDataPointRef>;

    fn into_iter(self) -> Self::IntoIter {
        self.points.iter()
    }
}

impl<'a> IntoIterator for MapDataWayPoints {
    type Item = MapDataPointRef;

    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.points.into_iter()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapDataWay {
    pub id: u64,
    pub points: MapDataWayPoints,
}

// #[derive(Clone, Debug, PartialEq)]
// pub struct MapDataRelation {
//     pub id: u64,
//     pub node_ids: MapDataWayNodes,
//     pub one_way: bool,
//     pub members: [MapDataRelationMember; 3],
// }

// #[derive(Clone, Debug, PartialEq)]
// pub enum MapDataRelationMember {
//     From { way_id: u64 },
//     To { way_id: u64 },
//     Via { node_id: u64 },
// }

#[derive(Clone, Debug, PartialEq)]
pub struct MapDataLine {
    pub id: String,
    pub way: MapDataWayRef,
    pub points: (MapDataPointRef, MapDataPointRef),
    // pub length_m: f64,
    // pub bearing_deg: f64,
    // pub one_way: bool,
    // pub accessible_from_line_ids: Vec<String>,
}

type PointMap = BTreeMap<u64, MapDataPointRef>;

pub struct MapDataGraph {
    points: HashMap<u64, Rc<RefCell<MapDataPoint>>>,
    point_hashed_offset_none: PointMap,
    point_hashed_offset_lat: PointMap,
    nodes_hashed_offset_lon: PointMap,
    nodes_hashed_offset_lat_lon: PointMap,
    ways: HashMap<u64, MapDataWayRef>,
    lines: HashMap<String, MapDataLineRef>,
}

fn get_line_id(way_id: &u64, point_id_1: &u64, point_id_2: &u64) -> String {
    format!("{}-{}-{}", way_id, point_id_1, point_id_2)
}

impl MapDataGraph {
    pub fn new() -> Self {
        Self {
            points: HashMap::new(),
            point_hashed_offset_none: BTreeMap::new(),
            point_hashed_offset_lat: BTreeMap::new(),
            nodes_hashed_offset_lon: BTreeMap::new(),
            nodes_hashed_offset_lat_lon: BTreeMap::new(),
            ways: HashMap::new(),
            lines: HashMap::new(),
        }
    }

    pub fn get_point_by_id(&self, id: &u64) -> Option<MapDataPointRef> {
        self.points.get(id).map(|p| Rc::clone(p))
    }

    pub fn insert_node(&mut self, value: OsmNode) -> () {
        let lat = value.lat.clone();
        let lon = value.lon.clone();
        let point = Rc::new(RefCell::new(MapDataPoint {
            id: value.id,
            lat: value.lat,
            lon: value.lon,
            part_of_ways: Vec::new(),
            lines: Vec::new(),
            fork: false,
        }));
        self.point_hashed_offset_none.insert(
            get_gps_coords_hash(lat.clone(), lon.clone(), HashOffset::None),
            Rc::clone(&point),
        );
        self.point_hashed_offset_none.insert(
            get_gps_coords_hash(lat.clone(), lon.clone(), HashOffset::Lat),
            Rc::clone(&point),
        );
        self.point_hashed_offset_none.insert(
            get_gps_coords_hash(lat.clone(), lon.clone(), HashOffset::Lon),
            Rc::clone(&point),
        );
        self.point_hashed_offset_none.insert(
            get_gps_coords_hash(lat, lon, HashOffset::LatLon),
            Rc::clone(&point),
        );
        let id = point.borrow().id.clone();
        self.points.insert(id, point);
    }

    pub fn insert_way(&mut self, osm_way: OsmWay) -> Result<(), MapDataError> {
        let mut prev_point: Option<MapDataPointRef> = None;
        let way = Rc::new(RefCell::new(MapDataWay {
            id: osm_way.id,
            points: MapDataWayPoints::new(),
        }));
        for point_id in &osm_way.point_ids {
            if let Some(point) = self.points.get(&point_id) {
                let mut point_mut: RefMut<'_, _> = point.borrow_mut();

                point_mut.part_of_ways.push(Rc::clone(&way));
                if point_mut.part_of_ways.len() > 2 {
                    point_mut.fork = true;
                } else if let Some(other_way) = point_mut
                    .part_of_ways
                    .iter()
                    .find(|&w| w.borrow().id != way.borrow().id)
                {
                    point_mut.fork = !other_way
                        .borrow()
                        .points
                        .is_first_or_last(Rc::clone(&point))
                        || !way.borrow().points.is_first_or_last(Rc::clone(&point));
                }
                if let Some(prev_point) = &prev_point {
                    let line_id = get_line_id(&way.borrow().id, &prev_point.borrow().id, &point_id);
                    // let prev_point_geo = Point::new(prev_point.lon, prev_point.lat);
                    // let point_geo = Point::new(point_mut.lon, point_mut.lat);
                    let line = Rc::new(RefCell::new(MapDataLine {
                        id: line_id,
                        way: Rc::clone(&way),
                        points: (Rc::clone(&prev_point), Rc::clone(&point)),
                        // length_m: prev_point_geo.haversine_distance(&point_geo),
                        // bearing_deg: prev_point_geo.haversine_bearing(point_geo),
                        // one_way: way.one_way,
                        // accessible_from_line_ids: Vec::new(),
                    }));
                    self.lines
                        .insert(line.borrow().id.clone(), Rc::clone(&line));
                    point_mut.lines.push(line);
                }
                prev_point = Some(Rc::clone(&point));
            } else {
                return Err(MapDataError::MissingPoint {
                    point_id: point_id.clone(),
                });
            }
        }
        self.ways.insert(way.borrow().id.clone(), Rc::clone(&way));

        Ok(())
    }

    // pub fn insert_relation(&mut self, relation: MapDataRelation) -> Result<(), MapDataError> {}

    pub fn get_adjacent(
        &self,
        center_point: MapDataPointRef,
    ) -> Vec<(MapDataLineRef, MapDataPointRef)> {
        center_point
            .borrow()
            .lines
            .iter()
            .map(|line| {
                let other_point = if line.borrow().points.0 == center_point {
                    Rc::clone(&line.borrow().points.1)
                } else {
                    Rc::clone(&line.borrow().points.0)
                };
                (Rc::clone(&line), other_point)
            })
            .collect()
        // let lines_and_points: Vec<_> = center_point
        //     .borrow()
        //     .part_of_ways
        //     .iter()
        //     .map(|way| {
        //         let center_point_idx_on_way = way
        //             .borrow()
        //             .points
        //             .iter()
        //             .position(|&point| point == center_point);
        //         if let Some(center_point_idx_on_way) = center_point_idx_on_way {
        //             let point_before = way.borrow().points.get_before(center_point_idx_on_way);
        //             let point_after = way.borrow().points.get_after(center_point_idx_on_way);
        //             return [point_before, point_after]
        //                 .iter()
        //                 .filter_map(|&point| point)
        //                 .map(|&point| {
        //                     let line_id_bck =
        //                         get_line_id(way.borrow().id, &point.id, &center_point.borrow().id);
        //                     let line_id_fwd =
        //                         get_line_id(&way.id, &center_point.borrow().id, &point.id);
        //                     let line_bck = self.lines.get(&line_id_bck);
        //                     let line_fwd = self.lines.get(&line_id_fwd);
        //                     [line_bck, line_fwd]
        //                         .iter()
        //                         .filter_map(|&line| line)
        //                         .map(|line| (line.clone(), point.clone()))
        //                         .collect::<Vec<_>>()
        //                 })
        //                 .flatten()
        //                 .collect::<Vec<_>>();
        //         }
        //
        //         Vec::new()
        //     })
        //     .flatten()
        //     .collect();
        //
        // lines_and_points
    }

    pub fn get_closest_to_coords(&self, lat: f64, lon: f64) -> Option<MapDataPointRef> {
        let search_hash = get_gps_coords_hash(lat, lon, HashOffset::None);
        let mut grid_points = HashMap::new();

        for level in 0..=32 {
            let shift_width = 2 * level;
            let from = search_hash >> shift_width << shift_width;
            let to = from
                | if shift_width > 0 {
                    u64::max_value() >> (64 - shift_width)
                } else {
                    search_hash
                };

            let offset_none_points = self.point_hashed_offset_none.range(from..=to);
            let offset_lat_points = self.point_hashed_offset_lat.range(from..=to);
            let offset_lon_points = self.nodes_hashed_offset_lon.range(from..=to);
            let offset_lat_lon_points = self.nodes_hashed_offset_lat_lon.range(from..=to);
            let points: [Vec<Rc<RefCell<MapDataPoint>>>; 4] = [
                offset_none_points
                    .map(|(_, point)| Rc::clone(&point))
                    .collect(),
                offset_lat_points
                    .map(|(_, point)| Rc::clone(&point))
                    .collect(),
                offset_lon_points
                    .map(|(_, point)| Rc::clone(&point))
                    .collect(),
                offset_lat_lon_points
                    .map(|(_, point)| Rc::clone(&point))
                    .collect(),
            ];

            let points = points.concat();
            if !points.is_empty() || (from == 0 && to == u64::max_value()) {
                points.iter().for_each(|p| {
                    let id: u64 = p.borrow().id.clone();
                    grid_points.insert(id, Rc::clone(&p));
                });
                break;
            }
        }

        if grid_points.len() == 1 {
            let point = grid_points.values().next().map(|p| Rc::clone(p));
            return point;
        }

        let mut points_with_dist: Vec<(u32, Rc<RefCell<MapDataPoint>>)> = grid_points
            .iter()
            .map(|(_, p)| {
                let point1 = Point::new(p.borrow().lon, p.borrow().lat);
                let point2 = Point::new(lon, lat);
                let distance = point1.haversine_distance(&point2);
                (distance.round() as u32, Rc::clone(&p))
            })
            .collect();

        points_with_dist.sort_by(|(dist_a, _), (dist_b, _)| dist_a.cmp(dist_b));
        points_with_dist.get(0).map(|(_, p)| Rc::clone(p))
    }
}

#[cfg(test)]
mod tests {
    use core::panic;
    use std::{collections::HashSet, u8};

    use crate::test_utils::{get_test_data, get_test_map_data_graph};

    use super::*;

    #[test]
    fn check_missing_points() {
        let mut map_data = MapDataGraph::new();
        let res = map_data.insert_way(OsmWay {
            id: 1,
            point_ids: vec![1],
        });
        if let Ok(_) = res {
            assert!(false);
        } else if let Err(e) = res {
            if let MapDataError::MissingPoint { point_id: p } = e {
                assert_eq!(p, 1);
            } else {
                assert!(false);
            }
        }
    }

    #[test]
    fn mark_forks() {
        let map_data = get_test_map_data_graph();
        let point = map_data.get_point_by_id(&5).unwrap();
        let points = map_data.get_adjacent(point);
        eprintln!("points {:#?}", points);
        points.iter().for_each(|p| {
            assert!((p.1.borrow().id == 3 && p.1.borrow().fork == true) || p.1.borrow().id != 3)
        });

        let point = map_data.get_point_by_id(&3).unwrap();
        let points = map_data.get_adjacent(point);
        let non_forks = vec![2, 5, 4];
        eprintln!("points {:#?}", points);
        points.iter().for_each(|p| {
            assert!(
                ((non_forks.contains(&p.1.borrow().id) && p.1.borrow().fork == false)
                    || !non_forks.contains(&p.1.borrow().id))
            )
        });
        points.iter().for_each(|p| {
            assert!((p.1.borrow().id == 6 && p.1.borrow().fork == true) || p.1.borrow().id != 6)
        });
    }

    #[test]
    fn adjacent_lookup() {
        let test_data = get_test_data();
        let mut map_data = MapDataGraph::new();

        let (test_nodes, test_ways) = &test_data;
        for test_node in test_nodes {
            map_data.insert_node(test_node.clone());
        }
        for test_way in test_ways {
            map_data.insert_way(test_way.clone()).unwrap();
        }

        let tests: Vec<(u8, MapDataPointRef, Vec<(String, u64)>)> = vec![
            (
                1,
                map_data.get_point_by_id(&2).unwrap(),
                vec![(String::from("1234-1-2"), 1), (String::from("1234-2-3"), 3)],
            ),
            (
                2,
                map_data.get_point_by_id(&3).unwrap(),
                vec![
                    (String::from("5367-5-3"), 5),
                    (String::from("5367-6-3"), 6),
                    (String::from("1234-2-3"), 2),
                    (String::from("1234-4-3"), 4),
                ],
            ),
            (
                3,
                map_data.get_point_by_id(&1).unwrap(),
                vec![(String::from("1234-1-2"), 2)],
            ),
        ];

        for test in tests {
            let (test_id, point, expected_result) = test;
            let adj_elements = map_data.get_adjacent(point);
            eprintln!(
                "id: {}, expected {} results, found {} results",
                test_id,
                expected_result.len(),
                adj_elements.len()
            );
            assert_eq!(adj_elements.len(), expected_result.len());
            for (adj_line, adj_point) in &adj_elements {
                let adj_match = expected_result.iter().find(|&(line_id, point_id)| {
                    line_id.split("-").collect::<HashSet<_>>()
                        == adj_line.borrow().id.split("-").collect::<HashSet<_>>()
                        && point_id == &adj_point.borrow().id
                });
                eprintln!(
                    "id: {}, expected {:?}, found {:?}",
                    test_id, expected_result, adj_elements
                );
                assert_eq!(adj_match.is_some(), true);
            }
        }
    }

    #[test]
    fn closest_lookup() {
        let tests: Vec<(Vec<OsmNode>, OsmNode, u64)> = vec![
            (
                vec![OsmNode {
                    id: 1,
                    lat: 57.1640,
                    lon: 24.8652,
                }],
                OsmNode {
                    id: 0,
                    lat: 57.1670,
                    lon: 24.8658,
                },
                1,
            ),
            (
                vec![
                    OsmNode {
                        id: 1,
                        lat: 57.1640,
                        lon: 24.8652,
                    },
                    OsmNode {
                        id: 2,
                        lat: 57.1740,
                        lon: 24.8630,
                    },
                ],
                OsmNode {
                    id: 0,
                    lat: 57.1670,
                    lon: 24.8658,
                },
                1,
            ),
            (
                vec![
                    OsmNode {
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                    },
                    OsmNode {
                        id: 2,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                    },
                ],
                OsmNode {
                    id: 0,
                    lat: 57.163429387682214,
                    lon: 24.87742424011231,
                },
                2,
            ),
            (
                vec![
                    OsmNode {
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                    },
                    OsmNode {
                        id: 2,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                    },
                ],
                OsmNode {
                    id: 0,
                    lat: 57.193343289610794,
                    lon: 24.872531890869144,
                },
                1,
            ),
            (
                vec![
                    OsmNode {
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                    },
                    OsmNode {
                        id: 2,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                    },
                ],
                OsmNode {
                    id: 0,
                    lat: -10.660607953624762,
                    lon: -52.03125,
                },
                1,
            ),
            (
                vec![
                    OsmNode {
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                    },
                    OsmNode {
                        id: 2,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                    },
                    OsmNode {
                        id: 3,
                        lat: 9.795677582829743,
                        lon: -1.7578125000000002,
                    },
                    OsmNode {
                        id: 4,
                        lat: -36.03133177633188,
                        lon: -65.21484375000001,
                    },
                ],
                OsmNode {
                    id: 0,
                    lat: -10.660607953624762,
                    lon: -52.03125,
                },
                4,
            ),
            (
                vec![
                    OsmNode {
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                    },
                    OsmNode {
                        id: 2,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                    },
                    OsmNode {
                        id: 3,
                        lat: 9.795677582829743,
                        lon: -1.7578125000000002,
                    },
                ],
                OsmNode {
                    id: 0,
                    lat: -10.660607953624762,
                    lon: -52.03125,
                },
                3,
            ),
            (
                vec![
                    OsmNode {
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                    },
                    OsmNode {
                        id: 2,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                    },
                    OsmNode {
                        id: 3,
                        lat: 9.795677582829743,
                        lon: -1.7578125000000002,
                    },
                    OsmNode {
                        id: 4,
                        lat: -36.03133177633188,
                        lon: -65.21484375000001,
                    },
                ],
                OsmNode {
                    id: 0,
                    lat: -28.92163128242129,
                    lon: 144.14062500000003,
                },
                4,
            ),
        ];
        for (i, test) in tests.iter().enumerate() {
            let (points, check_point, closest_id) = test;
            let mut coords = MapDataGraph::new();
            for point in points {
                coords.insert_node(point.clone());
            }

            let closest = coords.get_closest_to_coords(check_point.lat, check_point.lon);
            if let Some(closest) = closest {
                eprintln!(
                    "{}: closest found id {} expected {}",
                    i,
                    closest.borrow().id,
                    closest_id
                );
                assert_eq!(closest.borrow().id, *closest_id);
            } else {
                panic!("No points found");
            }
        }
    }
}
