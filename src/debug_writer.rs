use std::{cell::OnceCell, io, path::PathBuf, sync::OnceLock};
use tracing::error;

use rusqlite::Connection;

use crate::{
    map_data::graph::MapDataPointRef,
    router::{
        itinerary::Itinerary,
        navigator::WeightCalcResult,
        route::{segment::Segment, segment_list::SegmentList},
        walker::{WalkerError, WalkerMoveResult},
    },
};

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
    #[error("Could not setup execute PRAGMA: {error:?}")]
    DbPragma { error: rusqlite::Error },
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
            if std::fs::exists(&filename)
                .map_err(|error| DebugWriterError::DbFileCheck { error })?
            {
                std::fs::remove_file(&filename)
                    .map_err(|error| DebugWriterError::DbFileRemove { error })?;
            }
            DEBUG_WRITER.with(|oc| {
                let connection = Connection::open(&filename)
                    .map_err(|error| DebugWriterError::DbOpen { error })?;

                connection
                    .pragma_update(None, "journal_mode", "WAL")
                    .map_err(|error| DebugWriterError::DbPragma { error })?;

                connection
                    .execute(
                        "
                        create table if not exists itineraries (
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
                        create table if not exists waypoints (
                            itinerary_id text not null,
                            seq number not null,
                            lat decimal not null,
                            lon decimal not null
                        );
                        ",
                        (),
                    )
                    .map_err(|error| DebugWriterError::DbSchemaSetup { error })?;

                connection
                    .execute(
                        "
                        create table if not exists steps (
                            itinerary_id text not null,
                            num number not null,
                            move_result text not null
                        );
                        ",
                        (),
                    )
                    .map_err(|error| DebugWriterError::DbSchemaSetup { error })?;

                connection
                    .execute(
                        "
                        create table if not exists fork_choices (
                            itinerary_id text not null,
                            step_num number not null,
                            end_point_id number not null,
                            line_point_0_lat decimal not null,
                            line_point_0_lon decimal not null,
                            line_point_1_lat decimal not null,
                            line_point_1_lon decimal not null,
                            segment_end_point number not null,
                            discarded number not null
                        );
                        ",
                        (),
                    )
                    .map_err(|error| DebugWriterError::DbSchemaSetup { error })?;

                connection
                    .execute(
                        "
                        create table if not exists fork_choice_weights (
                            itinerary_id text not null,
                            step_num number not null,
                            end_point_id number not null,
                            weight_name text not null,
                            weight_type text not null,
                            weight_value number not null
                        );
                        ",
                        (),
                    )
                    .map_err(|error| DebugWriterError::DbSchemaSetup { error })?;

                connection
                    .execute(
                        "
                        create table if not exists step_result (
                            itinerary_id text not null,
                            step_num number not null,
                            result text not null,
                            chosen_fork_point_id number not null
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

    pub fn write_step_result(
        itinerary_id: String,
        step: u32,
        result: &str,
        chosen_fork_point_id: Option<u64>,
    ) {
        let res: Result<(), DebugWriterError> = DEBUG_WRITER.with(|oc| {
            if let Some(debug_writer) = oc.get() {
                if let Some(debug_writer) = debug_writer {
                    let mut statement = debug_writer
                        .connection
                        .prepare(
                            "
                            insert into step_result 
                            (
                                itinerary_id,
                                step_num,
                                result,
                                chosen_fork_point_id
                            )
                            values 
                            (?1, ?2, ?3, ?4)
                            ",
                        )
                        .map_err(|error| DebugWriterError::DbPrepare { error })?;

                    statement
                        .execute([
                            itinerary_id.clone(),
                            step.to_string(),
                            result.to_string(),
                            chosen_fork_point_id.map_or(0, |v| v).to_string(),
                        ])
                        .map_err(|error| DebugWriterError::DbWrite { error })?;
                }
            }
            Ok(())
        });

        if let Err(error) = res {
            error!(error = ?error, "Could not write debug log")
        }
    }
    pub fn write_fork_choice_weight(
        itinerary_id: String,
        step: u32,
        end_point_id: &u64,
        weight_name: &String,
        weight_result: &WeightCalcResult,
    ) {
        let res: Result<(), DebugWriterError> = DEBUG_WRITER.with(|oc| {
            if let Some(debug_writer) = oc.get() {
                if let Some(debug_writer) = debug_writer {
                    let mut statement = debug_writer
                        .connection
                        .prepare(
                            "
                            insert into fork_choices 
                            (
                                itinerary_id,
                                step_num,
                                end_point_id,
                                weight_name,
                                weight_type,
                                weight_value,
                            )
                            values 
                            (?1, ?2, ?3, ?4, ?5, ?6)
                            ",
                        )
                        .map_err(|error| DebugWriterError::DbPrepare { error })?;

                    let (weight_type, weight_value) = match weight_result {
                        WeightCalcResult::DoNotUse => ("DoNotUse", &0),
                        WeightCalcResult::UseWithWeight(v) => ("UseWithWeight", v),
                    };
                    statement
                        .execute([
                            itinerary_id.clone(),
                            step.to_string(),
                            end_point_id.to_string(),
                            weight_name.to_string(),
                            weight_type.to_string(),
                            weight_value.to_string(),
                        ])
                        .map_err(|error| DebugWriterError::DbWrite { error })?;
                }
            }
            Ok(())
        });

        if let Err(error) = res {
            error!(error = ?error, "Could not write debug log")
        }
    }

    pub fn write_fork_choices(
        itinerary_id: String,
        step: u32,
        segment_list: &SegmentList,
        discarded_choices: &Vec<MapDataPointRef>,
    ) {
        let res: Result<(), DebugWriterError> = DEBUG_WRITER.with(|oc| {
            if let Some(debug_writer) = oc.get() {
                if let Some(debug_writer) = debug_writer {
                    let mut statement = debug_writer
                        .connection
                        .prepare(
                            "
                            insert into fork_choices 
                            (
                                itinerary_id,
                                step_num,
                                end_point_id,
                                line_point_0_lat,
                                line_point_0_lon,
                                line_point_1_lat,
                                line_point_1_lon,
                                segment_end_point,
                                discarded
                            )
                            values 
                            (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                            ",
                        )
                        .map_err(|error| DebugWriterError::DbPrepare { error })?;

                    for segment in segment_list.clone().into_iter() {
                        statement
                            .execute([
                                itinerary_id.clone(),
                                step.to_string(),
                                segment.get_end_point().borrow().id.to_string(),
                                segment
                                    .get_line()
                                    .borrow()
                                    .points
                                    .0
                                    .borrow()
                                    .lat
                                    .to_string(),
                                segment
                                    .get_line()
                                    .borrow()
                                    .points
                                    .0
                                    .borrow()
                                    .lon
                                    .to_string(),
                                segment
                                    .get_line()
                                    .borrow()
                                    .points
                                    .1
                                    .borrow()
                                    .lat
                                    .to_string(),
                                segment
                                    .get_line()
                                    .borrow()
                                    .points
                                    .1
                                    .borrow()
                                    .lon
                                    .to_string(),
                                if segment.get_end_point() == &segment.get_line().borrow().points.0
                                {
                                    0
                                } else {
                                    1
                                }
                                .to_string(),
                                if discarded_choices
                                    .iter()
                                    .find(|c| c == &segment.get_end_point())
                                    .is_some()
                                {
                                    1
                                } else {
                                    0
                                }
                                .to_string(),
                            ])
                            .map_err(|error| DebugWriterError::DbWrite { error })?;
                    }
                }
            }
            Ok(())
        });

        if let Err(error) = res {
            error!(error = ?error, "Could not write debug log")
        }
    }

    pub fn write_step(
        itinerary_id: String,
        step: u32,
        move_result: &Result<WalkerMoveResult, WalkerError>,
    ) {
        let res: Result<(), DebugWriterError> = DEBUG_WRITER.with(|oc| {
            if let Some(debug_writer) = oc.get() {
                if let Some(debug_writer) = debug_writer {
                    let mut statement = debug_writer
                        .connection
                        .prepare(
                            "
                            insert into steps 
                            (itinerary_id, num, move_result)
                            values 
                            (?1, ?2, ?3)
                            ",
                        )
                        .map_err(|error| DebugWriterError::DbPrepare { error })?;

                    let move_result = match move_result {
                        Err(_) => "Error",
                        Ok(WalkerMoveResult::Finish) => "Finish",
                        Ok(WalkerMoveResult::DeadEnd) => "Dead End",
                        Ok(WalkerMoveResult::Fork(_)) => "Fork",
                    };
                    statement
                        .execute([itinerary_id, step.to_string(), move_result.to_string()])
                        .map_err(|error| DebugWriterError::DbWrite { error })?;
                }
            }
            Ok(())
        });

        if let Err(error) = res {
            error!(error = ?error, "Could not write debug log")
        }
    }

    pub fn write_itineraries(itineraries: &Vec<Itinerary>) -> () {
        let res: Result<(), DebugWriterError> = DEBUG_WRITER.with(|oc| {
            if let Some(debug_writer) = oc.get() {
                if let Some(debug_writer) = debug_writer {
                    for itinerary in itineraries {
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
                                itinerary.waypoints.len().to_string(),
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
                                insert into waypoints 
                                (itinerary_id, seq, lat, lon) 
                                values 
                                (?1, ?2, ?3, ?4)
                                ",
                            )
                            .map_err(|error| DebugWriterError::DbPrepare { error })?;

                        itinerary
                            .waypoints
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
            }
            Ok(())
        });

        if let Err(error) = res {
            error!(error = ?error, "Could not write debug log")
        }
    }
}
