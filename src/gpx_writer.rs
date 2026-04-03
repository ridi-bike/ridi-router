use geo::Point;
use gpx::{errors::GpxError, write, Gpx, GpxVersion, Route as GpxRoute, Waypoint};
use std::{collections::HashMap, fs::File, io::Error, isize, path::PathBuf};

use crate::{route_output::ComputedRoute, router::route::RouteStatElement};

#[derive(Debug, thiserror::Error)]
pub enum GpxWriterError {
    #[error("File Creation Error {error}")]
    FileCreateError { error: Error },

    #[error("Gpx Write Error {error}")]
    GpxWrite { error: GpxError },
}

pub struct GpxWriter {
    routes: Vec<ComputedRoute>,
    file_name: PathBuf,
}

fn sort_by_longest(map: HashMap<String, RouteStatElement>) -> Vec<(String, RouteStatElement)> {
    let mut vec = Vec::from_iter(map);
    vec.sort_by(|a, b| b.1.len_m.total_cmp(&a.1.len_m));
    vec
}

impl GpxWriter {
    pub fn new(routes: Vec<ComputedRoute>, file_name: PathBuf) -> Self {
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

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs::{self, File},
        io::BufReader,
        path::PathBuf,
        process,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        route_output::ComputedRoute,
        router::route::{RouteStatElement, RouteStats},
    };

    use super::{sort_by_longest, GpxWriter};

    fn unique_test_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "ridi-router-gpx-writer-{}-{}.gpx",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn test_stats(len_m: f64) -> RouteStats {
        RouteStats {
            len_m,
            junction_count: 2,
            highway: HashMap::from([(
                "primary".to_string(),
                RouteStatElement {
                    len_m,
                    percentage: 100.0,
                },
            )]),
            surface: HashMap::new(),
            smoothness: HashMap::new(),
            score: 7.5,
            cluster: Some(2),
            approximated_route: Vec::new(),
        }
    }

    fn test_route(coords: Vec<(f32, f32)>, len_m: f64) -> ComputedRoute {
        ComputedRoute {
            coords,
            stats: test_stats(len_m),
        }
    }

    #[test]
    fn sort_by_longest_sorts_descending_by_length() {
        let sorted = sort_by_longest(HashMap::from([
            (
                "short".to_string(),
                RouteStatElement {
                    len_m: 10.0,
                    percentage: 10.0,
                },
            ),
            (
                "long".to_string(),
                RouteStatElement {
                    len_m: 30.0,
                    percentage: 30.0,
                },
            ),
            (
                "mid".to_string(),
                RouteStatElement {
                    len_m: 20.0,
                    percentage: 20.0,
                },
            ),
        ]));

        assert_eq!(sorted[0].0, "long");
        assert_eq!(sorted[1].0, "mid");
        assert_eq!(sorted[2].0, "short");
    }

    #[test]
    fn write_gpx_accepts_transport_neutral_routes() {
        let file = unique_test_path();

        GpxWriter::new(
            vec![test_route(vec![(48.1, 11.5), (48.2, 11.6)], 10_000.0)],
            file.clone(),
        )
        .write_gpx()
        .unwrap();

        assert!(file.exists());
        fs::remove_file(file).unwrap();
    }

    #[test]
    fn write_gpx_includes_route_points_for_each_route() {
        let file = unique_test_path();

        GpxWriter::new(
            vec![
                test_route(vec![(48.1, 11.5), (48.2, 11.6)], 10_000.0),
                test_route(vec![(49.1, 12.5), (49.2, 12.6), (49.3, 12.7)], 20_000.0),
            ],
            file.clone(),
        )
        .write_gpx()
        .unwrap();

        let parsed = gpx::read(BufReader::new(File::open(&file).unwrap())).unwrap();
        fs::remove_file(file).unwrap();

        assert_eq!(parsed.routes.len(), 2);
        assert_eq!(parsed.routes[0].points.len(), 2);
        assert_eq!(parsed.routes[1].points.len(), 3);
    }
}
