use geo::Bearing;
use geo::Distance;
use geo::Haversine;
use geo::Point;
use serde::Deserialize;
use serde::Serialize;

use std::fmt::Debug;
use std::fmt::Display;

use super::graph::MapDataLineRef;
use super::graph::MapDataPointRef;
use super::rule::MapDataRule;

#[derive(Clone, Serialize, Deserialize)]
pub struct MapDataPoint {
    pub id: u64,
    pub lat: f32,
    pub lon: f32,
    pub lines: Vec<MapDataLineRef>,
    pub rules: Vec<MapDataRule>,
}

impl MapDataPoint {
    pub fn distance_between(&self, point: &MapDataPointRef) -> f32 {
        let self_geo = Point::new(self.lon, self.lat);
        let point_geo = Point::new(point.borrow().lon, point.borrow().lat);
        Haversine::distance(self_geo, point_geo)
    }
    pub fn bearing(&self, point: &MapDataPointRef) -> f32 {
        let self_geo = Point::new(self.lon, self.lat);
        let point_geo = Point::new(point.borrow().lon, point.borrow().lat);
        Haversine::bearing(self_geo, point_geo)
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

impl Display for MapDataPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Point({}: {}, {})", self.id, self.lat, self.lon)
    }
}
