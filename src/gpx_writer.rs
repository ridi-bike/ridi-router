use geo::Point;
use gpx::{errors::GpxError, write, Gpx, GpxVersion, Route as GpxRoute, Waypoint};
use std::{collections::HashMap, fs::File, io::Error, isize, path::PathBuf};

use crate::{ipc_handler::RouteMessage, router::route::RouteStatElement};

#[derive(Debug, thiserror::Error)]
pub enum GpxWriterError {
    #[error("File Creation Error {error}")]
    FileCreateError { error: Error },

    #[error("Gpx Write Error {error}")]
    GpxWrite { error: GpxError },
}

pub struct GpxWriter {
    routes: Vec<RouteMessage>,
    file_name: PathBuf,
}

fn sort_by_longest(map: HashMap<String, RouteStatElement>) -> Vec<(String, RouteStatElement)> {
    let mut vec = Vec::from_iter(map);
    vec.sort_by(|a, b| b.1.len_m.total_cmp(&a.1.len_m));
    vec
}

impl GpxWriter {
    pub fn new(routes: Vec<RouteMessage>, file_name: PathBuf) -> Self {
        Self { routes, file_name }
    }
    pub fn write_gpx(self) -> Result<(), GpxWriterError> {
        #[cfg(not(feature = "debug-split-gpx"))]
        let mut gpx = {
            let mut gpx = Gpx::default();
            gpx.version = GpxVersion::Gpx11;
            gpx
        };
        for (idx, route) in self.routes.clone().into_iter().enumerate() {
            #[cfg(feature = "debug-split-gpx")]
            let mut gpx = {
                let mut gpx = Gpx::default();
                gpx.version = GpxVersion::Gpx11;
                gpx
            };
            let mut gpx_route = GpxRoute::new();
            gpx_route.name = Some(format!(
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

            gpx_route.description = Some(description);

            for (lat, lon) in &route.coords {
                let waypoint = Waypoint::new(Point::new(*lon as f64, *lat as f64));
                gpx_route.points.push(waypoint);
            }

            gpx.routes.push(gpx_route);
            #[cfg(feature = "debug-split-gpx")]
            {
                let mut filename = PathBuf::from(&self.file_name);
                filename.set_file_name(format!(
                    "{}_{}.gpx",
                    filename.file_name().unwrap().to_string_lossy(),
                    idx
                ));
                let file = File::create(&filename)
                    .map_err(|error| GpxWriterError::FileCreateError { error })?;

                write(&gpx, file).map_err(|error| GpxWriterError::GpxWrite { error })?;
            }
        }
        #[cfg(not(feature = "debug-split-gpx"))]
        {
            let file = File::create(&self.file_name)
                .map_err(|error| GpxWriterError::FileCreateError { error })?;

            write(&gpx, file).map_err(|error| GpxWriterError::GpxWrite { error })?;
        }

        Ok(())
    }
}
