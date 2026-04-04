use std::fmt::Debug;


use crate::{map_data::graph::{MapDataLineRef, MapDataPointRef}, RoutingContext};

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
    pub(crate) fn bearing(&self, ctx: &RoutingContext<'_>) -> f32 {
        let line = ctx.line(&self.line);
        if self.end_point == line.points.0 {
            let from = ctx.point(&line.points.0);
            let to = ctx.point(&line.points.1);
            return from.bearing(&to);
        }
        let from = ctx.point(&line.points.1);
        let to = ctx.point(&line.points.0);
        from.bearing(&to)
    }
}

impl Debug for Segment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Segment(line={}, end_point={})", self.line, self.end_point)
    }
}

impl From<(MapDataLineRef, MapDataPointRef)> for Segment {
    fn from(value: (MapDataLineRef, MapDataPointRef)) -> Self {
        Segment::new(value.0, value.1)
    }
}
