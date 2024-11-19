use geo::Point;
use gpx::{write, Gpx, GpxVersion, Track, TrackSegment, Waypoint};
use std::{fs::File, io::Error, path::PathBuf};

use crate::{
    ipc_handler::{ResponseMessage, RouteMessage},
    map_data::graph::MapDataPointRef,
    router::route::Route,
};

#[derive(Debug)]
pub enum GpxWriterError {
    FileCreateError { error: Error },
}

pub struct GpxWriter {
    routes: Vec<RouteMessage>,
    file_name: PathBuf,
}

impl GpxWriter {
    pub fn new(routes: Vec<RouteMessage>, file_name: PathBuf) -> Self {
        Self { routes, file_name }
    }
    pub fn write_gpx(self) -> Result<(), GpxWriterError> {
        let mut gpx = Gpx::default();
        gpx.version = GpxVersion::Gpx11;

        for route in self.routes {
            let mut track_segment = TrackSegment::new();

            for coord in route.coords {
                let waypoint = Waypoint::new(Point::new(coord.lon.into(), coord.lat.into()));
                track_segment.points.push(waypoint);
            }

            let mut track = Track::new();
            track.segments.push(track_segment);

            gpx.tracks.push(track);
        }

        let file = File::create(self.file_name)
            .or_else(|error| Err(GpxWriterError::FileCreateError { error }))?;

        write(&gpx, file).unwrap();

        Ok(())
    }
}
