use std::{
    io::{self, Write},
    path::PathBuf,
};

use tracing::info;

use crate::{
    gpx_writer::{GpxWriter, GpxWriterError},
    ipc_handler::ResponseMessage,
};

#[derive(Debug)]
pub enum ResultWriterError {
    SerializeJson { error: serde_json::Error },
    Gpx { error: GpxWriterError },
    RoutesGenerationFailed { error: String },
    Stdout { error: io::Error },
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

                info!("Writing {} bytes of json to stdout", json.as_bytes().len());

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
                    info!("Writing gpx {:?}", file);

                    GpxWriter::new(routes, file.clone())
                        .write_gpx()
                        .map_err(|error| ResultWriterError::Gpx { error })?;

                    Ok(())
                }
            },
            DataDestination::Json { file } => {
                let json = serde_json::to_string(&response)
                    .map_err(|error| ResultWriterError::SerializeJson { error })?;

                info!(
                    "Writing {} bytes of json to {:?}",
                    json.as_bytes().len(),
                    file
                );
                std::fs::write(file, json)
                    .map_err(|error| ResultWriterError::FileWrite { error })?;

                Ok(())
            }
        }
    }
}
