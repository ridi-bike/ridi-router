use clap::builder::Str;
use geo::Point;
use gpx::{write, Gpx, GpxVersion, Route as GpxRoute, Track, TrackSegment, Waypoint};
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
            let mut gpx_route = GpxRoute::new();

            let mut description = String::new();
            description.push_str(&format!("Length: {}km\n", route.stats.len_m / 1000.));
            description.push_str(&format!(
                "Number of junctions: {}\n",
                route.stats.junction_count
            ));
            description.push_str(&format!("Road types:\n"));
            for (road_type, stat) in route.stats.highway.iter() {
                description.push_str(&format!(
                    " - {road_type}: {}km, {}%\n",
                    (stat.len_m / 10.).round() / 100.0,
                    (stat.percentage * 100.).round() / 100.
                ));
            }
            description.push_str(&format!("Road surface:\n"));
            for (surface_type, stat) in route.stats.surface.iter() {
                description.push_str(&format!(
                    " - {surface_type}: {}km, {}%\n",
                    (stat.len_m / 10.).round() / 100.0,
                    (stat.percentage * 100.).round() / 100.
                ));
            }
            description.push_str(&format!("Road smoothness:\n"));
            for (smoothness_type, stat) in route.stats.smoothness.iter() {
                description.push_str(&format!(
                    " - {smoothness_type}: {}km, {}%\n",
                    (stat.len_m / 10.).round() / 100.0,
                    (stat.percentage * 100.).round() / 100.
                ));
            }

            gpx_route.description = Some(description);

            for coord in route.coords {
                let waypoint = Waypoint::new(Point::new(coord.lon.into(), coord.lat.into()));
                gpx_route.points.push(waypoint);
            }

            gpx.routes.push(gpx_route);
        }

        let file = File::create(self.file_name)
            .or_else(|error| Err(GpxWriterError::FileCreateError { error }))?;

        write(&gpx, file).unwrap();

        Ok(())
    }
}
