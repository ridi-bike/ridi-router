use core::panic;
use geo::{point, HaversineBearing, HaversineDistance, Point};
use std::{
    cell::{RefCell, RefMut},
    collections::{BTreeMap, HashMap},
    fmt::Debug,
    rc::Rc,
    slice::Iter,
    u64, usize,
};

use crate::gps_hash::{get_gps_coords_hash, HashOffset};

#[derive(Debug, PartialEq, Clone)]
pub enum MapDataError {
    MissingPoint {
        point_id: u64,
    },
    MissingRestriction {
        relation_id: u64,
    },
    UnknownRestriction {
        relation_id: u64,
        restriction: String,
    },
    MissingViaNode {
        relation_id: u64,
    },
    MissingViaPoint {
        point_id: u64,
    },
    WayIdNotLinkedWithViaPoint {
        relation_id: u64,
        point_id: u64,
        way_id: u64,
    },
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

#[derive(Debug, Clone, PartialEq)]
pub enum OsmRelationMemberType {
    Way,
    Node,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OsmRelationMemberRole {
    From,
    To,
    Via,
    Other(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct OsmRelationMember {
    pub member_type: OsmRelationMemberType,
    pub role: OsmRelationMemberRole,
    pub member_ref: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MapDataRuleType {
    OnlyAllowed,
    NotAllowed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OsmRelation {
    pub id: u64,
    pub members: Vec<OsmRelationMember>,
    pub tags: HashMap<String, String>,
}

#[derive(Clone)]
pub struct MapDataRule {
    pub from_lines: Vec<MapDataLineRef>,
    pub to_lines: Vec<MapDataLineRef>,
    pub rule_type: MapDataRuleType,
}

#[derive(Clone)]
pub struct MapDataPoint {
    pub id: u64,
    pub lat: f64,
    pub lon: f64,
    pub part_of_ways: Vec<MapDataWayRef>,
    pub lines: Vec<MapDataLineRef>,
    pub fork: bool,
    pub rules: Vec<MapDataRule>,
}

impl PartialEq for MapDataPoint {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Debug for MapDataPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MapDataPoint
    id={}
    lat={}
    lon={}
    part_of_ways={:?}
    lines={:?}
    fork={}",
            self.id,
            self.lat,
            self.lon,
            self.part_of_ways
                .iter()
                .map(|w| w.borrow().id)
                .collect::<Vec<_>>(),
            self.lines
                .iter()
                .map(|l| l.borrow().id.clone())
                .collect::<Vec<_>>(),
            self.fork
        )
    }
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

    pub fn is_first_or_last(&self, point: &MapDataPointRef) -> bool {
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

#[derive(Clone)]
pub struct MapDataWay {
    pub id: u64,
    pub points: MapDataWayPoints,
}

impl MapDataWay {
    pub fn add_point(way: MapDataWayRef, point: MapDataPointRef) -> () {
        let mut way_mut = way.borrow_mut();
        way_mut.points.add(point);
    }
}

impl PartialEq for MapDataWay {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Debug for MapDataWay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MapDataWay
    id={}
    points={:?}",
            self.id,
            self.points
                .iter()
                .map(|p| p.borrow().id)
                .collect::<Vec<_>>(),
        )
    }
}

#[derive(Clone)]
pub struct MapDataLine {
    pub id: String,
    pub way: MapDataWayRef,
    pub points: (MapDataPointRef, MapDataPointRef),
}

impl PartialEq for MapDataLine {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Debug for MapDataLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MapDataLine
    id={}
    way={}
    points=({},{})",
            self.id,
            self.way.borrow().id,
            self.points.0.borrow().id,
            self.points.1.borrow().id,
        )
    }
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
            rules: Vec::new(),
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

    fn way_is_ok(&self, _osm_way: &OsmWay) -> bool {
        true
    }

    pub fn insert_way(&mut self, osm_way: OsmWay) -> Result<(), MapDataError> {
        if !self.way_is_ok(&osm_way) {
            return Ok(());
        }
        let mut prev_point: Option<MapDataPointRef> = None;
        let way = Rc::new(RefCell::new(MapDataWay {
            id: osm_way.id,
            points: MapDataWayPoints::new(),
        }));
        for point_id in &osm_way.point_ids {
            if let Some(point) = self.points.get(&point_id) {
                let mut way_mut = way.borrow_mut();
                way_mut.points.add(Rc::clone(&point));
            }

            if let Some(point) = self.points.get(&point_id) {
                let mut point_mut = point.borrow_mut();
                point_mut.part_of_ways.push(Rc::clone(&way));
            }

            if let Some(point) = self.points.get(&point_id) {
                let point_fork = if point.borrow().part_of_ways.len() > 2 {
                    true
                } else if let Some(other_way) = point
                    .borrow()
                    .part_of_ways
                    .iter()
                    .find(|&w| w.borrow().id != way.borrow().id)
                {
                    !other_way.borrow().points.is_first_or_last(&point)
                        || !way.borrow().points.is_first_or_last(&point)
                } else {
                    false
                };
                let mut point_mut = point.borrow_mut();
                point_mut.fork = point_fork;
            }

            if let Some(point) = self.points.get(&point_id) {
                let mut point_mut = point.borrow_mut();
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
                    point_mut.lines.push(Rc::clone(&line));

                    let mut prev_point_mut = prev_point.borrow_mut();
                    prev_point_mut.lines.push(line);
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

    fn relation_is_ok(&self, relation: &OsmRelation) -> bool {
        if let Some(rel_type) = relation.tags.get("type") {
            // https://wiki.openstreetmap.org/w/index.php?title=Relation:restriction&uselang=en
            // currently only "restriction", but "restriction:bus" was in use until 2013
            if rel_type.starts_with("restriction") {
                return true;
            }
        }
        false
    }

    pub fn insert_relation(&mut self, relation: OsmRelation) -> Result<(), MapDataError> {
        if !self.relation_is_ok(&relation) {
            return Ok(());
        }
        let restriction = relation
            .tags
            .get("restriction")
            .or(relation.tags.get("restriction:motorcycle"))
            .ok_or(MapDataError::MissingRestriction {
                relation_id: relation.id,
            })?;
        let rule_type = match restriction.as_str() {
            "no_right_turn" => MapDataRuleType::NotAllowed,
            "no_left_turn" => MapDataRuleType::NotAllowed,
            "no_u_turn" => MapDataRuleType::NotAllowed,
            "no_straight_on" => MapDataRuleType::NotAllowed,
            "no_entry" => MapDataRuleType::NotAllowed,
            "no_exit" => MapDataRuleType::NotAllowed,
            "only_right_turn" => MapDataRuleType::OnlyAllowed,
            "only_left_turn" => MapDataRuleType::OnlyAllowed,
            "only_u_turn" => MapDataRuleType::OnlyAllowed,
            "only_straight_on" => MapDataRuleType::OnlyAllowed,
            restriction => {
                return Err(MapDataError::UnknownRestriction {
                    relation_id: relation.id,
                    restriction: restriction.to_string(),
                })
            }
        };

        let via_members = relation
            .members
            .iter()
            .filter(|member| member.role == OsmRelationMemberRole::Via)
            .collect::<Vec<_>>();
        if via_members.len() == 1 {
            let via_node = via_members.first().ok_or(MapDataError::MissingViaNode {
                relation_id: relation.id,
            })?;
            let via_point =
                self.points
                    .get(&via_node.member_ref)
                    .ok_or(MapDataError::MissingViaPoint {
                        point_id: via_node.member_ref,
                    })?;
            fn get_way_ids(
                members: &Vec<OsmRelationMember>,
                role: OsmRelationMemberRole,
            ) -> Vec<u64> {
                members
                    .iter()
                    .filter_map(|member| {
                        if member.role == role {
                            return Some(member.member_ref);
                        }
                        None
                    })
                    .collect::<Vec<_>>()
            }
            fn get_lines_from_way_ids(
                way_ids: &Vec<u64>,
                point: &MapDataPointRef,
                relation_id: u64,
            ) -> Result<Vec<MapDataLineRef>, MapDataError> {
                way_ids
                    .iter()
                    .map(|way_id| {
                        point
                            .borrow()
                            .lines
                            .iter()
                            .find(|line| line.borrow().way.borrow().id == *way_id)
                            .ok_or(MapDataError::WayIdNotLinkedWithViaPoint {
                                relation_id,
                                point_id: point.borrow().id,
                                way_id: *way_id,
                            })
                            .map(|line| Rc::clone(line))
                    })
                    .collect()
            }
            let from_way_ids = get_way_ids(&relation.members, OsmRelationMemberRole::From);
            let from_lines = get_lines_from_way_ids(&from_way_ids, &via_point, relation.id)?;
            let to_way_ids = get_way_ids(&relation.members, OsmRelationMemberRole::To);
            let to_lines = get_lines_from_way_ids(&to_way_ids, &via_point, relation.id)?;
            let mut point = via_point.borrow_mut();
            let rule = MapDataRule {
                from_lines,
                to_lines,
                rule_type,
            };
            point.rules.push(rule);
        } else {
            panic!("not yet implemented relations with via ways");
        }
        Ok(())
    }

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

    #[derive(Debug)]
    struct PointTest {
        lat: f64,
        lon: f64,
        ways: Vec<u64>,
        lines: Vec<&'static str>,
        fork: bool,
    }

    #[test]
    fn check_point_consistency() {
        fn point_is_ok(map_data: &MapDataGraph, id: &u64, test: PointTest) -> bool {
            let point = map_data
                .get_point_by_id(id)
                .expect(format!("point {} must exist", id).as_str());
            let point = point.borrow();
            eprintln!("point {:#?}", point);
            eprintln!("test {:#?}", test);
            point.lat == test.lat
                && point.lon == test.lon
                && point.part_of_ways.len() == test.ways.len()
                && point.part_of_ways.iter().enumerate().all(|(idx, w)| {
                    let test_way_id = test
                        .ways
                        .get(idx)
                        .expect(format!("{}: way at idx {} must exist", id, idx).as_str());
                    w.borrow().id == *test_way_id
                })
                && point.lines.len() == test.lines.len()
                && point.lines.iter().enumerate().all(|(idx, l)| {
                    let test_line_id = test
                        .lines
                        .get(idx)
                        .expect(format!("{}: line at idx {} must exist", id, idx).as_str());
                    l.borrow().id == *test_line_id
                })
                && point.fork == test.fork
        }
        let map_data = get_test_map_data_graph();
        assert!(point_is_ok(
            &map_data,
            &1,
            PointTest {
                lat: 1.0,
                lon: 1.0,
                ways: vec![1234],
                lines: vec!["1234-1-2"],
                fork: false
            }
        ));
        assert!(point_is_ok(
            &map_data,
            &2,
            PointTest {
                lat: 2.0,
                lon: 2.0,
                ways: vec![1234],
                lines: vec!["1234-1-2", "1234-2-3"],
                fork: false
            }
        ));
        assert!(point_is_ok(
            &map_data,
            &3,
            PointTest {
                lat: 3.0,
                lon: 3.0,
                ways: vec![1234, 5367],
                lines: vec!["1234-2-3", "1234-3-4", "5367-5-3", "5367-3-6"],
                fork: true
            }
        ));
        assert!(point_is_ok(
            &map_data,
            &4,
            PointTest {
                lat: 4.0,
                lon: 4.0,
                ways: vec![1234, 489],
                lines: vec!["1234-3-4", "489-4-8"],
                fork: false
            }
        ));
        assert!(point_is_ok(
            &map_data,
            &5,
            PointTest {
                lat: 5.0,
                lon: 5.0,
                ways: vec![5367],
                lines: vec!["5367-5-3"],
                fork: false
            }
        ));
        assert!(point_is_ok(
            &map_data,
            &6,
            PointTest {
                lat: 6.0,
                lon: 6.0,
                ways: vec![5367, 68],
                lines: vec!["5367-3-6", "5367-6-7", "68-6-8"],
                fork: true
            }
        ));
        assert!(point_is_ok(
            &map_data,
            &7,
            PointTest {
                lat: 7.0,
                lon: 7.0,
                ways: vec![5367],
                lines: vec!["5367-6-7"],
                fork: false
            }
        ));
        assert!(point_is_ok(
            &map_data,
            &8,
            PointTest {
                lat: 8.0,
                lon: 8.0,
                ways: vec![489, 68],
                lines: vec!["489-4-8", "489-8-9", "68-6-8"],
                fork: true
            }
        ));
        assert!(point_is_ok(
            &map_data,
            &9,
            PointTest {
                lat: 9.0,
                lon: 9.0,
                ways: vec![489],
                lines: vec!["489-8-9"],
                fork: false
            }
        ));
        assert!(point_is_ok(
            &map_data,
            &11,
            PointTest {
                lat: 11.0,
                lon: 11.0,
                ways: vec![1112],
                lines: vec!["1112-11-12"],
                fork: false
            }
        ));
        assert!(point_is_ok(
            &map_data,
            &12,
            PointTest {
                lat: 12.0,
                lon: 12.0,
                ways: vec![1112],
                lines: vec!["1112-11-12"],
                fork: false
            }
        ));
    }

    #[test]
    fn check_way_consistency() {
        fn way_is_ok(map_data: &MapDataGraph, id: &u64, test_points: Vec<u64>) -> bool {
            let way = map_data
                .ways
                .get(id)
                .expect(format!("way {} must exist", id).as_str());
            let way = way.borrow();
            eprintln!("way {:#?}", way);
            eprintln!("test {:#?}", test_points);
            way.points.points.len() == test_points.len()
                && way.points.points.iter().enumerate().all(|(idx, p)| {
                    let p = p.borrow();
                    p.id == *test_points
                        .get(idx)
                        .expect(format!("point at idx {} must exist", idx).as_str())
                })
        }
        let map_data = get_test_map_data_graph();

        assert!(way_is_ok(&map_data, &1234, vec![1, 2, 3, 4]));
        assert!(way_is_ok(&map_data, &5367, vec![5, 3, 6, 7]));
        assert!(way_is_ok(&map_data, &489, vec![4, 8, 9]));
        assert!(way_is_ok(&map_data, &68, vec![6, 8]));
        assert!(way_is_ok(&map_data, &1112, vec![11, 12]));
    }

    #[test]
    fn check_line_consistency() {
        fn line_is_ok(
            map_data: &MapDataGraph,
            id: &str,
            test_way: u64,
            test_points: (u64, u64),
        ) -> bool {
            let line = map_data
                .lines
                .get(id)
                .expect(format!("line {} must exist", id).as_str());
            let line = line.borrow();
            eprintln!("line {:#?}", line);
            eprintln!("test {:#?}", test_points);
            line.way.borrow().id == test_way
                && line.points.0.borrow().id == test_points.0
                && line.points.1.borrow().id == test_points.1
        }
        let map_data = get_test_map_data_graph();
        assert!(line_is_ok(&map_data, "1234-1-2", 1234, (1, 2)));
        assert!(line_is_ok(&map_data, "1234-2-3", 1234, (2, 3)));
        assert!(line_is_ok(&map_data, "1234-3-4", 1234, (3, 4)));
        assert!(line_is_ok(&map_data, "5367-5-3", 5367, (5, 3)));
        assert!(line_is_ok(&map_data, "5367-3-6", 5367, (3, 6)));
        assert!(line_is_ok(&map_data, "5367-6-7", 5367, (6, 7)));
        assert!(line_is_ok(&map_data, "489-4-8", 489, (4, 8)));
        assert!(line_is_ok(&map_data, "489-8-9", 489, (8, 9)));
        assert!(line_is_ok(&map_data, "68-6-8", 68, (6, 8)));
        assert!(line_is_ok(&map_data, "1112-11-12", 1112, (11, 12)));
    }

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
        points.iter().for_each(|p| {
            assert!((p.1.borrow().id == 3 && p.1.borrow().fork == true) || p.1.borrow().id != 3)
        });

        let point = map_data.get_point_by_id(&3).unwrap();
        let points = map_data.get_adjacent(point);
        let non_forks = vec![2, 5, 4];
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
            assert_eq!(adj_elements.len(), expected_result.len());
            for (adj_line, adj_point) in &adj_elements {
                let adj_match = expected_result.iter().find(|&(line_id, point_id)| {
                    line_id.split("-").collect::<HashSet<_>>()
                        == adj_line.borrow().id.split("-").collect::<HashSet<_>>()
                        && point_id == &adj_point.borrow().id
                });
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
                    // 57.16961885299059,24.875192642211914
                    // 10231.8212 km
                    // 223.61
                    OsmNode {
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                    },
                    // 57.159484808175435,24.877617359161377
                    // 10231.6372 km
                    // 223.61
                    OsmNode {
                        id: 2,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                    },
                ],
                // -10.660607953624762,-52.03125
                OsmNode {
                    id: 0,
                    lat: -10.660607953624762,
                    lon: -52.03125,
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
                assert_eq!(closest.borrow().id, *closest_id);
            } else {
                panic!("No points found");
            }
        }
    }
}
