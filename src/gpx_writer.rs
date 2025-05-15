use geo::Point;
use gpx::{
    errors::GpxError, write, Gpx, GpxVersion, Route as GpxRoute, Track as GpxTrack,
    TrackSegment as GpxTrackSegment, Waypoint,
};
use std::{collections::HashMap, fs::File, io::Error, isize, path::PathBuf};

use crate::{
    ipc_handler::{DeadEndMessage, RouteMessage},
    router::route::RouteStatElement,
};

#[derive(Debug, thiserror::Error)]
pub enum GpxWriterError {
    #[error("File Creation Error {error}")]
    FileCreateError { error: Error },

    #[error("Gpx Write Error {error}")]
    GpxWrite { error: GpxError },
}

pub struct GpxWriter {
    routes: Vec<RouteMessage>,
    dead_ends: Option<Vec<DeadEndMessage>>,
    file_name: PathBuf,
}

fn sort_by_longest(map: HashMap<String, RouteStatElement>) -> Vec<(String, RouteStatElement)> {
    let mut vec = Vec::from_iter(map);
    vec.sort_by(|a, b| b.1.len_m.total_cmp(&a.1.len_m));
    vec
}

impl GpxWriter {
    pub fn new(
        routes: Vec<RouteMessage>,
        dead_ends: Option<Vec<DeadEndMessage>>,
        file_name: PathBuf,
    ) -> Self {
        Self {
            routes,
            dead_ends,
            file_name,
        }
    }
    pub fn write_gpx(self) -> Result<(), GpxWriterError> {
        let mut gpx = Gpx::default();
        gpx.version = GpxVersion::Gpx11;

        for (idx, route) in self.routes.clone().into_iter().enumerate() {
            let mut gpx_track = GpxTrack::new();
            gpx_track.name = Some(format!(
                "r_{idx}_c_{}",
                route.stats.cluster.map_or(-1, |c| c as isize)
            ));

            let mut description = String::new();
            description.push_str(&format!("Length: {:.2}km\n", route.stats.len_m / 1000.));
            description.push_str(&format!(
                "Number of junctions: {}\n",
                route.stats.junction_count
            ));
            description.push_str(&format!(
                "Cluster: {}\n",
                route.stats.cluster.map_or(-1, |c| c as isize)
            ));
            description.push_str(&format!("Score: {:.2}\n", route.stats.score));
            description.push_str("Road types:\n");
            for (road_type, stat) in sort_by_longest(route.stats.highway).iter() {
                description.push_str(&format!(
                    " - {road_type}: {:.2}km, {:.2}%\n",
                    stat.len_m / 1000.,
                    stat.percentage,
                ));
            }
            description.push_str("Road surface:\n");
            for (surface_type, stat) in sort_by_longest(route.stats.surface).iter() {
                description.push_str(&format!(
                    " - {surface_type}: {:.2}km, {:.2}%\n",
                    stat.len_m / 1000.,
                    stat.percentage,
                ));
            }
            description.push_str("Road smoothness:\n");
            for (smoothness_type, stat) in sort_by_longest(route.stats.smoothness).iter() {
                description.push_str(&format!(
                    " - {smoothness_type}: {:.2}km, {:.2}%\n",
                    stat.len_m / 1000.,
                    stat.percentage,
                ));
            }

            gpx_track.description = Some(description);

            if let Some(coords_by_tags) = route.coords_by_tags {
                for segment in coords_by_tags {
                    let mut gpx_segment = GpxTrackSegment::new();
                    for (lat, lon) in segment.coords {
                        let waypoint = Waypoint::new(Point::new(lon as f64, lat as f64));
                        gpx_segment.points.push(waypoint);
                    }
                    gpx_track.segments.push(gpx_segment);
                }
            } else {
                let mut gpx_segment = GpxTrackSegment::new();
                for (lat, lon) in &route.coords {
                    let waypoint = Waypoint::new(Point::new(*lon as f64, *lat as f64));
                    gpx_segment.points.push(waypoint);
                }
                gpx_track.segments.push(gpx_segment);
            }

            gpx.tracks.push(gpx_track);
        }

        if let Some(dead_ends) = self.dead_ends {
            dead_ends.iter().for_each(|dead_end| {
                let mut waypoint = Waypoint::new(Point::new(
                    dead_end.coords.1.into(),
                    dead_end.coords.0.into(),
                ));
                waypoint.name = Some(format!("Dead end: {}", dead_end.dead_end_type));
                gpx.waypoints.push(waypoint)
            });
        }

        let file = File::create(&self.file_name)
            .map_err(|error| GpxWriterError::FileCreateError { error })?;

        write(&gpx, file).map_err(|error| GpxWriterError::GpxWrite { error })?;

        Ok(())
    }
}
