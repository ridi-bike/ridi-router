use crate::map_data_graph::{MapDataLineRef, MapDataPointRef};
use std::fmt::Debug;

#[derive(PartialEq, Clone)]
pub struct RouteSegment {
    line: MapDataLineRef,
    end_point: MapDataPointRef,
}

impl RouteSegment {
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

impl Debug for RouteSegment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let line = self.get_line().borrow().clone();
        let point = self.get_end_point().borrow().clone();
        write!(f, "line:\n\t{:#?}\npoint:\n\t{:#?}", line, point)
    }
}

impl From<(MapDataLineRef, MapDataPointRef)> for RouteSegment {
    fn from(value: (MapDataLineRef, MapDataPointRef)) -> Self {
        RouteSegment::new(value.0, value.1)
    }
}
