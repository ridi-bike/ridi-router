use std::{fs, io, path::PathBuf};

use crate::file_naming::route_file_path;
use ridi_router_routing::ComputedRoute;

#[derive(Debug, thiserror::Error)]
pub enum JsonWriterError {
    #[error("JSON serialization failed for '{path:?}': {error}")]
    SerializeJson {
        path: PathBuf,
        error: serde_json::Error,
    },

    #[error("Failed to write JSON file '{path:?}': {error}")]
    FileWrite { path: PathBuf, error: io::Error },
}

pub struct JsonWriter {
    routes: Vec<ComputedRoute>,
    output_dir: PathBuf,
}

impl JsonWriter {
    pub fn new(routes: Vec<ComputedRoute>, output_dir: PathBuf) -> Self {
        Self { routes, output_dir }
    }

    pub fn write_json(self) -> Result<(), JsonWriterError> {
        for (idx, route) in self.routes.into_iter().enumerate() {
            let path = route_file_path(&self.output_dir, idx, route.stats.len_m, "json");
            let json = serde_json::to_string(&route).map_err(|error| JsonWriterError::SerializeJson {
                path: path.clone(),
                error,
            })?;

            fs::write(&path, json).map_err(|error| JsonWriterError::FileWrite {
                path: path.clone(),
                error,
            })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs, path::PathBuf, process, time::{SystemTime, UNIX_EPOCH}};

    use ridi_router_routing::{ComputedRoute, RouteStatElement, RouteStats};

    use super::JsonWriter;

    fn unique_test_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "ridi-router-json-writer-{}-{}",
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
    fn json_writer_writes_one_file_per_route() {
        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();

        JsonWriter::new(
            vec![
                test_route(vec![(48.1, 11.5), (48.2, 11.6)], 10_000.0),
                test_route(vec![(49.1, 12.5), (49.2, 12.6)], 20_000.0),
            ],
            output_dir.clone(),
        )
        .write_json()
        .unwrap();

        let entries = fs::read_dir(&output_dir).unwrap().count();
        fs::remove_dir_all(output_dir).unwrap();

        assert_eq!(entries, 2);
    }

    #[test]
    fn json_writer_uses_expected_filename_pattern() {
        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();

        JsonWriter::new(
            vec![test_route(vec![(48.1, 11.5), (48.2, 11.6)], 12_345.0)],
            output_dir.clone(),
        )
        .write_json()
        .unwrap();

        assert!(output_dir.join("001-12km.json").exists());
        fs::remove_dir_all(output_dir).unwrap();
    }

    #[test]
    fn json_writer_serializes_each_route_as_standalone_document() {
        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();

        JsonWriter::new(
            vec![test_route(vec![(48.1, 11.5), (48.2, 11.6)], 12_345.0)],
            output_dir.clone(),
        )
        .write_json()
        .unwrap();

        let written = fs::read_to_string(output_dir.join("001-12km.json")).unwrap();
        fs::remove_dir_all(output_dir).unwrap();

        assert!(written.contains("\"coords\""));
        assert!(written.contains("\"len_m\":12345.0"));
        assert!(!written.contains("\"routes\""));
    }
}
