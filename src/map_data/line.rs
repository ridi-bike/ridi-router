use std::fmt::Debug;

use super::graph::{MapDataElementTagRef, MapDataPointRef, MapDataWayRef};

#[derive(Clone)]
pub struct MapDataLine {
    // pub id: String,
    pub way: MapDataWayRef,
    pub points: (MapDataPointRef, MapDataPointRef),
    pub one_way: bool,
    pub roundabout: bool,
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
            self.one_way,
            self.roundabout
        )
    }
}
