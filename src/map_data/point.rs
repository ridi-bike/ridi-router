use geo::HaversineBearing;
use geo::HaversineDistance;
use geo::Point;

use std::{cell::RefCell, fmt::Debug, rc::Rc};

use super::{line::MapDataLineRef, rule::MapDataRule, way::MapDataWayRef};

#[derive(Clone)]
pub struct MapDataPoint {
    pub id: u64,
    pub lat: f64,
    pub lon: f64,
    pub part_of_ways: Vec<MapDataWayRef>,
    pub lines: Vec<MapDataLineRef>,
    pub junction: bool,
    pub rules: Vec<MapDataRule>,
}

impl MapDataPoint {
    pub fn distance_between(&self, point: &MapDataPointRef) -> f64 {
        let self_geo = Point::new(self.lon, self.lat);
        let point_geo = Point::new(point.borrow().lon, point.borrow().lat);
        self_geo.haversine_distance(&point_geo)
    }
    pub fn bearing_to(&self, point: &MapDataPointRef) -> f64 {
        let self_geo = Point::new(self.lon, self.lat);
        let point_geo = Point::new(point.borrow().lon, point.borrow().lat);
        self_geo.haversine_bearing(point_geo)
    }
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
    junction={}
    rules={:#?}",
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
            self.junction,
            self.rules
        )
    }
}

pub type MapDataPointRef = Rc<RefCell<MapDataPoint>>;
