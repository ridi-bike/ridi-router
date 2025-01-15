use derive_name::Name;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::File,
    io::{self, Write},
    path::PathBuf,
    sync::{OnceLock, RwLock},
};
use tracing::error;
use typeshare::typeshare;

use crate::{
    map_data::graph::MapDataPointRef,
    router::{
        itinerary::Itinerary,
        navigator::WeightCalcResult,
        route::{segment_list::SegmentList, Route},
        walker::{WalkerError, WalkerMoveResult},
    },
};

#[derive(Serialize, derive_name::Name, struct_field_names_as_array::FieldNamesAsSlice)]
#[typeshare]
pub struct DebugStreamStepResults {
    pub itinerary_id: String,
    #[typeshare(serialized_as = "number")]
    pub step_num: i64,
    pub result: String,
    #[typeshare(serialized_as = "number")]
    pub chosen_fork_point_id: i64,
}

#[derive(Serialize, derive_name::Name, struct_field_names_as_array::FieldNamesAsSlice)]
#[typeshare]
pub struct DebugStreamForkChoiceWeights {
    pub itinerary_id: String,
    #[typeshare(serialized_as = "number")]
    pub step_num: i64,
    #[typeshare(serialized_as = "number")]
    pub end_point_id: i64,
    pub weight_name: String,
    pub weight_type: String,
    #[typeshare(serialized_as = "number")]
    pub weight_value: i64,
}

#[derive(Serialize, derive_name::Name, struct_field_names_as_array::FieldNamesAsSlice)]
#[typeshare]
pub struct DebugStreamForkChoices {
    pub itinerary_id: String,
    #[typeshare(serialized_as = "number")]
    pub step_num: i64,
    #[typeshare(serialized_as = "number")]
    pub end_point_id: i64,
    pub line_point_0_lat: f64,
    pub line_point_0_lon: f64,
    pub line_point_1_lat: f64,
    pub line_point_1_lon: f64,
    #[typeshare(serialized_as = "number")]
    pub segment_end_point: i64,
    pub discarded: bool,
}

#[derive(Serialize, derive_name::Name, struct_field_names_as_array::FieldNamesAsSlice)]
#[typeshare]
pub struct DebugStreamSteps {
    pub itinerary_id: String,
    #[typeshare(serialized_as = "number")]
    pub step_num: i64,
    pub move_result: String,
    pub route: String,
}

#[derive(Serialize, derive_name::Name, struct_field_names_as_array::FieldNamesAsSlice)]
#[typeshare]
pub struct DebugStreamItineraries {
    pub itinerary_id: String,
    #[typeshare(serialized_as = "number")]
    pub waypoints_count: i64,
    #[typeshare(serialized_as = "number")]
    pub radius: i64,
    pub visit_all: bool,
    pub start_lat: f32,
    pub start_lon: f32,
    pub finish_lat: f32,
    pub finish_lon: f32,
}

#[derive(Serialize, derive_name::Name, struct_field_names_as_array::FieldNamesAsSlice)]
#[typeshare]
pub struct DebugStreamItineraryWaypoints {
    pub itinerary_id: String,
    #[typeshare(serialized_as = "number")]
    pub idx: i64,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum DebugWriterError {
    #[error("Could not check if debug dir exists: {error}")]
    DirCheck { error: io::Error },
    #[error("Could not remove existing debug dir: {error}")]
    DirRemove { error: io::Error },
    #[error("Could not create metadata file: {error}")]
    MetadataCreate { error: io::Error },
    #[error("Could not create debug dir: {error}")]
    DirCreate { error: io::Error },
    #[error("Could not read global debug_writer")]
    StaticRead { error: String },
    #[error("Could not create file")]
    FileCreate {
        file_name: PathBuf,
        error: io::Error,
    },
    #[error("Could not write record")]
    Write { error: csv::Error },
    #[error("Could not flush file")]
    Flush { error: io::Error },
    #[error("Could not serialize route")]
    SerializeRoute { error: serde_json::Error },
    #[error("Could not serialize metadata {error}")]
    SerializeMetadata { error: serde_json::Error },
    #[error("Could not write metadata {error}")]
    MetadataWrite { error: io::Error },
}

static DEBUG_DIR: OnceLock<PathBuf> = OnceLock::new();

thread_local! {
    static DEBUG_WRITER: OnceLock<RwLock<DebugWriter>> = const { OnceLock::new() };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugMetadata {
    pub router_version: String,
}

pub struct DebugWriter {
    files: HashMap<String, csv::Writer<File>>,
}

impl DebugWriter {
    fn exec<T: Fn(&mut csv::Writer<File>) -> Result<(), DebugWriterError>>(
        file_type_id: &str,
        cb: T,
    ) {
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
                .map_err(|error| DebugWriterError::DirCreate { error })?;

            let metadata_file = Self::get_metadata_file_path(&dir_name);
            let mut file = File::create(metadata_file)
                .map_err(|error| DebugWriterError::MetadataCreate { error })?;
            let metadata = DebugMetadata {
                router_version: env!("CARGO_PKG_VERSION").to_string(),
            };
            file.write_all(
                serde_json::to_string(&metadata)
                    .map_err(|error| DebugWriterError::SerializeMetadata { error })?
                    .as_bytes(),
            )
            .map_err(|error| DebugWriterError::MetadataWrite { error })?;
            DEBUG_DIR.get_or_init(|| dir_name);
        }

        Ok(())
    }

    pub fn get_metadata_file_path(dir_name: &PathBuf) -> PathBuf {
        let mut metadata_file = dir_name.clone();
        metadata_file.push("metadata.json");
        metadata_file
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
                    step_num: step as i64,
                    result: result.to_string(),
                    chosen_fork_point_id: chosen_fork_point_id.map_or(0, |v| v as i64),
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
                    step_num: step as i64,
                    end_point_id: *end_point_id as i64,
                    weight_name: weight_name.to_string(),
                    weight_type: weight_type.to_string(),
                    weight_value: *weight_value as i64,
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
            DebugWriter::exec(DebugStreamForkChoices::name(), |writer| {
                writer
                    .serialize(DebugStreamForkChoices {
                        itinerary_id: itinerary_id.clone(),
                        step_num: step as i64,
                        end_point_id: segment.get_end_point().borrow().id as i64,
                        line_point_0_lat: segment.get_line().borrow().points.0.borrow().lat as f64,
                        line_point_0_lon: segment.get_line().borrow().points.0.borrow().lon as f64,
                        line_point_1_lat: segment.get_line().borrow().points.1.borrow().lat as f64,
                        line_point_1_lon: segment.get_line().borrow().points.1.borrow().lon as f64,
                        segment_end_point: if segment.get_end_point()
                            == &segment.get_line().borrow().points.0
                        {
                            0
                        } else {
                            1
                        } as i64,
                        discarded: discarded_choices
                            .iter()
                            .any(|c| c == segment.get_end_point()),
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
        route: &Route,
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
                    step_num: step as i64,
                    move_result: move_result.to_string(),
                    route: serde_json::to_string(
                        &route
                            .get_route_chunk_since_junction_before_last()
                            .iter()
                            .map(|segment| {
                                (
                                    segment.get_end_point().borrow().lat,
                                    segment.get_end_point().borrow().lon,
                                )
                            })
                            .collect::<Vec<_>>(),
                    )
                    .map_err(|error| DebugWriterError::SerializeRoute { error })?,
                })
                .map_err(|error| DebugWriterError::Write { error })?;
            Ok(())
        });
    }

    pub fn write_itineraries(itineraries: &Vec<Itinerary>) {
        for itinerary in itineraries {
            DebugWriter::exec(DebugStreamItineraries::name(), |writer| {
                writer
                    .serialize(DebugStreamItineraries {
                        itinerary_id: itinerary.id(),
                        waypoints_count: itinerary.waypoints.len() as i64,
                        radius: itinerary.waypoint_radius as i64,
                        visit_all: itinerary.visit_all_waypoints,
                        start_lat: itinerary.start.borrow().lat,
                        start_lon: itinerary.start.borrow().lon,
                        finish_lat: itinerary.finish.borrow().lat,
                        finish_lon: itinerary.finish.borrow().lon,
                    })
                    .map_err(|error| DebugWriterError::Write { error })?;
                Ok(())
            });
            for (idx, wp) in itinerary.waypoints.iter().enumerate() {
                DebugWriter::exec(DebugStreamItineraryWaypoints::name(), |writer| {
                    writer
                        .serialize(DebugStreamItineraryWaypoints {
                            itinerary_id: itinerary.id(),
                            idx: idx as i64,
                            lat: wp.borrow().lat as f64,
                            lon: wp.borrow().lon as f64,
                        })
                        .map_err(|error| DebugWriterError::Write { error })?;
                    Ok(())
                });
            }
        }
    }
}
