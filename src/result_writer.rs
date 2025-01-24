use std::{
    io::{self, Write},
    path::PathBuf,
};

use tracing::{info, trace};

use crate::{
    gpx_writer::{GpxWriter, GpxWriterError},
    ipc_handler::ResponseMessage,
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

pub struct ResultWriter;
impl ResultWriter {
    #[tracing::instrument(skip(response))]
    pub fn write(
        dest: DataDestination,
        response: ResponseMessage,
    ) -> Result<(), ResultWriterError> {
        match dest {
            DataDestination::Stdout => {
                let json = serde_json::to_string(&response)
                    .map_err(|error| ResultWriterError::SerializeJson { error })?;

                trace!(bytes_len = json.as_bytes().len(), "Writing json to stdout");

                std::io::stdout()
                    .write_all(json.as_bytes())
                    .map_err(|error| ResultWriterError::Stdout { error })?;
                Ok(())
            }
            DataDestination::Gpx { file } => match response.result {
                crate::ipc_handler::RouterResult::Error { message } => {
                    Err(ResultWriterError::RoutesGenerationFailed { error: message })
                }
                crate::ipc_handler::RouterResult::Ok { routes } => {
                    info!(file = ?file, "Writing gpx");

                    GpxWriter::new(routes, file.clone())
                        .write_gpx()
                        .map_err(|error| ResultWriterError::Gpx { error })?;

                    Ok(())
                }
            },
            DataDestination::Json { file } => {
                let json = serde_json::to_string(&response)
                    .map_err(|error| ResultWriterError::SerializeJson { error })?;

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
