use geo::HaversineBearing;
use geo::HaversineDistance;
use geo::Point;

use std::fmt::Debug;

use super::graph::MapDataLineRef;
use super::graph::MapDataPointRef;
use super::graph::MapDataWayRef;
use super::rule::MapDataRule;

#[derive(Clone)]
pub struct MapDataPoint {
    pub id: u64,
    pub lat: f64,
    pub lon: f64,
    pub lines: Vec<MapDataLineRef>,
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
    pub fn is_junction(&self) -> bool {
        self.lines.len() > 2
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
    lines={:?}
    junction={}
    rules={:#?}",
            self.id,
            self.lat,
            self.lon,
            self.lines
                .iter()
                .map(|l| l.borrow().line_id())
                .collect::<Vec<_>>(),
            self.is_junction(),
            self.rules
        )
    }
}
