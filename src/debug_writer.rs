use std::{
    collections::HashMap,
    fs::File,
    io,
    path::PathBuf,
    sync::{LazyLock, OnceLock, PoisonError, RwLock, RwLockReadGuard},
    time::Duration,
};
use tracing::error;

use crate::{
    map_data::graph::MapDataPointRef,
    router::{
        itinerary::Itinerary,
        navigator::WeightCalcResult,
        route::{segment::Segment, segment_list::SegmentList},
        walker::{WalkerError, WalkerMoveResult},
    },
};

#[derive(Debug, thiserror::Error)]
pub enum DebugWriterError {
    #[error("Could not check if debug dir exists: {error}")]
    DirCheck { error: io::Error },
    #[error("Could not remove existing debug dir: {error}")]
    DirRemove { error: io::Error },
    #[error("Could not create debug dir: {error}")]
    DirCreate { error: io::Error },
    #[error("Could not read global debug_writer")]
    StaticRead { error: String },
    #[error("Could not get write lock on debug_writer")]
    StaticWrite { error: String },
    #[error("Could not create file")]
    FileCreate {
        file_name: PathBuf,
        error: io::Error,
    },
    #[error("Could not write record")]
    Write { error: csv::Error },
}

static DEBUG_DIR: OnceLock<PathBuf> = OnceLock::new();

thread_local! {
    static DEBUG_WRITER: OnceLock<RwLock<DebugWriter>> = OnceLock::new();
}

pub struct DebugWriter {
    files: HashMap<String, csv::Writer<File>>,
}

impl DebugWriter {
    fn exec<T: Fn(&mut csv::Writer<File>) -> Result<(), DebugWriterError>>(
        file_id: &str,
        item_id: &str,
        header_row: &[&str],
        cb: T,
    ) -> () {
        if let Some(debug_dir) = DEBUG_DIR.get() {
            let res = DEBUG_WRITER.with(|debug_writer| -> Result<(), DebugWriterError> {
                let debug_writer = debug_writer.get_or_init(|| {
                    RwLock::new(DebugWriter {
                        files: HashMap::new(),
                    })
                });
                let mut debug_writer_write =
                    debug_writer
                        .write()
                        .map_err(|error| DebugWriterError::StaticRead {
                            error: error.to_string(),
                        })?;

                if let Some(writer) = debug_writer_write.files.get_mut(file_id) {
                    cb(writer)?;
                } else {
                    let mut file_name = debug_dir.clone();
                    file_name.push(format!("{item_id}.{file_id}"));
                    file_name.set_extension("csv");
                    let file = File::create(&file_name)
                        .map_err(|error| DebugWriterError::FileCreate { file_name, error })?;
                    let mut writer = csv::Writer::from_writer(file);
                    writer
                        .write_record(header_row)
                        .map_err(|error| DebugWriterError::Write { error })?;
                    cb(&mut writer)?;
                    debug_writer_write.files.insert(file_id.to_string(), writer);
                }
                Ok(())
            });
            if let Err(error) = res {
                error!(error = display(error), "Failed to write to log");
            }
        }
    }

    pub fn init(dir_name: Option<PathBuf>) -> Result<(), DebugWriterError> {
        if let Some(dir_name) = dir_name {
            if std::fs::exists(&dir_name).map_err(|error| DebugWriterError::DirCheck { error })? {
                std::fs::remove_dir_all(&dir_name)
                    .map_err(|error| DebugWriterError::DirRemove { error })?;
                std::fs::create_dir_all(&dir_name)
                    .map_err(|error| DebugWriterError::DirRemove { error })?;
            }
            DEBUG_DIR.get_or_init(|| dir_name);
        }

        Ok(())
    }

    pub fn write_step_result(
        itinerary_id: String,
        step: u32,
        result: &str,
        chosen_fork_point_id: Option<u64>,
    ) {
        DebugWriter::exec(
            "steps",
            &itinerary_id,
            &["itinerary_id", "step_num", "result", "chosen_fork_point_id"],
            |writer| {
                writer
                    .write_record([
                        itinerary_id.clone(),
                        step.to_string(),
                        result.to_string(),
                        chosen_fork_point_id.map_or(0, |v| v).to_string(),
                    ])
                    .map_err(|error| DebugWriterError::Write { error })?;
                Ok(())
            },
        );
    }
    pub fn write_fork_choice_weight(
        itinerary_id: String,
        step: u32,
        end_point_id: &u64,
        weight_name: &String,
        weight_result: &WeightCalcResult,
    ) {
        let (weight_type, weight_value) = match weight_result {
            WeightCalcResult::DoNotUse => ("DoNotUse", &0),
            WeightCalcResult::UseWithWeight(v) => ("UseWithWeight", v),
        };
        DebugWriter::exec(
            "steps",
            &itinerary_id,
            &[
                "itinerary_id",
                "step_num",
                "end_point_id",
                "weight_name",
                "weight_type",
                "weight_value",
            ],
            |writer| {
                writer
                    .write_record([
                        itinerary_id.clone(),
                        step.to_string(),
                        end_point_id.to_string(),
                        weight_name.to_string(),
                        weight_type.to_string(),
                        weight_value.to_string(),
                    ])
                    .map_err(|error| DebugWriterError::Write { error })?;
                Ok(())
            },
        );
    }

    pub fn write_fork_choices(
        itinerary_id: String,
        step: u32,
        segment_list: &SegmentList,
        discarded_choices: &Vec<MapDataPointRef>,
    ) {
        for segment in segment_list.clone().into_iter() {
            DebugWriter::exec(
                "steps",
                &itinerary_id,
                &[
                    "itinerary_id",
                    "step_num",
                    "end_point_id",
                    "line_point_0_lat",
                    "line_point_0_lon",
                    "line_point_1_lat",
                    "line_point_1_lon",
                    "segment_end_point",
                    "discarded",
                ],
                |writer| {
                    writer
                        .write_record([
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
                            if segment.get_end_point() == &segment.get_line().borrow().points.0 {
                                0
                            } else {
                                1
                            }
                            .to_string(),
                            discarded_choices
                                .iter()
                                .find(|c| c == &segment.get_end_point())
                                .is_some()
                                .to_string(),
                        ])
                        .map_err(|error| DebugWriterError::Write { error })?;
                    Ok(())
                },
            );
        }
    }

    pub fn write_step(
        itinerary_id: String,
        step: u32,
        move_result: &Result<WalkerMoveResult, WalkerError>,
    ) {
        let move_result = match move_result {
            Err(_) => "Error",
            Ok(WalkerMoveResult::Finish) => "Finish",
            Ok(WalkerMoveResult::DeadEnd) => "Dead End",
            Ok(WalkerMoveResult::Fork(_)) => "Fork",
        };
        DebugWriter::exec(
            "steps",
            &itinerary_id,
            &["itinerary_id", "step_num", "move_result"],
            |writer| {
                writer
                    .write_record([
                        itinerary_id.clone(),
                        step.to_string(),
                        move_result.to_string(),
                    ])
                    .map_err(|error| DebugWriterError::Write { error })?;
                Ok(())
            },
        );
    }

    pub fn write_itineraries(itineraries: &Vec<Itinerary>) -> () {
        for itinerary in itineraries {
            DebugWriter::exec(
                "steps",
                &itinerary.id(),
                &["itinerary_id", "waypoints_count", "radius", "visit_all"],
                |writer| {
                    writer
                        .write_record([
                            itinerary.id(),
                            itinerary.waypoints.len().to_string(),
                            itinerary.waypoint_radius.to_string(),
                            itinerary.visit_all_waypoints.to_string(),
                        ])
                        .map_err(|error| DebugWriterError::Write { error })?;
                    Ok(())
                },
            );
            for (idx, wp) in itinerary.waypoints.iter().enumerate() {
                DebugWriter::exec(
                    "steps",
                    &itinerary.id(),
                    &["itinerary_id", "step_num", "move_result"],
                    |writer| {
                        writer
                            .write_record([
                                itinerary.id(),
                                idx.to_string(),
                                wp.borrow().lat.to_string(),
                                wp.borrow().lon.to_string(),
                            ])
                            .map_err(|error| DebugWriterError::Write { error })?;
                        Ok(())
                    },
                );
            }
        }
    }
}
