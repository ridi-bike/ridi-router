use derive_name::Name;
use serde::Serialize;
use std::{
    collections::HashMap,
    fs::File,
    io,
    path::PathBuf,
    sync::{OnceLock, RwLock},
};
use tracing::error;

use crate::{
    map_data::graph::MapDataPointRef,
    router::{
        itinerary::Itinerary,
        navigator::WeightCalcResult,
        route::segment_list::SegmentList,
        walker::{WalkerError, WalkerMoveResult},
    },
};

#[derive(Serialize, derive_name::Name, struct_field_names_as_array::FieldNamesAsSlice)]
pub struct DebugStreamStepResults {
    pub itinerary_id: String,
    pub step_num: String,
    pub result: String,
    pub chosen_fork_point_id: String,
}

#[derive(Serialize, derive_name::Name, struct_field_names_as_array::FieldNamesAsSlice)]
pub struct DebugStreamForkChoiceWeights {
    pub itinerary_id: String,
    pub step_num: String,
    pub end_point_id: String,
    pub weight_name: String,
    pub weight_type: String,
    pub weight_value: String,
}

#[derive(Serialize, derive_name::Name, struct_field_names_as_array::FieldNamesAsSlice)]
pub struct DebugStreamForkCHoices {
    pub itinerary_id: String,
    pub step_num: String,
    pub end_point_id: String,
    pub line_point_0_lat: String,
    pub line_point_0_lon: String,
    pub line_point_1_lat: String,
    pub line_point_1_lon: String,
    pub segment_end_point: String,
    pub discarded: String,
}

#[derive(Serialize, derive_name::Name, struct_field_names_as_array::FieldNamesAsSlice)]
pub struct DebugStreamSteps {
    pub itinerary_id: String,
    pub step_num: String,
    pub move_result: String,
}

#[derive(Serialize, derive_name::Name, struct_field_names_as_array::FieldNamesAsSlice)]
pub struct DebugStreamItineraries {
    pub itinerary_id: String,
    pub waypoints_count: String,
    pub radius: String,
    pub visit_all: String,
}

#[derive(Serialize, derive_name::Name, struct_field_names_as_array::FieldNamesAsSlice)]
pub struct DebugStreamItineraryWaypoints {
    pub itinerary_id: String,
    pub idx: String,
    pub lat: String,
    pub lon: String,
}

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
    #[error("Could not flush file")]
    Flush { error: io::Error },
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
        file_type_id: &str,
        cb: T,
    ) -> () {
        if let Some(debug_dir) = DEBUG_DIR.get() {
            let file_id = format!("{file_type_id}-{:?}", std::thread::current().id());
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

                if let Some(writer) = debug_writer_write.files.get_mut(&file_id) {
                    cb(writer)?;
                    writer
                        .flush()
                        .map_err(|error| DebugWriterError::Flush { error })?;
                } else {
                    let mut file_name = debug_dir.clone();
                    file_name.push(&file_id);
                    file_name.set_extension("csv");
                    let file = File::create(&file_name)
                        .map_err(|error| DebugWriterError::FileCreate { file_name, error })?;
                    let mut writer = csv::Writer::from_writer(file);
                    cb(&mut writer)?;
                    writer
                        .flush()
                        .map_err(|error| DebugWriterError::Flush { error })?;
                    debug_writer_write.files.insert(file_id, writer);
                }
                Ok(())
            });
            if let Err(error) = res {
                error!(error = debug(error), "Failed to write to log");
            }
        }
    }

    pub fn init(dir_name: Option<PathBuf>) -> Result<(), DebugWriterError> {
        if let Some(dir_name) = dir_name {
            if std::fs::exists(&dir_name).map_err(|error| DebugWriterError::DirCheck { error })? {
                std::fs::remove_dir_all(&dir_name)
                    .map_err(|error| DebugWriterError::DirRemove { error })?;
            }
            std::fs::create_dir_all(&dir_name)
                .map_err(|error| DebugWriterError::DirRemove { error })?;
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
        DebugWriter::exec(DebugStreamStepResults::name(), |writer| {
            writer
                .serialize(DebugStreamStepResults {
                    itinerary_id: itinerary_id.clone(),
                    step_num: step.to_string(),
                    result: result.to_string(),
                    chosen_fork_point_id: chosen_fork_point_id.map_or(0, |v| v).to_string(),
                })
                .map_err(|error| DebugWriterError::Write { error })?;
            Ok(())
        });
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
        DebugWriter::exec(DebugStreamForkChoiceWeights::name(), |writer| {
            writer
                .serialize(DebugStreamForkChoiceWeights {
                    itinerary_id: itinerary_id.clone(),
                    step_num: step.to_string(),
                    end_point_id: end_point_id.to_string(),
                    weight_name: weight_name.to_string(),
                    weight_type: weight_type.to_string(),
                    weight_value: weight_value.to_string(),
                })
                .map_err(|error| DebugWriterError::Write { error })?;
            Ok(())
        });
    }

    pub fn write_fork_choices(
        itinerary_id: String,
        step: u32,
        segment_list: &SegmentList,
        discarded_choices: &Vec<MapDataPointRef>,
    ) {
        for segment in segment_list.clone().into_iter() {
            DebugWriter::exec(DebugStreamForkCHoices::name(), |writer| {
                writer
                    .serialize(DebugStreamForkCHoices {
                        itinerary_id: itinerary_id.clone(),
                        step_num: step.to_string(),
                        end_point_id: segment.get_end_point().borrow().id.to_string(),
                        line_point_0_lat: segment
                            .get_line()
                            .borrow()
                            .points
                            .0
                            .borrow()
                            .lat
                            .to_string(),
                        line_point_0_lon: segment
                            .get_line()
                            .borrow()
                            .points
                            .0
                            .borrow()
                            .lon
                            .to_string(),
                        line_point_1_lat: segment
                            .get_line()
                            .borrow()
                            .points
                            .1
                            .borrow()
                            .lat
                            .to_string(),
                        line_point_1_lon: segment
                            .get_line()
                            .borrow()
                            .points
                            .1
                            .borrow()
                            .lon
                            .to_string(),
                        segment_end_point: if segment.get_end_point()
                            == &segment.get_line().borrow().points.0
                        {
                            0
                        } else {
                            1
                        }
                        .to_string(),
                        discarded: discarded_choices
                            .iter()
                            .find(|c| c == &segment.get_end_point())
                            .is_some()
                            .to_string(),
                    })
                    .map_err(|error| DebugWriterError::Write { error })?;
                Ok(())
            });
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
        DebugWriter::exec(DebugStreamSteps::name(), |writer| {
            writer
                .serialize(DebugStreamSteps {
                    itinerary_id: itinerary_id.clone(),
                    step_num: step.to_string(),
                    move_result: move_result.to_string(),
                })
                .map_err(|error| DebugWriterError::Write { error })?;
            Ok(())
        });
    }

    pub fn write_itineraries(itineraries: &Vec<Itinerary>) -> () {
        for itinerary in itineraries {
            DebugWriter::exec(DebugStreamItineraries::name(), |writer| {
                writer
                    .serialize(DebugStreamItineraries {
                        itinerary_id: itinerary.id(),
                        waypoints_count: itinerary.waypoints.len().to_string(),
                        radius: itinerary.waypoint_radius.to_string(),
                        visit_all: itinerary.visit_all_waypoints.to_string(),
                    })
                    .map_err(|error| DebugWriterError::Write { error })?;
                Ok(())
            });
            for (idx, wp) in itinerary.waypoints.iter().enumerate() {
                DebugWriter::exec(DebugStreamItineraryWaypoints::name(), |writer| {
                    writer
                        .serialize(DebugStreamItineraryWaypoints {
                            itinerary_id: itinerary.id(),
                            idx: idx.to_string(),
                            lat: wp.borrow().lat.to_string(),
                            lon: wp.borrow().lon.to_string(),
                        })
                        .map_err(|error| DebugWriterError::Write { error })?;
                    Ok(())
                });
            }
        }
    }
}
