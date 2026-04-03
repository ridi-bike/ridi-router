use geo::Point;
use gpx::{errors::GpxError, write, Gpx, GpxVersion, Route as GpxRoute, Waypoint};
use std::{collections::HashMap, fs::File, io::Error, isize, path::PathBuf};

use crate::file_naming::route_file_path;
use ridi_router_routing::{ComputedRoute, RouteStatElement};

#[derive(Debug, thiserror::Error)]
pub enum GpxWriterError {
    #[error("Failed to create GPX file '{path:?}': {error}")]
    FileCreateError { path: PathBuf, error: Error },

    #[error("Failed to write GPX file '{path:?}': {error}")]
    GpxWrite { path: PathBuf, error: GpxError },
}

pub struct GpxWriter {
    routes: Vec<ComputedRoute>,
    output_dir: PathBuf,
}

fn sort_by_longest(map: HashMap<String, RouteStatElement>) -> Vec<(String, RouteStatElement)> {
    let mut vec = Vec::from_iter(map);
    vec.sort_by(|a, b| b.1.len_m.total_cmp(&a.1.len_m));
    vec
}

impl GpxWriter {
    pub fn new(routes: Vec<ComputedRoute>, output_dir: PathBuf) -> Self {
        Self { routes, output_dir }
    }

    pub fn write_gpx(self) -> Result<(), GpxWriterError> {
        for (idx, route) in self.routes.into_iter().enumerate() {
            let path = route_file_path(&self.output_dir, idx, route.stats.len_m, "gpx");
            let gpx = Self::build_gpx(route, idx);
            let file = File::create(&path).map_err(|error| GpxWriterError::FileCreateError {
                path: path.clone(),
                error,
            })?;

            write(&gpx, file).map_err(|error| GpxWriterError::GpxWrite {
                path: path.clone(),
                error,
            })?;
        }

        Ok(())
    }

    fn build_gpx(route: ComputedRoute, idx: usize) -> Gpx {
        let mut gpx = Gpx::default();
        gpx.version = GpxVersion::Gpx11;

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
        gpx
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs::{self, File}, io::BufReader, path::PathBuf, process, time::{SystemTime, UNIX_EPOCH}};

    use ridi_router_routing::{ComputedRoute, RouteStatElement, RouteStats};

    use super::{sort_by_longest, GpxWriter};

    fn unique_test_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "ridi-router-gpx-writer-{}-{}",
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
    fn gpx_writer_writes_one_file_per_route() {
        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();

        GpxWriter::new(
            vec![
                test_route(vec![(48.1, 11.5), (48.2, 11.6)], 10_000.0),
                test_route(vec![(49.1, 12.5), (49.2, 12.6)], 20_000.0),
            ],
            output_dir.clone(),
        )
        .write_gpx()
        .unwrap();

        let entries = fs::read_dir(&output_dir).unwrap().count();
        fs::remove_dir_all(output_dir).unwrap();

        assert_eq!(entries, 2);
    }

    #[test]
    fn gpx_writer_uses_expected_filename_pattern() {
        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();

        GpxWriter::new(
            vec![test_route(vec![(48.1, 11.5), (48.2, 11.6)], 12_345.0)],
            output_dir.clone(),
        )
        .write_gpx()
        .unwrap();

        assert!(output_dir.join("001-12km.gpx").exists());
        fs::remove_dir_all(output_dir).unwrap();
    }

    #[test]
    fn gpx_writer_accepts_arbitrary_route_order_without_sorting() {
        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();

        GpxWriter::new(
            vec![
                test_route(vec![(48.1, 11.5), (48.2, 11.6)], 1_000.0),
                test_route(vec![(49.1, 12.5), (49.2, 12.6)], 2_000.0),
            ],
            output_dir.clone(),
        )
        .write_gpx()
        .unwrap();

        assert!(output_dir.join("001-1km.gpx").exists());
        assert!(output_dir.join("002-2km.gpx").exists());
        fs::remove_dir_all(output_dir).unwrap();
    }

    #[test]
    fn write_gpx_includes_route_points_in_each_written_file() {
        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();

        GpxWriter::new(
            vec![test_route(
                vec![(48.1, 11.5), (48.2, 11.6), (48.3, 11.7)],
                10_000.0,
            )],
            output_dir.clone(),
        )
        .write_gpx()
        .unwrap();

        let parsed = gpx::read(BufReader::new(
            File::open(output_dir.join("001-10km.gpx")).unwrap(),
        ))
        .unwrap();
        fs::remove_dir_all(output_dir).unwrap();

        assert_eq!(parsed.routes.len(), 1);
        assert_eq!(parsed.routes[0].points.len(), 3);
    }

    #[test]
    fn gpx_output_adapter_uses_routing_result_type() {
        let route = test_route(vec![(48.1, 11.5), (48.2, 11.6)], 12_345.0);
        assert_eq!(route.stats.len_m, 12_345.0);
    }

    #[test]
    fn cli_gpx_output_can_diverge_from_library_struct_shape() {
        let gpx = GpxWriter::build_gpx(
            test_route(vec![(48.1, 11.5), (48.2, 11.6)], 12_345.0),
            0,
        );

        assert_eq!(gpx.routes.len(), 1);
        assert_eq!(gpx.routes[0].points.len(), 2);
        assert_eq!(gpx.routes[0].name.as_deref(), Some("r_0_c_2"));
        assert!(gpx.routes[0]
            .description
            .as_deref()
            .unwrap()
            .contains("Length: 12.35km"));
    }
}
