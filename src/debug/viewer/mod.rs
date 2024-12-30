use derive_name::Name;
use duckdb::{Connection, Result};
use std::{
    error::Error,
    ffi::OsString,
    fs::{self, File},
    io::{self, Cursor, Read},
    path::PathBuf,
    task::Wake,
};
use struct_field_names_as_array::FieldNamesAsSlice;
use tiny_http::{Header, Method, Request, Response, Server};
use tracing::info;

use crate::debug::writer::{
    DebugStreamForkChoiceWeights, DebugStreamForkChoices, DebugStreamItineraries,
    DebugStreamItineraryWaypoints, DebugStreamStepResults, DebugStreamSteps,
};

const FILES_URLS: [&str; 4] = ["/", "/viewer.js", "/van-1.5.2.js", "/van-1.5.2.debug.js"];

fn url_for_debug_stream_name(name: &str) -> String {
    format!("/data/{name}")
}

#[derive(Debug, thiserror::Error)]
pub enum DebugViewerError {
    #[error("Could not start server: {error}")]
    ServerStart {
        error: Box<dyn Error + Send + Sync + 'static>,
    },

    #[error("Could not start server")]
    HeaderCreate,

    #[error("Could not respond: {error}")]
    Respond { error: io::Error },

    #[error("Could not open file: {error}")]
    FileOpen { error: io::Error },

    #[error("Unexpected - can't happen")]
    Unexpected,

    #[error("Could not open db: {error}")]
    DbOpen { error: duckdb::Error },

    #[error("Failed to read debug dir: {error}")]
    ReadDebugDir { error: io::Error },

    #[error("Failed to read debug file in list: {error}")]
    ReadDebugFileInList { error: io::Error },

    #[error("Can't read file name")]
    CantReadFileName { error: OsString },

    #[error("Unexpected file found {file_name}")]
    UnexpectedFile { file_name: String },

    #[error("Could not execute db statement {error}")]
    DbStatementError { error: duckdb::Error },

    #[error("Could not serialize {error}")]
    Serialize { error: serde_json::Error },
}
pub struct DebugViewer;

impl DebugViewer {
    pub fn run(debug_dir: PathBuf) -> Result<(), DebugViewerError> {
        let db_conn =
            Connection::open_in_memory().map_err(|error| DebugViewerError::DbOpen { error })?;

        Self::prep_data(debug_dir, &db_conn)?;

        let addr = "0.0.0.0:1337";
        let server = Server::http(addr).map_err(|error| DebugViewerError::ServerStart { error })?;
        info!(addr, "Running Debug Viewer on http://{addr}");

        for request in server.incoming_requests() {
            if request.method() != &Method::Get {
                request
                    .respond(Response::from_string("not allowed").with_status_code(405))
                    .map_err(|error| DebugViewerError::Respond { error })?;
                continue;
            }

            if FILES_URLS.contains(&request.url()) {
                let response = DebugViewer::handle_file_request(&request)?;
                request
                    .respond(response)
                    .map_err(|error| DebugViewerError::Respond { error })?;
                continue;
            }

            if url_for_debug_stream_name(DebugStreamSteps::name()) == request.url()
                || url_for_debug_stream_name(DebugStreamStepResults::name()) == request.url()
                || url_for_debug_stream_name(DebugStreamForkChoices::name()) == request.url()
                || url_for_debug_stream_name(DebugStreamForkChoiceWeights::name()) == request.url()
                || url_for_debug_stream_name(DebugStreamItineraries::name()) == request.url()
                || url_for_debug_stream_name(DebugStreamItineraryWaypoints::name()) == request.url()
            {
                let response = DebugViewer::handle_data_request(&request, &db_conn)?;
                request
                    .respond(response)
                    .map_err(|error| DebugViewerError::Respond { error })?;
                continue;
            }
        }

        Ok(())
    }

    fn create_or_insert(
        db_con: &Connection,
        created_streams: &mut Vec<String>,
        name: &String,
        file_path: &String,
    ) -> Result<(), DebugViewerError> {
        if !created_streams.contains(name) {
            db_con
                .execute(
                    &format!(
                        "
                            CREATE TABLE {} AS
                                SELECT * FROM '{}';
                            ",
                        name, file_path
                    ),
                    [],
                )
                .map_err(|error| DebugViewerError::DbStatementError { error })?;
            created_streams.push(name.to_string());
        } else {
            db_con
                .execute(
                    &format!(
                        "
                            COPY {} FROM '{}';
                            ",
                        name, file_path
                    ),
                    [],
                )
                .map_err(|error| DebugViewerError::DbStatementError { error })?;
        }
        Ok(())
    }

    fn prep_data(debug_dir: PathBuf, db_con: &Connection) -> Result<(), DebugViewerError> {
        let dir_contents =
            fs::read_dir(debug_dir).map_err(|error| DebugViewerError::ReadDebugDir { error })?;
        let mut created_streams: Vec<String> = Vec::new();
        for debug_file in dir_contents {
            let debug_file =
                debug_file.map_err(|error| DebugViewerError::ReadDebugFileInList { error })?;
            let file_name = debug_file
                .file_name()
                .into_string()
                .map_err(|error| DebugViewerError::CantReadFileName { error })?;
            let file_path: String = debug_file
                .path()
                .into_os_string()
                .into_string()
                .map_err(|error| DebugViewerError::CantReadFileName { error })?;

            if file_name.starts_with(DebugStreamSteps::name()) {
                Self::create_or_insert(
                    &db_con,
                    &mut created_streams,
                    &DebugStreamSteps::name().to_string(),
                    &file_path,
                )?;
            }
            if file_name.starts_with(DebugStreamStepResults::name()) {
                Self::create_or_insert(
                    &db_con,
                    &mut created_streams,
                    &DebugStreamStepResults::name().to_string(),
                    &file_path,
                )?;
            }
            if file_name.starts_with(DebugStreamItineraries::name()) {
                Self::create_or_insert(
                    &db_con,
                    &mut created_streams,
                    &DebugStreamItineraries::name().to_string(),
                    &file_path,
                )?;
            }
            if file_name.starts_with(DebugStreamItineraryWaypoints::name()) {
                Self::create_or_insert(
                    &db_con,
                    &mut created_streams,
                    &DebugStreamItineraryWaypoints::name().to_string(),
                    &file_path,
                )?;
            }
            if file_name.starts_with(DebugStreamForkChoices::name()) {
                Self::create_or_insert(
                    &db_con,
                    &mut created_streams,
                    &DebugStreamForkChoices::name().to_string(),
                    &file_path,
                )?;
            }
            if file_name.starts_with(DebugStreamForkChoiceWeights::name()) {
                Self::create_or_insert(
                    &db_con,
                    &mut created_streams,
                    &DebugStreamForkChoiceWeights::name().to_string(),
                    &file_path,
                )?;
            }
        }
        Ok(())
    }

    fn handle_data_request(
        request: &Request,
        db_con: &Connection,
    ) -> Result<Response<Cursor<Vec<u8>>>, DebugViewerError> {
        println!(
            "received request! method: {:?}, url: {:?}",
            request.method(),
            request.url(),
        );

        if request.url() == url_for_debug_stream_name(DebugStreamSteps::name()) {
            let mut statement = db_con
                .prepare(&format!(
                    "SELECT {} FROM {}",
                    DebugStreamSteps::FIELD_NAMES_AS_SLICE.join(","),
                    DebugStreamSteps::name()
                ))
                .map_err(|error| DebugViewerError::DbStatementError { error })?;

            let rows = statement
                .query_map([], |row| {
                    Ok(DebugStreamSteps {
                        itinerary_id: row.get(0)?,
                        step_num: row.get(1)?,
                        move_result: row.get(2)?,
                    })
                })
                .map_err(|error| DebugViewerError::DbStatementError { error })?
                .collect::<Result<Vec<_>>>()
                .map_err(|error| DebugViewerError::DbStatementError { error })?;
            Ok(Response::from_string(
                serde_json::to_string(&rows)
                    .map_err(|error| DebugViewerError::Serialize { error })?,
            )
            .with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .map_err(|_| DebugViewerError::HeaderCreate)?,
            ))
        } else if request.url() == url_for_debug_stream_name(DebugStreamStepResults::name()) {
            let mut statement = db_con
                .prepare(&format!(
                    "SELECT {} FROM {}",
                    DebugStreamStepResults::FIELD_NAMES_AS_SLICE.join(","),
                    DebugStreamStepResults::name()
                ))
                .map_err(|error| DebugViewerError::DbStatementError { error })?;

            let rows = statement
                .query_map([], |row| {
                    Ok(DebugStreamStepResults {
                        itinerary_id: row.get(0)?,
                        step_num: row.get(1)?,
                        result: row.get(2)?,
                        chosen_fork_point_id: row.get(3)?,
                    })
                })
                .map_err(|error| DebugViewerError::DbStatementError { error })?
                .collect::<Result<Vec<_>>>()
                .map_err(|error| DebugViewerError::DbStatementError { error })?;
            Ok(Response::from_string(
                serde_json::to_string(&rows)
                    .map_err(|error| DebugViewerError::Serialize { error })?,
            )
            .with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .map_err(|_| DebugViewerError::HeaderCreate)?,
            ))
        } else if request.url() == url_for_debug_stream_name(DebugStreamForkChoices::name()) {
            let mut statement = db_con
                .prepare(&format!(
                    "SELECT {} FROM {}",
                    DebugStreamForkChoices::FIELD_NAMES_AS_SLICE.join(","),
                    DebugStreamForkChoices::name()
                ))
                .map_err(|error| DebugViewerError::DbStatementError { error })?;

            let rows = statement
                .query_map([], |row| {
                    Ok(DebugStreamForkChoices {
                        itinerary_id: row.get(0)?,
                        step_num: row.get(1)?,
                        end_point_id: row.get(2)?,
                        line_point_0_lat: row.get(3)?,
                        line_point_0_lon: row.get(4)?,
                        line_point_1_lat: row.get(5)?,
                        line_point_1_lon: row.get(6)?,
                        segment_end_point: row.get(7)?,
                        discarded: row.get(8)?,
                    })
                })
                .map_err(|error| DebugViewerError::DbStatementError { error })?
                .collect::<Result<Vec<_>>>()
                .map_err(|error| DebugViewerError::DbStatementError { error })?;
            Ok(Response::from_string(
                serde_json::to_string(&rows)
                    .map_err(|error| DebugViewerError::Serialize { error })?,
            )
            .with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .map_err(|_| DebugViewerError::HeaderCreate)?,
            ))
        } else if request.url() == url_for_debug_stream_name(DebugStreamForkChoiceWeights::name()) {
            let mut statement = db_con
                .prepare(&format!(
                    "SELECT {} FROM {}",
                    DebugStreamForkChoiceWeights::FIELD_NAMES_AS_SLICE.join(","),
                    DebugStreamForkChoiceWeights::name()
                ))
                .map_err(|error| DebugViewerError::DbStatementError { error })?;

            let rows = statement
                .query_map([], |row| {
                    Ok(DebugStreamForkChoiceWeights {
                        itinerary_id: row.get(0)?,
                        step_num: row.get(1)?,
                        end_point_id: row.get(2)?,
                        weight_name: row.get(3)?,
                        weight_type: row.get(4)?,
                        weight_value: row.get(5)?,
                    })
                })
                .map_err(|error| DebugViewerError::DbStatementError { error })?
                .collect::<Result<Vec<_>>>()
                .map_err(|error| DebugViewerError::DbStatementError { error })?;
            Ok(Response::from_string(
                serde_json::to_string(&rows)
                    .map_err(|error| DebugViewerError::Serialize { error })?,
            )
            .with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .map_err(|_| DebugViewerError::HeaderCreate)?,
            ))
        } else if request.url() == url_for_debug_stream_name(DebugStreamItineraries::name()) {
            let mut statement = db_con
                .prepare(&format!(
                    "SELECT {} FROM {}",
                    DebugStreamItineraries::FIELD_NAMES_AS_SLICE.join(","),
                    DebugStreamItineraries::name()
                ))
                .map_err(|error| DebugViewerError::DbStatementError { error })?;

            let rows = statement
                .query_map([], |row| {
                    Ok(DebugStreamItineraries {
                        itinerary_id: row.get(0)?,
                        waypoints_count: row.get(1)?,
                        radius: row.get(2)?,
                        visit_all: row.get(3)?,
                    })
                })
                .map_err(|error| DebugViewerError::DbStatementError { error })?
                .collect::<Result<Vec<_>>>()
                .map_err(|error| DebugViewerError::DbStatementError { error })?;
            Ok(Response::from_string(
                serde_json::to_string(&rows)
                    .map_err(|error| DebugViewerError::Serialize { error })?,
            )
            .with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .map_err(|_| DebugViewerError::HeaderCreate)?,
            ))
        } else if request.url() == url_for_debug_stream_name(DebugStreamItineraryWaypoints::name())
        {
            let mut statement = db_con
                .prepare(&format!(
                    "SELECT {} FROM {}",
                    DebugStreamItineraryWaypoints::FIELD_NAMES_AS_SLICE.join(","),
                    DebugStreamItineraryWaypoints::name()
                ))
                .map_err(|error| DebugViewerError::DbStatementError { error })?;

            let rows = statement
                .query_map([], |row| {
                    Ok(DebugStreamItineraryWaypoints {
                        itinerary_id: row.get(0)?,
                        idx: row.get(1)?,
                        lat: row.get(2)?,
                        lon: row.get(3)?,
                    })
                })
                .map_err(|error| DebugViewerError::DbStatementError { error })?
                .collect::<Result<Vec<_>>>()
                .map_err(|error| DebugViewerError::DbStatementError { error })?;
            Ok(Response::from_string(
                serde_json::to_string(&rows)
                    .map_err(|error| DebugViewerError::Serialize { error })?,
            )
            .with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .map_err(|_| DebugViewerError::HeaderCreate)?,
            ))
        } else {
            Err(DebugViewerError::Unexpected)?
        }
    }

    fn handle_file_request(
        request: &Request,
    ) -> Result<Response<Cursor<Vec<u8>>>, DebugViewerError> {
        println!(
            "received request! method: {:?}, url: {:?}",
            request.method(),
            request.url(),
        );

        Ok(match request.url() {
            "/" => {
                let mut contents = String::new();
                File::open("./src/debug/viewer/ui/viewer.html")
                    .map_err(|error| DebugViewerError::FileOpen { error })?
                    .read_to_string(&mut contents)
                    .map_err(|error| DebugViewerError::FileOpen { error })?;

                Response::from_string(contents).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..])
                        .map_err(|_| DebugViewerError::HeaderCreate)?,
                )
            }
            "/viewer.js" => {
                let mut contents = String::new();
                File::open("./src/debug/viewer/ui/viewer.js")
                    .map_err(|error| DebugViewerError::FileOpen { error })?
                    .read_to_string(&mut contents)
                    .map_err(|error| DebugViewerError::FileOpen { error })?;

                Response::from_string(contents).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"text/javascript"[..])
                        .map_err(|_| DebugViewerError::HeaderCreate)?,
                )
            }
            "/van-1.5.2.debug.js" => {
                let mut contents = String::new();
                File::open("./src/debug/viewer/ui/van-1.5.2.debug.js")
                    .map_err(|error| DebugViewerError::FileOpen { error })?
                    .read_to_string(&mut contents)
                    .map_err(|error| DebugViewerError::FileOpen { error })?;

                Response::from_string(contents).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"text/javascript"[..])
                        .map_err(|_| DebugViewerError::HeaderCreate)?,
                )
            }
            "/van-1.5.2.js" => {
                let mut contents = String::new();
                File::open("./src/debug/viewer/ui/van-1.5.2.js")
                    .map_err(|error| DebugViewerError::FileOpen { error })?
                    .read_to_string(&mut contents)
                    .map_err(|error| DebugViewerError::FileOpen { error })?;

                Response::from_string(contents).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"text/javascript"[..])
                        .map_err(|_| DebugViewerError::HeaderCreate)?,
                )
            }
            _ => Err(DebugViewerError::Unexpected)?,
        })
    }
}
