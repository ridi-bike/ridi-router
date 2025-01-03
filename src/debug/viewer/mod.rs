use derive_name::Name;
use duckdb::{params, Connection, Result, Row};
use qstring::QString;
use serde::Serialize;
use sql_builder::{bind::Bind, SqlBuilder};
use std::{
    error::Error,
    ffi::OsString,
    fs::{self, File},
    io::{self, Cursor},
    num::ParseIntError,
    path::PathBuf,
};
use struct_field_names_as_array::FieldNamesAsSlice;
use tiny_http::{Header, Method, Request, Response, Server};
use tracing::info;

use crate::debug::writer::{
    DebugStreamForkChoiceWeights, DebugStreamForkChoices, DebugStreamItineraries,
    DebugStreamItineraryWaypoints, DebugStreamStepResults, DebugStreamSteps,
};

const DATA_PREFIX: &str = "/data/";

fn url_for_debug_stream_name(name: &str) -> String {
    format!("{DATA_PREFIX}{name}")
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

    #[error("Could not execute db statement {error}")]
    DbStatementError { error: duckdb::Error },

    #[error("Could not serialize {error}")]
    Serialize { error: serde_json::Error },

    #[error("Could not build query: {error}")]
    SqlBuilder { error: anyhow::Error },

    #[error("Could not parse number: {error}")]
    Parse { error: ParseIntError },

    #[error("Missing query parameter: {param_name}")]
    MissingQueryParam { param_name: &'static str },

    #[error("Serde deserialize error on route chunks: {error}")]
    SerdeDesRouteChunks { error: serde_json::Error },
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

            if request.url().starts_with(DATA_PREFIX) {
                let response = match DebugViewer::handle_data_request(&request, &db_conn) {
                    Err(e) => {
                        request
                            .respond(Response::from_string(format!("{e:?}")).with_status_code(500))
                            .map_err(|error| DebugViewerError::Respond { error })?;
                        continue;
                    }
                    Ok(resp) => resp,
                };
                request
                    .respond(response)
                    .map_err(|error| DebugViewerError::Respond { error })?;
                continue;
            }

            if request.url().starts_with("/calc/route") {
                let response = match Self::handle_calc_route(&request, &db_conn) {
                    Err(e) => {
                        request
                            .respond(Response::from_string(format!("{e:?}")).with_status_code(500))
                            .map_err(|error| DebugViewerError::Respond { error })?;
                        continue;
                    }
                    Ok(r) => r,
                };
                request
                    .respond(response)
                    .map_err(|error| DebugViewerError::Respond { error })?;
                continue;
            }

            let response = match DebugViewer::handle_file_request(&request) {
                Err(e) => {
                    request
                        .respond(Response::from_string(format!("{e:?}")).with_status_code(500))
                        .map_err(|error| DebugViewerError::Respond { error })?;
                    continue;
                }
                Ok(resp) => resp,
            };
            request
                .respond(response)
                .map_err(|error| DebugViewerError::Respond { error })?;
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

    fn handle_data_for_table<F, T>(
        db_con: &Connection,
        table_name: &str,
        field_names: &[&str],
        query_itinerary_id: Option<String>,
        query_limit: Option<u16>,
        query_offset: Option<u16>,
        query_step_num: Option<u32>,
        map_row: F,
    ) -> Result<Response<Cursor<Vec<u8>>>, DebugViewerError>
    where
        F: FnMut(&Row<'_>) -> Result<T>,
        T: Serialize,
    {
        let mut sql = SqlBuilder::select_from(table_name);
        let sql = sql.fields(field_names);
        let sql = if let Some(it_id) = query_itinerary_id {
            sql.and_where("itinerary_id = ?".binds(&[&it_id]))
        } else {
            sql
        };
        let sql = if let Some(step_num) = query_step_num {
            sql.and_where("step_num = ?".binds(&[&step_num]))
        } else {
            sql
        };
        let sql = if let Some(limit) = query_limit {
            sql.limit(limit)
        } else {
            sql
        };
        let sql = if let Some(offset) = query_offset {
            sql.offset(offset)
        } else {
            sql
        };
        let sql = sql.order_by("itinerary_id", false);
        let sql = if table_name == "DebugStreamForkChoiceWeights" {
            sql.order_by("weight_name", true)
        } else {
            sql
        };
        let sql = if table_name == "DebugStreamForkChoices" {
            sql.order_by("discarded", true)
        } else {
            sql
        };
        let sql = if table_name != "DebugStreamItineraries"
            && table_name != "DebugStreamItineraryWaypoints"
        {
            sql.order_by("step_num", false)
        } else {
            sql
        };
        let sql = sql
            .sql()
            .map_err(|error| DebugViewerError::SqlBuilder { error })?;

        eprintln!("{}", sql);
        let mut statement = db_con
            .prepare(&sql)
            .map_err(|error| DebugViewerError::DbStatementError { error })?;

        let rows = statement
            .query_map([], map_row)
            .map_err(|error| DebugViewerError::DbStatementError { error })?
            .collect::<Result<Vec<_>>>()
            .map_err(|error| DebugViewerError::DbStatementError { error })?;

        Ok(Response::from_string(
            serde_json::to_string(&rows).map_err(|error| DebugViewerError::Serialize { error })?,
        )
        .with_header(
            Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                .map_err(|_| DebugViewerError::HeaderCreate)?,
        ))
    }

    fn handle_calc_route(
        request: &Request,
        db_con: &Connection,
    ) -> Result<Response<Cursor<Vec<u8>>>, DebugViewerError> {
        println!(
            "received CALC request! method: {:?}, url: {:?}",
            request.method(),
            request.url(),
        );
        let query = request.url().split("?").collect::<Vec<_>>();
        let query = query
            .get(1)
            .map_or_else(|| "?".to_string(), |v| format!("?{}", *v));
        let query = QString::from(query.as_str());
        let query_itinerary_id = query.get("itinerary_id").map(|v| v.to_string()).map_or(
            Err(DebugViewerError::MissingQueryParam {
                param_name: "itinerary_id",
            }),
            |v| Ok(v),
        )?;
        let query_step = query.get("step").map_or(
            Err(DebugViewerError::MissingQueryParam { param_name: "step" }),
            |v| -> Result<u32, DebugViewerError> {
                v.parse().map_err(|error| DebugViewerError::Parse { error })
            },
        )?;

        let mut statement = db_con
            .prepare(
                "select route from DebugStreamSteps
                    where itinerary_id = ? and step_num <= ?",
            )
            .map_err(|error| DebugViewerError::DbStatementError { error })?;

        let rows: Vec<String> = statement
            .query_map(params![query_itinerary_id, query_step], |row| {
                Ok(String::from(row.get::<usize, String>(0)?))
            })
            .map_err(|error| DebugViewerError::DbStatementError { error })?
            .collect::<Result<Vec<_>>>()
            .map_err(|error| DebugViewerError::DbStatementError { error })?;

        let rows = rows
            .iter()
            .map(|row| serde_json::from_str::<Vec<(f64, f64)>>(row))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DebugViewerError::SerdeDesRouteChunks { error })?;

        Ok(Response::from_string(
            serde_json::to_string(&rows).map_err(|error| DebugViewerError::Serialize { error })?,
        ))
    }

    fn handle_data_request(
        request: &Request,
        db_con: &Connection,
    ) -> Result<Response<Cursor<Vec<u8>>>, DebugViewerError> {
        println!(
            "received DATA request! method: {:?}, url: {:?}",
            request.method(),
            request.url(),
        );
        let query = request.url().split("?").collect::<Vec<_>>();
        let query = query
            .get(1)
            .map_or_else(|| "?".to_string(), |v| format!("?{}", *v));
        let query = QString::from(query.as_str());
        let query_itinerary_id = query.get("itinerary_id").map(|v| v.to_string());
        let query_step_num = query
            .get("step_num")
            .map(|v| -> Result<u32, DebugViewerError> {
                v.parse().map_err(|error| DebugViewerError::Parse { error })
            });
        let query_step_num = if let Some(step_num) = query_step_num {
            Some(step_num?)
        } else {
            None
        };
        let query_limit = query
            .get("limit")
            .map(|v| -> Result<u16, DebugViewerError> {
                v.parse().map_err(|error| DebugViewerError::Parse { error })
            });
        let query_limit = if let Some(limit) = query_limit {
            Some(limit?)
        } else {
            None
        };
        let query_offset = query
            .get("offset")
            .map(|v| -> Result<u16, DebugViewerError> {
                v.parse().map_err(|error| DebugViewerError::Parse { error })
            });
        let query_offset = if let Some(offset) = query_offset {
            Some(offset?)
        } else {
            None
        };

        if request
            .url()
            .starts_with(&url_for_debug_stream_name(DebugStreamSteps::name()))
        {
            Ok(Self::handle_data_for_table(
                &db_con,
                DebugStreamSteps::name(),
                DebugStreamSteps::FIELD_NAMES_AS_SLICE,
                query_itinerary_id,
                query_limit,
                query_offset,
                query_step_num,
                |row| {
                    Ok(DebugStreamSteps {
                        itinerary_id: row.get(0)?,
                        step_num: row.get(1)?,
                        move_result: row.get(2)?,
                        route: row.get(3)?,
                    })
                },
            )?)
        } else if request
            .url()
            .starts_with(&url_for_debug_stream_name(DebugStreamStepResults::name()))
        {
            Ok(Self::handle_data_for_table(
                &db_con,
                DebugStreamStepResults::name(),
                DebugStreamStepResults::FIELD_NAMES_AS_SLICE,
                query_itinerary_id,
                query_limit,
                query_offset,
                query_step_num,
                |row| {
                    Ok(DebugStreamStepResults {
                        itinerary_id: row.get(0)?,
                        step_num: row.get(1)?,
                        result: row.get(2)?,
                        chosen_fork_point_id: row.get(3)?,
                    })
                },
            )?)
        } else if request
            .url()
            .starts_with(&url_for_debug_stream_name(DebugStreamForkChoices::name()))
        {
            Ok(Self::handle_data_for_table(
                &db_con,
                DebugStreamForkChoices::name(),
                DebugStreamForkChoices::FIELD_NAMES_AS_SLICE,
                query_itinerary_id,
                query_limit,
                query_offset,
                query_step_num,
                |row| {
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
                },
            )?)
        } else if request.url().starts_with(&url_for_debug_stream_name(
            DebugStreamForkChoiceWeights::name(),
        )) {
            Ok(Self::handle_data_for_table(
                &db_con,
                DebugStreamForkChoiceWeights::name(),
                DebugStreamForkChoiceWeights::FIELD_NAMES_AS_SLICE,
                query_itinerary_id,
                query_limit,
                query_offset,
                query_step_num,
                |row| {
                    Ok(DebugStreamForkChoiceWeights {
                        itinerary_id: row.get(0)?,
                        step_num: row.get(1)?,
                        end_point_id: row.get(2)?,
                        weight_name: row.get(3)?,
                        weight_type: row.get(4)?,
                        weight_value: row.get(5)?,
                    })
                },
            )?)
        } else if request
            .url()
            .starts_with(&url_for_debug_stream_name(DebugStreamItineraries::name()))
        {
            Ok(Self::handle_data_for_table(
                &db_con,
                DebugStreamItineraries::name(),
                DebugStreamItineraries::FIELD_NAMES_AS_SLICE,
                query_itinerary_id,
                query_limit,
                query_offset,
                query_step_num,
                |row| {
                    Ok(DebugStreamItineraries {
                        itinerary_id: row.get(0)?,
                        waypoints_count: row.get(1)?,
                        radius: row.get(2)?,
                        visit_all: row.get(3)?,
                        start_lat: row.get(4)?,
                        start_lon: row.get(5)?,
                        finish_lat: row.get(6)?,
                        finish_lon: row.get(7)?,
                    })
                },
            )?)
        } else if request.url().starts_with(&url_for_debug_stream_name(
            DebugStreamItineraryWaypoints::name(),
        )) {
            Ok(Self::handle_data_for_table(
                &db_con,
                DebugStreamItineraryWaypoints::name(),
                DebugStreamItineraryWaypoints::FIELD_NAMES_AS_SLICE,
                query_itinerary_id,
                query_limit,
                query_offset,
                query_step_num,
                |row| {
                    Ok(DebugStreamItineraryWaypoints {
                        itinerary_id: row.get(0)?,
                        idx: row.get(1)?,
                        lat: row.get(2)?,
                        lon: row.get(3)?,
                    })
                },
            )?)
        } else {
            Err(DebugViewerError::Unexpected)?
        }
    }

    fn handle_file_request(request: &Request) -> Result<Response<File>, DebugViewerError> {
        println!(
            "received FILE request! method: {:?}, url: {:?}",
            request.method(),
            request.url(),
        );

        let mut file_name = request.url().to_string();
        loop {
            let file_name_len = file_name.len();
            file_name = file_name.replace("../", "");
            file_name = file_name.replace("./", "");
            if file_name.len() == file_name_len {
                break;
            }
        }
        let file_name = if file_name == "/" {
            "/index.html".to_string()
        } else {
            file_name
        };
        let file_name = format!("./src/debug/viewer/ui/dist{file_name}");

        let file = File::open(&file_name).map_err(|error| DebugViewerError::FileOpen { error })?;

        let mime_type = match PathBuf::from(file_name).extension() {
            None => "",
            Some(extension) => match extension.to_str() {
                Some("js") => "text/javascript",
                Some("css") => "text/css",
                Some("html") => "text/html",
                Some("svg") => "image/svg+xml",
                _ => "",
            },
        };

        Ok(Response::from_file(file).with_header(
            Header::from_bytes(&b"Content-Type"[..], &mime_type.as_bytes()[..])
                .map_err(|_| DebugViewerError::HeaderCreate)?,
        ))
    }
}
