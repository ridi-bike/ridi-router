use geo::Bearing;
use geo::Distance;
use geo::Haversine;
use geo::Point;
use serde::Deserialize;
use serde::Serialize;

use std::fmt::Debug;
use std::fmt::Display;

use super::graph::MapDataLineRef;
use super::rule::MapDataRule;

#[derive(Clone, Serialize, Deserialize)]
pub struct MapDataPoint {
    pub id: u64,
    pub lat: f32,
    pub lon: f32,
    pub lines: Vec<MapDataLineRef>,
    pub rules: Vec<MapDataRule>,
    pub residential_in_proximity: bool,
    pub nogo_area: bool,
}

impl MapDataPoint {
    fn to_geo_point(&self) -> Point<f32> {
        Point::new(self.lon, self.lat)
    }

    pub fn distance_between(&self, point: &MapDataPoint) -> f32 {
        Haversine.distance(self.to_geo_point(), point.to_geo_point())
    }

    pub fn bearing(&self, point: &MapDataPoint) -> f32 {
        Haversine.bearing(self.to_geo_point(), point.to_geo_point())
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
        f.debug_struct("MapDataPoint")
            .field("id", &self.id)
            .field("lat", &self.lat)
            .field("lon", &self.lon)
            .field("line_count", &self.lines.len())
            .field("rule_count", &self.rules.len())
            .field("residential_in_proximity", &self.residential_in_proximity)
            .field("nogo_area", &self.nogo_area)
            .finish()
    }
}

impl Display for MapDataPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Point({}: {}, {})", self.id, self.lat, self.lon)
    }
}
