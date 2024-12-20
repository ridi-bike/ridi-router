use std::{num::ParseFloatError, path::PathBuf, string::ParseError, sync::OnceLock, time::Instant};

use clap::{Args, Parser, ValueEnum};
use geo::wkt;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    ipc_handler::{IpcHandler, IpcHandlerError, ResponseMessage, RouteMessage, RouterResult},
    map_data::graph::MapDataGraph,
    map_data_cache::{MapDataCache, MapDataCacheError},
    osm_data_reader::DataSource,
    result_writer::{DataDestination, ResultWriter, ResultWriterError},
    router::{
        generator::{Generator, RouteWithStats},
        route::Route,
        rules::RouterRules,
    },
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
#[arg()]
enum RoutingMode {
    StartFinish {
        #[arg(long, value_name = "LAT,LON")]
        start: String,

        #[arg(long, value_name = "LAT,LON")]
        finish: String,
    },
    RoundTrip {
        #[arg(long, value_name = "LAT,LON")]
        center: String,

        #[arg(
            long,
            value_name = "DEGREES",
            help = "Degrees, where: North: 0°, East: 90°, South: 180°, West: 270°"
        )]
        bearing: f32,

        #[arg(long, value_name = "KM")]
        distance: u32,
    },
}

#[derive(Subcommand)]
enum CliMode {
    Cache {
        #[arg(long, value_name = "FILE")]
        input: PathBuf,

        #[arg(long, value_name = "FILE")]
        cache_dir: PathBuf,
    },
    Server {
        #[arg(long, value_name = "FILE")]
        input: PathBuf,

        #[arg(long, value_name = "FILE")]
        cache_dir: Option<PathBuf>,

        #[arg(long, value_name = "NAME")]
        socket_name: Option<String>,
    },
    Client {
        #[arg(long, value_name = "FILE")]
        output: Option<PathBuf>,

        #[command(subcommand)]
        routing_mode: RoutingMode,

        #[arg(long, value_name = "NAME")]
        socket_name: Option<String>,

        #[arg(long, value_name = "FILE")]
        rule_file: Option<PathBuf>,
    },
    Dual {
        #[arg(long, value_name = "FILE")]
        input: PathBuf,

        #[arg(long, value_name = "FILE")]
        cache_dir: Option<PathBuf>,

        #[arg(long, value_name = "FILE")]
        output: Option<PathBuf>,

        #[arg(long, value_name = "FILE")]
        rule_file: Option<PathBuf>,

        #[command(subcommand)]
        routing_mode: RoutingMode,
    },
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum RoutingParams {
    StartFinish {
        start_lat: f32,
        start_lon: f32,
        finish_lat: f32,
        finish_lon: f32,
    },
    RoundTrip {
        lat: f32,
        lon: f32,
        bearing: f32,
        distance: u32,
    },
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
        routing_params: RoutingParams,
        data_destination: DataDestination,
        socket_name: Option<String>,
        rule_file: Option<PathBuf>,
    },
    Dual {
        data_source: DataSource,
        cache_dir: Option<PathBuf>,
        routing_params: RoutingParams,
        data_destination: DataDestination,
        rule_file: Option<PathBuf>,
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
                routing_mode,
                socket_name,
                rule_file,
            } => {
                let routing_params = get_router_params(routing_mode)
                    .expect("could not get start/finish coordinates");
                RouterMode::Client {
                    routing_params,
                    data_destination: get_data_destination(output)
                        .expect("could not get data destination"),
                    socket_name,
                    rule_file,
                }
            }
            CliMode::Dual {
                input,
                cache_dir,
                output,
                routing_mode,
                rule_file,
            } => {
                let routing_params = get_router_params(routing_mode)
                    .expect("could not get start/finish coordinates");
                RouterMode::Dual {
                    data_source: get_data_source(input).expect("could not get data source"),
                    cache_dir,
                    routing_params,
                    data_destination: get_data_destination(output)
                        .expect("could not get data destination"),
                    rule_file,
                }
            }
        };

        Self { mode }
    }

    #[tracing::instrument(skip_all)]
    fn generate_route(
        routing_params: &RoutingParams,
        rules: RouterRules,
    ) -> Result<Vec<RouteWithStats>, RouterRunnerError> {
        let (start_lat, start_lon, finish_lat, finish_lon) = match routing_params {
            RoutingParams::StartFinish {
                start_lat,
                start_lon,
                finish_lat,
                finish_lon,
            } => (*start_lat, *start_lon, *finish_lat, *finish_lon),
            RoutingParams::RoundTrip { lat, lon, .. } => (*lat, *lon, *lat, *lon),
        };
        let start = MapDataGraph::get()
            .get_closest_to_coords(start_lat, start_lon)
            .ok_or(RouterRunnerError::PointNotFound {
                point: "Start point".to_string(),
            })?;

        info!("Start point {start}");

        let finish = MapDataGraph::get()
            .get_closest_to_coords(finish_lat, finish_lon)
            .ok_or(RouterRunnerError::PointNotFound {
                point: "Finish point".to_string(),
            })?;

        info!("Finish point {finish}");

        let round_trip = if let RoutingParams::RoundTrip {
            bearing, distance, ..
        } = routing_params
        {
            Some((*bearing, *distance))
        } else {
            None
        };
        let route_generator = Generator::new(start.clone(), finish.clone(), round_trip, rules);
        let routes = route_generator.generate_routes();
        Ok(routes)
    }

    #[tracing::instrument(skip_all)]
    fn run_dual(
        &self,
        data_source: &DataSource,
        cache_dir: Option<PathBuf>,
        routing_params: &RoutingParams,
        data_destination: &DataDestination,
        rule_file: Option<PathBuf>,
    ) -> Result<(), RouterRunnerError> {
        let rules = RouterRules::read(rule_file).expect("Failed to read rules");
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
        let route_result = RouterRunner::generate_route(routing_params, rules);
        ResultWriter::write(
            data_destination.clone(),
            ResponseMessage {
                id: "oo".to_string(),
                result: route_result.map_or_else(
                    |error| RouterResult::Error {
                        message: format!("Error generating route {:?}", error),
                    },
                    |routes| RouterResult::Ok {
                        routes: routes
                            .iter()
                            .map(|route| RouteMessage {
                                coords: route
                                    .route
                                    .clone()
                                    .into_iter()
                                    .map(|segment| {
                                        (
                                            segment.get_end_point().borrow().lat,
                                            segment.get_end_point().borrow().lon,
                                        )
                                    })
                                    .collect(),
                                stats: route.stats.clone(),
                            })
                            .collect(),
                    },
                ),
            },
        )
        .map_err(|error| RouterRunnerError::ResultWrite { error })?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
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

    #[tracing::instrument(skip(self))]
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
        info!("startup took {}s", startup_end.as_secs());

        let ipc =
            IpcHandler::init(socket_name).map_err(|error| RouterRunnerError::Ipc { error })?;
        dbg!("ipc init done");
        ipc.listen(|request_message| {
            let route_res = RouterRunner::generate_route(
                &request_message.routing_params,
                request_message.rules,
            );

            ResponseMessage {
                id: request_message.id,
                result: route_res.map_or_else(
                    |error| RouterResult::Error {
                        message: format!("Error generating route {:?}", error),
                    },
                    |routes| RouterResult::Ok {
                        routes: routes
                            .iter()
                            .map(|route| RouteMessage {
                                coords: route
                                    .route
                                    .clone()
                                    .into_iter()
                                    .map(|segment| {
                                        (
                                            segment.get_end_point().borrow().lat,
                                            segment.get_end_point().borrow().lon,
                                        )
                                    })
                                    .collect(),
                                stats: route.stats.clone(),
                            })
                            .collect(),
                    },
                ),
            }
        })
        .map_err(|error| RouterRunnerError::Ipc { error })?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn run_client(
        &self,
        routing_params: &RoutingParams,
        data_destination: &DataDestination,
        socket_name: Option<String>,
        rule_file: Option<PathBuf>,
    ) -> Result<(), RouterRunnerError> {
        let rules = RouterRules::read(rule_file).expect("could not read rules");
        let ipc =
            IpcHandler::init(socket_name).map_err(|error| RouterRunnerError::Ipc { error })?;
        let response = ipc
            .connect(routing_params.clone(), rules)
            .map_err(|error| RouterRunnerError::Ipc { error })?;
        ResultWriter::write(data_destination.clone(), response)
            .map_err(|error| RouterRunnerError::ResultWrite { error })?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn run(&self) -> Result<(), RouterRunnerError> {
        match &self.mode {
            RouterMode::Dual {
                routing_params,
                data_source,
                cache_dir,
                data_destination,
                rule_file,
            } => self.run_dual(
                &data_source,
                cache_dir.clone(),
                routing_params,
                &data_destination,
                rule_file.clone(),
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
                routing_params,
                data_destination,
                socket_name,
                rule_file,
            } => self.run_client(
                &routing_params,
                &data_destination,
                socket_name.clone(),
                rule_file.clone(),
            ),
        }
    }
}

fn get_router_params(routing_mode: RoutingMode) -> Result<RoutingParams, RouterRunnerError> {
    match routing_mode {
        RoutingMode::StartFinish { start, finish } => {
            let mut start = start.split(",");
            let mut finish = finish.split(",");
            return Ok(RoutingParams::StartFinish {
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
            });
        }
        RoutingMode::RoundTrip {
            center,
            bearing,
            distance,
        } => {
            let mut center = center.split(",");
            let lat = center
                .next()
                .ok_or_else(|| RouterRunnerError::Coords {
                    name: "LAT".to_string(),
                    cause: "missing".to_string(),
                    error: None,
                })?
                .parse()
                .map_err(|error| RouterRunnerError::Coords {
                    name: "LAT".to_string(),
                    cause: "not parsable as f64".to_string(),
                    error: Some(error),
                })?;
            let lon = center
                .next()
                .ok_or_else(|| RouterRunnerError::Coords {
                    name: "LON".to_string(),
                    cause: "missing".to_string(),
                    error: None,
                })?
                .parse()
                .map_err(|error| RouterRunnerError::Coords {
                    name: "LON".to_string(),
                    cause: "not parsable as f64".to_string(),
                    error: Some(error),
                })?;
            return Ok(RoutingParams::RoundTrip {
                lat,
                lon,
                distance,
                bearing,
            });
        }
    }
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
