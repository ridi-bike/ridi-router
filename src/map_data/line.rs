use std::fmt::Debug;

use super::graph::{MapDataElementTagRef, MapDataPointRef, MapDataWayRef};

#[derive(Clone, PartialEq)]
pub enum LineDirection {
    BothWays = 0,
    OneWay = 1,
    Roundabout = 2,
}

#[derive(Clone)]
pub struct MapDataLine {
    // pub id: String,
    pub way: MapDataWayRef,
    pub points: (MapDataPointRef, MapDataPointRef),
    pub direction: LineDirection,
    pub tags: (MapDataElementTagRef, MapDataElementTagRef),
}
impl MapDataLine {
    pub fn line_id(&self) -> String {
        format!(
            "{}-{}-{}",
            self.way.borrow().id,
            self.points.0.borrow().id,
            self.points.1.borrow().id
        )
    }
    pub fn tag_name(&self) -> Option<&String> {
        self.tags.0.get()
    }
    pub fn tag_ref(&self) -> Option<&String> {
        self.tags.1.get()
    }
    pub fn is_one_way(&self) -> bool {
        self.direction == LineDirection::OneWay
    }
    pub fn is_roundabout(&self) -> bool {
        self.direction == LineDirection::Roundabout
    }
}

impl PartialEq for MapDataLine {
    fn eq(&self, other: &Self) -> bool {
        self.points.0 == other.points.0 && self.points.1 == other.points.1
    }
}

impl Debug for MapDataLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MapDataLine
    id={}
    way={}
    points=({},{})
    one_way={}
    roundabout={}",
            self.line_id(),
            self.way.borrow().id,
            self.points.0.borrow().id,
            self.points.1.borrow().id,
            self.is_one_way(),
            self.direction == LineDirection::Roundabout
        )
    }
}
