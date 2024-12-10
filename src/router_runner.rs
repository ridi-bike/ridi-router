use std::{num::ParseFloatError, path::PathBuf, string::ParseError, sync::OnceLock, time::Instant};

use clap::Parser;
use tracing::info;

use crate::{
    ipc_handler::{CoordsMessage, IpcHandler, IpcHandlerError, ResponseMessage, RouteMessage},
    map_data::graph::MapDataGraph,
    map_data_cache::{MapDataCache, MapDataCacheError},
    osm_data_reader::DataSource,
    result_writer::{DataDestination, ResultWriter, ResultWriterError},
    router::{generator::Generator, route::Route},
};

use clap::Subcommand;

#[derive(Debug)]
pub enum RouterRunnerError {
    InputFileFormatIncorrect {
        filename: PathBuf,
    },
    OutputFileFormatIncorrect {
        filename: PathBuf,
    },
    Coords {
        name: String,
        cause: String,
        error: Option<ParseFloatError>,
    },
    Ipc {
        error: IpcHandlerError,
    },
    PointNotFound {
        point: String,
    },
    ResultWrite {
        error: ResultWriterError,
    },
    CacheWrite {
        error: MapDataCacheError,
    },
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    pub mode: CliMode,
}

#[derive(Subcommand)]
enum CliMode {
    Cache {
        #[arg(short, long, value_name = "FILE")]
        input: PathBuf,

        #[arg(short, long, value_name = "FILE")]
        cache_dir: PathBuf,
    },
    Server {
        #[arg(short, long, value_name = "FILE")]
        input: PathBuf,

        #[arg(short, long, value_name = "FILE")]
        cache_dir: Option<PathBuf>,

        #[arg(short, long, value_name = "NAME")]
        socket_name: Option<String>,
    },
    Client {
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,

        #[arg(short, long, value_name = "COORDINATES")]
        start: String,

        #[arg(short, long, value_name = "COORDINATES")]
        finish: String,

        #[arg(short, long, value_name = "NAME")]
        socket_name: Option<String>,
    },
    Dual {
        #[arg(short, long, value_name = "FILE")]
        input: PathBuf,

        #[arg(short, long, value_name = "FILE")]
        cache_dir: Option<PathBuf>,

        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,

        #[arg(short, long, value_name = "COORDINATES")]
        start: String,

        #[arg(short, long, value_name = "COORDINATES")]
        finish: String,
    },
}

#[derive(Debug)]
pub struct StartFinish {
    pub start_lat: f32,
    pub start_lon: f32,
    pub finish_lat: f32,
    pub finish_lon: f32,
}

#[derive(Debug)]
pub enum RouterMode {
    Cache {
        data_source: DataSource,
        cache_dir: PathBuf,
    },
    Server {
        data_source: DataSource,
        cache_dir: Option<PathBuf>,
        socket_name: Option<String>,
    },
    Client {
        start_finish: StartFinish,
        data_destination: DataDestination,
        socket_name: Option<String>,
    },
    Dual {
        data_source: DataSource,
        cache_dir: Option<PathBuf>,
        start_finish: StartFinish,
        data_destination: DataDestination,
    },
}

pub struct RouterRunner {
    mode: RouterMode,
}

impl RouterRunner {
    pub fn init() -> Self {
        let cli = Cli::parse();
        let mode = match cli.mode {
            CliMode::Cache { input, cache_dir } => RouterMode::Cache {
                data_source: get_data_source(input).expect("could not get data source"),
                cache_dir,
            },
            CliMode::Server {
                input,
                cache_dir,
                socket_name,
            } => RouterMode::Server {
                data_source: get_data_source(input).expect("could not get data source"),
                cache_dir,
                socket_name,
            },
            CliMode::Client {
                output,
                start,
                finish,
                socket_name,
            } => {
                let start_finish = get_start_finish(start, finish)
                    .expect("could not get start/finish coordinates");
                RouterMode::Client {
                    start_finish,
                    data_destination: get_data_destination(output)
                        .expect("could not get data destination"),
                    socket_name,
                }
            }
            CliMode::Dual {
                input,
                cache_dir,
                output,
                start,
                finish,
            } => {
                let start_finish = get_start_finish(start, finish)
                    .expect("could not get start/finish coordinates");
                RouterMode::Dual {
                    data_source: get_data_source(input).expect("could not get data source"),
                    cache_dir,
                    start_finish,
                    data_destination: get_data_destination(output)
                        .expect("could not get data destination"),
                }
            }
        };

        Self { mode }
    }

    fn generate_route(start_finish: &StartFinish) -> Result<Vec<Route>, RouterRunnerError> {
        let start = MapDataGraph::get()
            .get_closest_to_coords(start_finish.start_lat, start_finish.start_lon)
            .ok_or(RouterRunnerError::PointNotFound {
                point: "Start point".to_string(),
            })?;

        let finish = MapDataGraph::get()
            .get_closest_to_coords(start_finish.finish_lat, start_finish.finish_lon)
            .ok_or(RouterRunnerError::PointNotFound {
                point: "Finish point".to_string(),
            })?;

        let route_generator = Generator::new(start.clone(), finish.clone());
        let routes = route_generator.generate_routes();
        Ok(routes)
    }

    fn run_dual(
        &self,
        data_source: &DataSource,
        cache_dir: Option<PathBuf>,
        start_finish: &StartFinish,
        data_destination: &DataDestination,
    ) -> Result<(), RouterRunnerError> {
        let mut data_cache = MapDataCache::init(cache_dir);
        let cached_map_data = data_cache.read_cache();
        let cached_map_data = match cached_map_data {
            Ok(d) => d,
            Err(error) => {
                tracing::error!("Failed to process cache: {:?}", error);
                None
            }
        };
        if let Some(packed_data) = cached_map_data {
            MapDataGraph::unpack(packed_data);
        } else {
            MapDataGraph::init(data_source);
            let packed_data = MapDataGraph::get().pack();
            if let Err(error) = data_cache.write_cache(packed_data) {
                tracing::error!("Failed to write cache: {:?}", error);
            }
        }
        let route_result = RouterRunner::generate_route(start_finish);
        ResultWriter::write(
            data_destination.clone(),
            ResponseMessage {
                id: "oo".to_string(),
                result: route_result
                    .map(|routes| {
                        routes
                            .iter()
                            .map(|route| RouteMessage {
                                coords: route
                                    .clone()
                                    .into_iter()
                                    .map(|segment| CoordsMessage {
                                        lat: segment.get_end_point().borrow().lat,
                                        lon: segment.get_end_point().borrow().lon,
                                    })
                                    .collect::<Vec<CoordsMessage>>(),
                            })
                            .collect()
                    })
                    .map_err(|error| format!("Error generating route {:?}", error)),
            },
        )
        .map_err(|error| RouterRunnerError::ResultWrite { error })?;
        Ok(())
    }

    fn run_cache(
        &self,
        data_source: &DataSource,
        cache_dir: PathBuf,
    ) -> Result<(), RouterRunnerError> {
        let startup_start = Instant::now();

        let data_cache = MapDataCache::init(Some(cache_dir));
        MapDataGraph::init(data_source);
        let packed_data = MapDataGraph::get().pack();
        data_cache
            .write_cache(packed_data)
            .map_err(|error| RouterRunnerError::CacheWrite { error })?;

        let startup_end = startup_start.elapsed();
        info!("cache gen took {}s", startup_end.as_secs());

        Ok(())
    }

    fn run_server(
        &self,
        data_source: &DataSource,
        cache_dir: Option<PathBuf>,
        socket_name: Option<String>,
    ) -> Result<(), RouterRunnerError> {
        let startup_start = Instant::now();

        let mut data_cache = MapDataCache::init(cache_dir);
        let cached_map_data = data_cache.read_cache();
        let cached_map_data = match cached_map_data {
            Ok(d) => d,
            Err(error) => {
                tracing::error!("Failed to process cache: {:?}", error);
                None
            }
        };
        if let Some(packed_data) = cached_map_data {
            MapDataGraph::unpack(packed_data);
        } else {
            MapDataGraph::init(data_source);
            let packed_data = MapDataGraph::get().pack();
            if let Err(error) = data_cache.write_cache(packed_data) {
                tracing::error!("Failed to write cache: {:?}", error);
            }
        }

        let startup_end = startup_start.elapsed();
        eprintln!("startup took {}s", startup_end.as_secs());

        let ipc =
            IpcHandler::init(socket_name).map_err(|error| RouterRunnerError::Ipc { error })?;
        dbg!("ipc init done");
        ipc.listen(|request_message| {
            let route_res = RouterRunner::generate_route(&StartFinish {
                start_lat: request_message.start.lat,
                start_lon: request_message.start.lon,
                finish_lat: request_message.finish.lat,
                finish_lon: request_message.finish.lon,
            });

            ResponseMessage {
                id: request_message.id,
                result: route_res
                    .map(|routes| {
                        routes
                            .iter()
                            .map(|route| RouteMessage {
                                coords: route
                                    .clone()
                                    .into_iter()
                                    .map(|segment| CoordsMessage {
                                        lat: segment.get_end_point().borrow().lat,
                                        lon: segment.get_end_point().borrow().lon,
                                    })
                                    .collect::<Vec<CoordsMessage>>(),
                            })
                            .collect()
                    })
                    .map_err(|error| format!("Error generating route {:?}", error)),
            }
        })
        .map_err(|error| RouterRunnerError::Ipc { error })?;
        Ok(())
    }

    fn run_client(
        &self,
        start_finish: &StartFinish,
        data_destination: &DataDestination,
        socket_name: Option<String>,
    ) -> Result<(), RouterRunnerError> {
        let ipc =
            IpcHandler::init(socket_name).map_err(|error| RouterRunnerError::Ipc { error })?;
        let response = ipc
            .connect(start_finish)
            .map_err(|error| RouterRunnerError::Ipc { error })?;
        ResultWriter::write(data_destination.clone(), response)
            .map_err(|error| RouterRunnerError::ResultWrite { error })?;
        Ok(())
    }

    pub fn run(&self) -> Result<(), RouterRunnerError> {
        match &self.mode {
            RouterMode::Dual {
                start_finish,
                data_source,
                cache_dir,
                data_destination,
            } => self.run_dual(
                &data_source,
                cache_dir.clone(),
                &start_finish,
                &data_destination,
            ),
            RouterMode::Cache {
                data_source,
                cache_dir,
            } => self.run_cache(data_source, cache_dir.clone()),
            RouterMode::Server {
                data_source,
                cache_dir,
                socket_name,
            } => self.run_server(&data_source, cache_dir.clone(), socket_name.clone()),
            RouterMode::Client {
                start_finish,
                data_destination,
                socket_name,
            } => self.run_client(&start_finish, &data_destination, socket_name.clone()),
        }
    }
}

fn get_start_finish(start: String, finish: String) -> Result<StartFinish, RouterRunnerError> {
    let mut start = start.split(",");
    let mut finish = finish.split(",");
    Ok(StartFinish {
        start_lat: start
            .next()
            .ok_or_else(|| RouterRunnerError::Coords {
                name: "Start LAT".to_string(),
                cause: "missing".to_string(),
                error: None,
            })?
            .parse()
            .map_err(|error| RouterRunnerError::Coords {
                name: "Start LAT".to_string(),
                cause: "not parsable as f64".to_string(),
                error: Some(error),
            })?,
        start_lon: start
            .next()
            .ok_or_else(|| RouterRunnerError::Coords {
                name: "Start LON".to_string(),
                cause: "missing".to_string(),
                error: None,
            })?
            .parse()
            .map_err(|error| RouterRunnerError::Coords {
                name: "Start Lon".to_string(),
                cause: "not parsable as f64".to_string(),
                error: Some(error),
            })?,
        finish_lat: finish
            .next()
            .ok_or_else(|| RouterRunnerError::Coords {
                name: "Finish LAT".to_string(),
                cause: "missing".to_string(),
                error: None,
            })?
            .parse()
            .map_err(|error| RouterRunnerError::Coords {
                name: "Finish LAT".to_string(),
                cause: "not parsable as f64".to_string(),
                error: Some(error),
            })?,
        finish_lon: finish
            .next()
            .ok_or_else(|| RouterRunnerError::Coords {
                name: "Finish LON".to_string(),
                cause: "missing".to_string(),
                error: None,
            })?
            .parse()
            .map_err(|error| RouterRunnerError::Coords {
                name: "Finish LON".to_string(),
                cause: "not parsable as f64".to_string(),
                error: Some(error),
            })?,
    })
}
fn get_data_source(file: PathBuf) -> Result<DataSource, RouterRunnerError> {
    if let Some(ext) = file.extension() {
        if ext == "json" {
            return Ok(DataSource::JsonFile { file });
        } else if ext == "pbf" {
            return Ok(DataSource::PbfFile { file });
        }
    }
    Err(RouterRunnerError::InputFileFormatIncorrect { filename: file })
}
fn get_data_destination(output: Option<PathBuf>) -> Result<DataDestination, RouterRunnerError> {
    if let Some(output) = output {
        if let Some(ext) = output.extension() {
            if ext == "json" {
                return Ok(DataDestination::Json { file: output });
            } else if ext == "gpx" {
                return Ok(DataDestination::Gpx { file: output });
            }
        }
        return Err(RouterRunnerError::OutputFileFormatIncorrect { filename: output });
    }

    Ok(DataDestination::Stdout)
}
