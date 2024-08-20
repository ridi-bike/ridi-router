use std::fmt::Debug;

use crate::map_data::{line::MapDataLineRef, point::MapDataPointRef};

#[derive(PartialEq, Clone)]
pub struct Segment {
    line: MapDataLineRef,
    end_point: MapDataPointRef,
}

impl Segment {
    pub fn new(line: MapDataLineRef, end_point: MapDataPointRef) -> Self {
        Self { line, end_point }
    }
    pub fn get_end_point(&self) -> &MapDataPointRef {
        &self.end_point
    }
    pub fn get_line(&self) -> &MapDataLineRef {
        &self.line
    }
}

impl Debug for Segment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let line = self.get_line().borrow().clone();
        let point = self.get_end_point().borrow().clone();
        write!(f, "line:\n\t{:#?}\npoint:\n\t{:#?}", line, point)
    }
}

impl From<(MapDataLineRef, MapDataPointRef)> for Segment {
    fn from(value: (MapDataLineRef, MapDataPointRef)) -> Self {
        Segment::new(value.0, value.1)
    }
}
