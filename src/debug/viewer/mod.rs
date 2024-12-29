use duckdb::{arrow::array::create_array, params, Connection, Result};
use std::{
    error::Error,
    fs::{self, File},
    io::{self, Cursor, Read},
    path::PathBuf,
};
use tiny_http::{Header, Method, Request, Response, Server};
use tracing::info;

use crate::debug::writer::DEBUG_STREAMS;

const FILES_URLS: [&str; 4] = ["/", "/viewer.js", "/van-1.5.2.js", "/van-1.5.2.debug.js"];

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
    CantReadFileName,

    #[error("Unexpected file found {file_name}")]
    UnexpectedFile { file_name: String },

    #[error("Could not execute db statement {error}")]
    DbStatementError { error: duckdb::Error },
}
pub struct DebugViewer;

impl DebugViewer {
    pub fn run(debug_dir: PathBuf) -> Result<(), DebugViewerError> {
        let db_conn =
            Connection::open_in_memory().map_err(|error| DebugViewerError::DbOpen { error })?;

        let addr = "0.0.0.0:1337";
        let server = Server::http(addr).map_err(|error| DebugViewerError::ServerStart { error })?;
        info!(addr, "Running Debug Viewer on http://{addr}");

        for request in server.incoming_requests() {
            if request.method() != &Method::Get {
                request.respond(Response::from_string("not allowed").with_status_code(405));
            }

            if FILES_URLS.contains(&request.url()) {
                let response = DebugViewer::handle_file_request(&request)?;
                request
                    .respond(response)
                    .map_err(|error| DebugViewerError::Respond { error })?;
            }

            if DEBUG_STREAMS
                .iter()
                .any(|f| request.url().starts_with(&format!("/data/{}", f.0)))
            {
                let response = DebugViewer::handle_data_request(&request, &db_conn)?;
                request
                    .respond(response)
                    .map_err(|error| DebugViewerError::Respond { error })?;
            }
        }

        Ok(())
    }

    fn prep_data(debug_dir: PathBuf, db_con: &Connection) -> Result<(), DebugViewerError> {
        let dir_contents =
            fs::read_dir(debug_dir).map_err(|error| DebugViewerError::ReadDebugDir { error })?;
        let mut created_streams = Vec::new();
        for debug_file in dir_contents {
            let debug_file =
                debug_file.map_err(|error| DebugViewerError::ReadDebugFileInList { error })?;
            let file_name = debug_file
                .file_name()
                .to_str()
                .ok_or(DebugViewerError::CantReadFileName)?;
            let file_path = debug_file
                .path()
                .to_str()
                .ok_or(DebugViewerError::CantReadFileName)?;
            let debug_stream = DEBUG_STREAMS
                .iter()
                .find(|s| file_name.starts_with(s.0))
                .ok_or(DebugViewerError::UnexpectedFile {
                    file_name: file_name.to_string(),
                })?;
            if !created_streams.contains(debug_stream.0) {
                db_con
                    .execute(
                        &format!(
                            "
                            CREATE TABLE {} AS
                                SELECT * FROM '{}';
                            ",
                            debug_stream.0, file_path
                        ),
                        [],
                    )
                    .map_err(|error| DebugViewerError::DbStatementError { error })?;
                created_streams.push(debug_stream.0);
            } else {
                db_con
                    .execute(
                        &format!(
                            "
                            COPY {} FROM '{}';
                            ",
                            debug_stream.0, file_path
                        ),
                        [],
                    )
                    .map_err(|error| DebugViewerError::DbStatementError { error })?;
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

        let debug_stream = DEBUG_STREAMS
            .iter()
            .find(|s| request.url().starts_with(&format!("/data/{}", s.0)))
            .ok_or(DebugViewerError::UnexpectedFile {
                file_name: request.url().to_string(),
            })?;
        let mut statement = db_con
            .prepare(&format!("SELECT * FROM {}", debug_stream.0))
            .map_err(|error| DebugViewerError::DbStatementError { error })?;

        let rows = statement
            .query_map([], |row| {
                Ok(debug_stream
                    .1
                    .iter()
                    .enumerate()
                    .map(|(idx, field)| row.get(idx)?))
            })
            .map_err(|error| DebugViewerError::DbStatementError { error })?;

        let rows = rows
            .into_iter()
            .map(|row| row.map_err(|error| DebugViewerError::DbStatementError { error }))
            .collect::<Result<Vec<_>, DebugViewerError>>()?;

        Ok(Response::from_data(rows))
    }

    fn handle_file_request(
        request: &Request,
    ) -> Result<Response<Cursor<Vec<u8>>>, DebugViewerError> {
        println!(
            "received request! method: {:?}, url: {:?}",
            request.method(),
            request.url(),
        );

        // Ok(match request.url() {
        //     "/" => Response::from_string(HTML).with_header(
        //         Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..])
        //             .map_err(|_| DebugViewerError::HeaderCreate)?,
        //     ),
        //     "/viewer.js" => Response::from_string(JS).with_header(
        //         Header::from_bytes(&b"Content-Type"[..], &b"text/javascript"[..])
        //             .map_err(|_| DebugViewerError::HeaderCreate)?,
        //     ),
        //     _ => Response::from_string("not found").with_status_code(404),
        // })
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
