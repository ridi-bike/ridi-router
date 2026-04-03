use std::{
    io::{self, Write},
    path::PathBuf,
};

use serde::Serialize;
use tracing::{info, trace};

use crate::{
    gpx_writer::{GpxWriter, GpxWriterError},
    route_output::RouteComputation,
};

#[derive(Debug, thiserror::Error)]
pub enum ResultWriterError {
    #[error("JSON Serialization error {error}")]
    SerializeJson { error: serde_json::Error },

    #[error("GPX writing failed: {error}")]
    Gpx { error: GpxWriterError },

    #[error("Failed to generate routes: {error}")]
    RoutesGenerationFailed { error: String },

    #[error("Failed to write to stdout: {error}")]
    Stdout { error: io::Error },

    #[error("Failed to write to file: {error}")]
    FileWrite { error: io::Error },
}

#[derive(Debug, Clone)]
pub enum DataDestination {
    Stdout,
    Gpx { file: PathBuf },
    Json { file: PathBuf },
}

#[derive(Serialize)]
struct ErrorResult<'a> {
    message: &'a str,
}

pub struct ResultWriter;
impl ResultWriter {
    fn serialize_result(
        result: &Result<RouteComputation, String>,
    ) -> Result<String, ResultWriterError> {
        match result {
            Ok(computation) => serde_json::to_string(computation)
                .map_err(|error| ResultWriterError::SerializeJson { error }),
            Err(message) => serde_json::to_string(&ErrorResult { message })
                .map_err(|error| ResultWriterError::SerializeJson { error }),
        }
    }

    #[tracing::instrument(skip(result))]
    pub fn write(
        dest: DataDestination,
        result: Result<RouteComputation, String>,
    ) -> Result<(), ResultWriterError> {
        match dest {
            DataDestination::Stdout => {
                let json = Self::serialize_result(&result)?;

                trace!(bytes_len = json.as_bytes().len(), "Writing json to stdout");

                std::io::stdout()
                    .write_all(json.as_bytes())
                    .map_err(|error| ResultWriterError::Stdout { error })?;
                Ok(())
            }
            DataDestination::Gpx { file } => match result {
                Err(message) => Err(ResultWriterError::RoutesGenerationFailed { error: message }),
                Ok(computation) => {
                    info!(file = ?file, "Writing gpx");

                    GpxWriter::new(computation.routes, file.clone())
                        .write_gpx()
                        .map_err(|error| ResultWriterError::Gpx { error })?;

                    Ok(())
                }
            },
            DataDestination::Json { file } => {
                let json = Self::serialize_result(&result)?;

                trace!(
                    bytes_len = json.as_bytes().len(),
                    destination = ?file,
                    "Writing json"
                );

                std::fs::write(file, json)
                    .map_err(|error| ResultWriterError::FileWrite { error })?;

                Ok(())
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

    use crate::{
        route_output::{ComputedRoute, RouteComputation},
        router::route::{RouteStatElement, RouteStats},
    };

    use super::{DataDestination, ResultWriter, ResultWriterError};

    fn unique_test_path(extension: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ridi-router-result-writer-{}-{}.{}",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            extension
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
    fn write_json_serializes_transport_neutral_result() {
        let file = unique_test_path("json");
        let result = Ok(RouteComputation {
            routes: vec![test_route()],
        });

        ResultWriter::write(DataDestination::Json { file: file.clone() }, result).unwrap();

        let written = fs::read_to_string(&file).unwrap();
        fs::remove_file(&file).unwrap();

        assert!(written.contains("\"routes\""));
        assert!(written.contains("\"coords\""));
        assert!(written.contains("\"len_m\":12345.0"));
    }

    #[test]
    fn write_gpx_path_uses_transport_neutral_routes() {
        let file = unique_test_path("gpx");
        let result = Ok(RouteComputation {
            routes: vec![test_route()],
        });

        ResultWriter::write(DataDestination::Gpx { file: file.clone() }, result).unwrap();

        let written = fs::read_to_string(&file).unwrap();
        fs::remove_file(&file).unwrap();

        assert!(written.contains("<gpx"));
        assert!(written.contains("<rtept"));
    }

    #[test]
    fn write_error_result_returns_routes_generation_failed() {
        let file = unique_test_path("gpx");
        let result = Err("route generation failed".to_string());

        let error = ResultWriter::write(DataDestination::Gpx { file }, result).unwrap_err();

        assert!(matches!(
            error,
            ResultWriterError::RoutesGenerationFailed { error }
                if error == "route generation failed"
        ));
    }
}
