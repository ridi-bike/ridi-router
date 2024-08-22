use std::fmt::Debug;

use super::graph::{MapDataPointRef, MapDataWayRef};

#[derive(Clone)]
pub struct MapDataLine {
    pub id: String,
    pub way: MapDataWayRef,
    pub points: (MapDataPointRef, MapDataPointRef),
    pub one_way: bool,
    pub roundabout: bool,
    pub tags_ref: Option<String>,
    pub tags_name: Option<String>,
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
    points=({},{})
    one_way={}
    roundabout={}",
            self.id,
            self.way.borrow().id,
            self.points.0.borrow().id,
            self.points.1.borrow().id,
            self.one_way,
            self.roundabout
        )
    }
}
