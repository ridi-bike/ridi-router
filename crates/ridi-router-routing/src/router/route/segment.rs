use std::fmt::Debug;

use geo::Bearing;

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
    pub fn get_bearing(&self) -> f32 {
        if self.end_point == self.line.get().points.0 {
            return self
                .line
                .get()
                .points
                .0
                .get()
                .bearing(&self.line.get().points.1);
        }
        self.line
            .get()
            .points
            .1
            .get()
            .bearing(&self.line.get().points.0)
    }

    pub(crate) fn get_bearing_with_context(&self, ctx: &RoutingContext<'_>) -> f32 {
        let line = ctx.line(&self.line);
        if self.end_point == line.points.0 {
            let from = ctx.point(&line.points.0);
            let to = ctx.point(&line.points.1);
            let from_geo = geo::Point::new(from.lon, from.lat);
            let to_geo = geo::Point::new(to.lon, to.lat);
            return geo::Haversine.bearing(from_geo, to_geo);
        }
        let from = ctx.point(&line.points.1);
        let to = ctx.point(&line.points.0);
        let from_geo = geo::Point::new(from.lon, from.lat);
        let to_geo = geo::Point::new(to.lon, to.lat);
        geo::Haversine.bearing(from_geo, to_geo)
    }
}

impl Debug for Segment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let line = self.get_line().get().clone();
        let point = self.get_end_point().get().clone();
        write!(f, "line:\n\t{:#?}\npoint:\n\t{:#?}", line, point)
    }
}

impl From<(MapDataLineRef, MapDataPointRef)> for Segment {
    fn from(value: (MapDataLineRef, MapDataPointRef)) -> Self {
        Segment::new(value.0, value.1)
    }
}
