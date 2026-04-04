use std::fmt::{Debug, Display};

use serde::{Deserialize, Serialize};

use super::{
    graph::{ElementTagSetRef, MapDataPointRef},
    point::MapDataPoint,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LineDirection {
    BothWays = 0,
    OneWay = 1,
    Roundabout = 2,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct MapDataLine {
    // pub id: String,
    pub points: (MapDataPointRef, MapDataPointRef),
    pub direction: LineDirection,
    pub tags: ElementTagSetRef,
}
impl Display for MapDataLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Line({}-{})", self.points.0, self.points.1)
    }
}
impl MapDataLine {
    pub fn is_one_way(&self) -> bool {
        self.direction == LineDirection::OneWay || self.direction == LineDirection::Roundabout
    }
    pub fn is_roundabout(&self) -> bool {
        self.direction == LineDirection::Roundabout
    }
    pub fn len_m(&self, start: &MapDataPoint, end: &MapDataPoint) -> f32 {
        start.distance_between(end)
    }
}

impl PartialEq for MapDataLine {
    fn eq(&self, other: &Self) -> bool {
        self.points.0 == other.points.0 && self.points.1 == other.points.1
    }
}

impl Debug for MapDataLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapDataLine")
            .field("points", &self.points)
            .field("direction", &self.direction)
            .field("tags", &self.tags)
            .finish()
    }
}
