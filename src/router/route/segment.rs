use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::map_data::graph::{MapDataLineRef, MapDataPointRef};

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
    pub fn get_bearing(&self) -> f32 {
        if self.end_point == self.line.borrow().points.0 {
            return self
                .line
                .borrow()
                .points
                .0
                .borrow()
                .bearing(&self.line.borrow().points.1);
        }
        self.line
            .borrow()
            .points
            .1
            .borrow()
            .bearing(&self.line.borrow().points.0)
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
