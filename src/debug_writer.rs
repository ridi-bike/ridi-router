use std::{cell::OnceCell, io, path::PathBuf};
use tracing::error;

use rusqlite::Connection;

use crate::router::itinerary::Itinerary;

thread_local! {
    static DEBUG_WRITER: OnceCell<Option<DebugWriter>> = OnceCell::new();
}

#[derive(Debug, thiserror::Error)]
pub enum DebugWriterError {
    #[error("Could not check if debug log db file exists: {error}")]
    DbFileCheck { error: io::Error },
    #[error("Could not remove existing debug log db file: {error}")]
    DbFileRemove { error: io::Error },
    #[error("Could not open debug log database: {error}")]
    DbOpen { error: rusqlite::Error },
    #[error("Could not setup debug log database schema: {error}")]
    DbSchemaSetup { error: rusqlite::Error },
    #[error("Could not execute sql in database: {error}")]
    DbWrite { error: rusqlite::Error },
    #[error("Could not prepare sql: {error}")]
    DbPrepare { error: rusqlite::Error },
}

pub struct DebugWriter {
    connection: Connection,
}

impl DebugWriter {
    pub fn init(filename: Option<PathBuf>) -> Result<(), DebugWriterError> {
        if let Some(filename) = filename {
            DEBUG_WRITER.with(|oc| {
                if std::fs::exists(&filename)
                    .map_err(|error| DebugWriterError::DbFileCheck { error })?
                {
                    std::fs::remove_file(&filename)
                        .map_err(|error| DebugWriterError::DbFileRemove { error })?;
                }
                let connection = Connection::open(&filename)
                    .map_err(|error| DebugWriterError::DbOpen { error })?;
                connection
                    .execute(
                        "
                        create table itineraries (
                          id text not null primary key,
                          waypoint_count number not null,
                          radius decimal not null,
                          visit_all_wps number not null
                        );
                    ",
                        (),
                    )
                    .map_err(|error| DebugWriterError::DbSchemaSetup { error })?;
                connection
                    .execute(
                        "
                        create table itinerary_waypoints (
                          itinerary_id text not null,
                          seq number not null,
                          lat decimal not null,
                          lon decimal not null
                        );
                    ",
                        (),
                    )
                    .map_err(|error| DebugWriterError::DbSchemaSetup { error })?;

                oc.get_or_init(move || Some(DebugWriter { connection }));

                Ok(())
            })?;
        } else {
            DEBUG_WRITER.with(|oc| {
                oc.get_or_init(|| None);
            });
        }

        Ok(())
    }

    pub fn write_itinerary(itinerary: &Itinerary) -> () {
        let res: Result<(), DebugWriterError> = DEBUG_WRITER.with(|oc| {
            if let Some(debug_writer) = oc.get() {
                if let Some(debug_writer) = debug_writer {
                    let mut statement = debug_writer
                        .connection
                        .prepare(
                            "
                            insert into itineraries 
                            (id, waypoint_count, radius, visit_all_wps)
                            values 
                            (?1, ?2, ?3, ?4)
                            ",
                        )
                        .map_err(|error| DebugWriterError::DbPrepare { error })?;

                    statement
                        .execute([
                            itinerary.id(),
                            itinerary.get_waypoints().len().to_string(),
                            itinerary.waypoint_radius.to_string(),
                            if itinerary.visit_all_waypoints {
                                1.to_string()
                            } else {
                                0.to_string()
                            },
                        ])
                        .map_err(|error| DebugWriterError::DbWrite { error })?;

                    let mut statement = debug_writer
                        .connection
                        .prepare(
                            "
                            insert into itinerary_waypoints 
                            (itinerary_id, seq, lat, lon) 
                            values (?1, ?2, ?3, ?4)
                            ",
                        )
                        .map_err(|error| DebugWriterError::DbPrepare { error })?;

                    itinerary
                        .get_waypoints()
                        .iter()
                        .enumerate()
                        .map(|(idx, wp)| {
                            statement
                                .execute([
                                    itinerary.id(),
                                    idx.to_string(),
                                    wp.borrow().lat.to_string(),
                                    wp.borrow().lon.to_string(),
                                ])
                                .map_err(|error| DebugWriterError::DbWrite { error })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                }
            }
            Ok(())
        });

        if let Err(error) = res {
            error!(error = ?error, "Could not write debug log")
        }
    }
}
