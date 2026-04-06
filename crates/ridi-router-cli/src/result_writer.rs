use std::path::PathBuf;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    gpx_writer::{GpxWriter, GpxWriterError},
    json_writer::{JsonWriter, JsonWriterError},
};
use ridi_router_routing::RouteComputation;

#[derive(Debug, thiserror::Error)]
pub enum ResultWriterError {
    #[error("GPX writing failed: {error}")]
    Gpx { error: GpxWriterError },

    #[error("JSON writing failed: {error}")]
    Json { error: JsonWriterError },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum OutputFormat {
    Gpx,
    Json,
}

#[derive(Debug, Clone)]
pub struct RouteOutputRequest {
    pub output_dir: PathBuf,
    pub format: OutputFormat,
}

pub struct ResultWriter;
#[hotpath::measure_all]
impl ResultWriter {
    #[tracing::instrument(skip(computation))]
    pub fn write(
        request: RouteOutputRequest,
        computation: RouteComputation,
    ) -> Result<(), ResultWriterError> {
        match request.format {
            OutputFormat::Gpx => {
                info!(output_dir = ?request.output_dir, "Writing GPX routes");
                GpxWriter::new(computation.routes, request.output_dir)
                    .write_gpx()
                    .map_err(|error| ResultWriterError::Gpx { error })
            }
            OutputFormat::Json => {
                info!(output_dir = ?request.output_dir, "Writing JSON routes");
                JsonWriter::new(computation.routes, request.output_dir)
                    .write_json()
                    .map_err(|error| ResultWriterError::Json { error })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        path::PathBuf,
        process,
        time::{SystemTime, UNIX_EPOCH},
    };

    use ridi_router_routing::{ComputedRoute, RouteComputation, RouteStatElement, RouteStats};

    use super::{OutputFormat, ResultWriter, RouteOutputRequest};

    fn unique_test_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "ridi-router-result-writer-{}-{}",
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
            junction_count: 3,
            highway: HashMap::from([(
                "primary".to_string(),
                RouteStatElement {
                    len_m,
                    percentage: 100.0,
                },
            )]),
            surface: HashMap::new(),
            smoothness: HashMap::new(),
            score: 12.5,
            cluster: Some(1),
            approximated_route: vec![(1.0, 2.0), (3.0, 4.0)],
        }
    }

    fn test_route() -> ComputedRoute {
        ComputedRoute {
            coords: vec![(48.1, 11.5), (48.2, 11.6)],
            stats: test_stats(12_345.0),
        }
    }

    #[test]
    fn result_writer_writes_only_requested_format() {
        let json_output_dir = unique_test_dir();
        fs::create_dir_all(&json_output_dir).unwrap();

        ResultWriter::write(
            RouteOutputRequest {
                output_dir: json_output_dir.clone(),
                format: OutputFormat::Json,
            },
            RouteComputation {
                routes: vec![test_route()],
            },
        )
        .unwrap();

        assert!(json_output_dir.join("001-12km.json").exists());
        assert!(!json_output_dir.join("001-12km.gpx").exists());
        fs::remove_dir_all(&json_output_dir).unwrap();

        let gpx_output_dir = unique_test_dir();
        fs::create_dir_all(&gpx_output_dir).unwrap();

        ResultWriter::write(
            RouteOutputRequest {
                output_dir: gpx_output_dir.clone(),
                format: OutputFormat::Gpx,
            },
            RouteComputation {
                routes: vec![test_route()],
            },
        )
        .unwrap();

        assert!(gpx_output_dir.join("001-12km.gpx").exists());
        assert!(!gpx_output_dir.join("001-12km.json").exists());
        fs::remove_dir_all(gpx_output_dir).unwrap();
    }

    #[test]
    fn result_writer_leaves_output_dir_empty_when_routes_are_empty() {
        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();

        ResultWriter::write(
            RouteOutputRequest {
                output_dir: output_dir.clone(),
                format: OutputFormat::Json,
            },
            RouteComputation { routes: vec![] },
        )
        .unwrap();

        assert_eq!(fs::read_dir(&output_dir).unwrap().count(), 0);
        fs::remove_dir_all(output_dir).unwrap();
    }

    #[test]
    fn json_output_adapter_uses_routing_result_type() {
        let computation = RouteComputation {
            routes: vec![test_route()],
        };
        assert_eq!(computation.routes[0].stats.len_m, 12_345.0);
    }
}
