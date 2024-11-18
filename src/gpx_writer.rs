use geo::Point;
use gpx::{write, Gpx, GpxVersion, Track, TrackSegment, Waypoint};
use std::{fs::File, io::Error};

use crate::{map_data::graph::MapDataPointRef, router::route::Route};

#[derive(Debug)]
pub enum RoutesWriterError {
    FileCreateError { error: Error },
}

pub struct RoutesWriter {
    start_point: MapDataPointRef,
    routes: Vec<Route>,
    file_name: Option<String>,
    start_lat: f32,
    start_lon: f32,
}

impl RoutesWriter {
    pub fn new(
        start_point: MapDataPointRef,
        routes: Vec<Route>,
        start_lat: f32,
        start_lon: f32,
        file_name: Option<String>,
    ) -> Self {
        Self {
            start_point,
            routes,
            file_name,
            start_lat,
            start_lon,
        }
    }
    pub fn write_gpx(self) -> Result<(), RoutesWriterError> {
        let mut gpx = Gpx::default();
        gpx.version = GpxVersion::Gpx11;

        for route in self.routes {
            let mut track_segment = TrackSegment::new();

            let waypoint = Waypoint::new(Point::new(self.start_lon.into(), self.start_lat.into()));
            track_segment.points.push(waypoint);

            let waypoint = Waypoint::new(Point::new(
                self.start_point.borrow().lon.into(),
                self.start_point.borrow().lat.into(),
            ));
            track_segment.points.push(waypoint);

            for segment in route {
                let waypoint = Waypoint::new(Point::new(
                    segment.get_end_point().borrow().lon.into(),
                    segment.get_end_point().borrow().lat.into(),
                ));
                track_segment.points.push(waypoint);
            }

            let mut track = Track::new();
            track.segments.push(track_segment);

            gpx.tracks.push(track);
        }

        if let Some(file_name) = self.file_name {
            let file = File::create(format!("/home/toms/dev/moto-router/debug/{}", file_name))
                .or_else(|error| Err(RoutesWriterError::FileCreateError { error }))?;

            write(&gpx, file).unwrap();
        } else {
            write(&gpx, std::io::stdout()).unwrap();
        }

        Ok(())
    }
}
