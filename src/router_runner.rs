use std::{num::ParseFloatError, path::PathBuf, str::FromStr, time::Instant};

use clap::Parser;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    debug::writer::DebugWriter,
    ipc_handler::{IpcHandler, IpcHandlerError, ResponseMessage, RouteMessage, RouterResult},
    map_data::graph::MapDataGraph,
    map_data_cache::{MapDataCache, MapDataCacheError},
    osm_data_reader::DataSource,
    result_writer::{DataDestination, ResultWriter, ResultWriterError},
    router::{
        generator::{Generator, RouteWithStats},
        rules::RouterRules,
    },
};

use clap::Subcommand;

#[derive(Debug, thiserror::Error)]
pub enum RouterRunnerError {
    #[error("Output File Invalid '{filename}'")]
    OutputFileInvalid { filename: String },

    #[error("Input File Invalid '{filename}'")]
    InputFileInvalid { filename: String },

    #[error("Input File Format Incorrect for '{filename}'")]
    InputFileFormatIncorrect { filename: PathBuf },

    #[error("Output File Format Incorrect for '{filename}'")]
    OutputFileFormatIncorrect { filename: PathBuf },

    #[error("Coordinate error for {name}: {cause}{}", .error.as_ref().map(|e| format!(": {}", e)).unwrap_or_default())]
    Coords {
        name: String,
        cause: String,
        error: Option<ParseFloatError>,
    },

    #[error("IPC error: {error}")]
    Ipc { error: IpcHandlerError },

    #[error("Could not find {point} on map")]
    PointNotFound { point: String },

    #[error("Failed to write result: {error}")]
    ResultWrite { error: ResultWriterError },

    #[error("Failed to write cache: {error}")]
    CacheWrite { error: MapDataCacheError },

    #[cfg(feature = "debug-viewer")]
    #[error("Failed run debug viewer: {error}")]
    DebugViewer {
        error: crate::debug::viewer::DebugViewerError,
    },
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    pub mode: CliMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coords {
    lat: f32,
    lon: f32,
}

impl FromStr for Coords {
    type Err = RouterRunnerError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.split(",");
        let lat = split
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
            })?;

        let lon = split
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
            })?;
        Ok(Coords { lat, lon })
    }
}

impl FromStr for DataSource {
    type Err = RouterRunnerError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let file = PathBuf::from_str(s).map_err(|_error| RouterRunnerError::InputFileInvalid {
            filename: s.to_string(),
        })?;
        if let Some(ext) = file.extension() {
            if ext == "json" {
                return Ok(DataSource::JsonFile { file });
            } else if ext == "pbf" {
                return Ok(DataSource::PbfFile { file });
            }
        }
        Err(RouterRunnerError::InputFileFormatIncorrect { filename: file })
    }
}

impl FromStr for DataDestination {
    type Err = RouterRunnerError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "DataDestination::Stdout" {
            return Ok(DataDestination::Stdout);
        }
        let file = PathBuf::from_str(s).map_err(|_error| RouterRunnerError::OutputFileInvalid {
            filename: s.to_string(),
        })?;
        if let Some(ext) = file.extension() {
            if ext == "json" {
                return Ok(DataDestination::Json { file });
            } else if ext == "gpx" {
                return Ok(DataDestination::Gpx { file });
            }
        }
        return Err(RouterRunnerError::OutputFileFormatIncorrect { filename: file });
    }
}

#[derive(Clone, Subcommand, Debug, Serialize, Deserialize)]
#[arg()]
pub enum RoutingMode {
    /// Generate a route between specific Start coordinates and specific Finish coordinates
    StartFinish {
        #[arg(long, value_name = "LAT,LON", value_parser = clap::value_parser!(Coords))]
        /// Start coordinates in the format of 11.12543,32.12432
        start: Coords,

        #[arg(long, value_name = "LAT,LON")]
        /// Finish coordinates in the format of 11.12543,32.12432
        finish: Coords,
    },
    /// Generate a route that starts and finishes at the same point and loops in a direction
    /// for a specified distance
    RoundTrip {
        #[arg(long, value_name = "LAT,LON")]
        /// Start and finish coordinates in the format of 11.12543,32.12432
        start_finish: Coords,

        #[arg(long, value_name = "DEGREES")]
        /// Degrees, where: North: 0°, East: 90°, South: 180°, West: 270°
        bearing: f32,

        #[arg(long, value_name = "METERS")]
        /// Distance in meters of the desired trip distance
        distance: u32,
    },
}

#[derive(Subcommand)]
enum CliMode {
    /// Load input data and generate a route
    GenerateRoute {
        #[arg(long, value_name = "FILE")]
        /// Input file name for json or osm.pbf file
        input: DataSource,

        #[arg(long, value_name = "FILE")]
        /// Directory to store the generated cache. If specified, it will attempt to read form the
        /// cache, if not found, inout file will be read. If cache is not present, it will be
        /// generated for future
        cache_dir: Option<PathBuf>,

        #[arg(
            long,
            value_name = "FILE",
            required = false,
            default_value = "DataDestination::Stdout"
        )]
        /// Destination json or gpx file path and name. If not specified, results piped to screen
        output: DataDestination,

        #[arg(long, value_name = "FILE")]
        /// JSON file with specified rules for route generation. Default values used if file not
        /// specified
        rule_file: Option<PathBuf>,

        #[arg(long, value_name = "DIR")]
        /// Write debug files to a directory. Will slow down the route generation. Used for
        /// examining route generation rules. Can be viewed with the 'debug-viewer' binary
        debug_dir: Option<PathBuf>,

        #[command(subcommand)]
        /// Routing mode to generate a route between start and finish coordinates or a round trip
        /// mode to generate a route with the same start and finish coordinates
        routing_mode: RoutingMode,
    },
    /// Start a server for generating routes
    StartServer {
        #[arg(long, value_name = "FILE")]
        /// Input file name for json or osm.pbf file
        input: DataSource,

        #[arg(long, value_name = "FILE")]
        /// Directory to store the generated cache. If specified, it will attempt to read form the
        /// cache, if not found, inout file will be read. If cache is not present, it will be
        /// generated for future
        cache_dir: Option<PathBuf>,

        #[arg(long, value_name = "NAME")]
        /// Socket name in advanced cases where several servers are required to be running at the same time
        socket_name: Option<String>,
    },
    /// Start a client to connect to a running server to generate a route
    StartClient {
        #[arg(
            long,
            value_name = "FILE",
            required = false,
            default_value = "DataDestination::Stdout"
        )]
        /// Destination json or gpx file path and name. If not specified, results piped to screen
        output: DataDestination,

        #[command(subcommand)]
        /// Routing mode to generate a route between start and finish coordinates or a round trip
        /// mode to generate a route with the same start and finish coordinates
        routing_mode: RoutingMode,

        #[arg(long, value_name = "NAME")]
        /// Socket name in advanced cases where several servers are required to be running at the same time
        socket_name: Option<String>,

        #[arg(long, value_name = "FILE")]
        /// JSON file with specified rules for route generation. Default values used if file not
        /// specified
        rule_file: Option<PathBuf>,
    },
    /// Create an input data cache
    PrepCache {
        #[arg(long, value_name = "FILE")]
        /// Input file name for json or osm.pbf file
        input: DataSource,

        #[arg(long, value_name = "DIR")]
        /// Directory to store the generated cache
        cache_dir: PathBuf,
    },
    #[cfg(feature = "debug-viewer")]
    DebugViewer {
        #[arg(long, value_name = "DIR")]
        /// Load a directory with debug files generated when generating a route
        debug_dir: PathBuf,
    },
}

pub struct RouterRunner;

impl RouterRunner {
    #[tracing::instrument(skip_all)]
    fn generate_route(
        routing_mode: &RoutingMode,
        rules: RouterRules,
    ) -> Result<Vec<RouteWithStats>, RouterRunnerError> {
        let (start_lat, start_lon, finish_lat, finish_lon) = match routing_mode {
            RoutingMode::StartFinish { start, finish } => {
                (start.lat, start.lon, finish.lat, finish.lon)
            }
            RoutingMode::RoundTrip { start_finish, .. } => (
                start_finish.lat,
                start_finish.lon,
                start_finish.lat,
                start_finish.lon,
            ),
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

        let round_trip = if let RoutingMode::RoundTrip {
            bearing, distance, ..
        } = routing_mode
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
        data_source: &DataSource,
        cache_dir: Option<PathBuf>,
        routing_mode: &RoutingMode,
        data_destination: &DataDestination,
        rule_file: Option<PathBuf>,
        debug_dir: Option<PathBuf>,
    ) -> Result<(), RouterRunnerError> {
        DebugWriter::init(debug_dir).expect("Failed to set up debugging");
        let rules = RouterRules::read(rule_file).expect("Failed to read rules");
        let mut data_cache = MapDataCache::init(cache_dir, data_source);
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
        let route_result = RouterRunner::generate_route(routing_mode, rules);
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

    #[tracing::instrument]
    fn run_cache(data_source: &DataSource, cache_dir: PathBuf) -> Result<(), RouterRunnerError> {
        let startup_start = Instant::now();

        let mut data_cache = MapDataCache::init(Some(cache_dir), data_source);
        data_cache
            .read_input_metadata()
            .map_err(|error| RouterRunnerError::CacheWrite { error })?;
        MapDataGraph::init(data_source);
        let packed_data = MapDataGraph::get().pack();
        data_cache
            .write_cache(packed_data)
            .map_err(|error| RouterRunnerError::CacheWrite { error })?;

        let startup_end = startup_start.elapsed();
        info!("cache gen took {}s", startup_end.as_secs());

        Ok(())
    }

    #[tracing::instrument]
    fn run_server(
        data_source: &DataSource,
        cache_dir: Option<PathBuf>,
        socket_name: Option<String>,
    ) -> Result<(), RouterRunnerError> {
        let startup_start = Instant::now();

        let mut data_cache = MapDataCache::init(cache_dir, data_source);
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
            let route_res =
                RouterRunner::generate_route(&request_message.routing_mode, request_message.rules);

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

    #[tracing::instrument]
    fn run_client(
        routing_mode: &RoutingMode,
        data_destination: &DataDestination,
        socket_name: Option<String>,
        rule_file: Option<PathBuf>,
    ) -> Result<(), RouterRunnerError> {
        let rules = RouterRules::read(rule_file).expect("could not read rules");
        let ipc =
            IpcHandler::init(socket_name).map_err(|error| RouterRunnerError::Ipc { error })?;
        let response = ipc
            .connect(routing_mode, rules)
            .map_err(|error| RouterRunnerError::Ipc { error })?;
        ResultWriter::write(data_destination.clone(), response)
            .map_err(|error| RouterRunnerError::ResultWrite { error })?;
        Ok(())
    }

    #[tracing::instrument]
    pub fn run() -> Result<(), RouterRunnerError> {
        let cli = Cli::parse();
        match &cli.mode {
            CliMode::GenerateRoute {
                routing_mode,
                cache_dir,
                rule_file,
                input,
                output,
                debug_dir,
            } => RouterRunner::run_dual(
                &input,
                cache_dir.clone(),
                routing_mode,
                &output,
                rule_file.clone(),
                debug_dir.clone(),
            ),
            CliMode::PrepCache { input, cache_dir } => {
                RouterRunner::run_cache(input, cache_dir.clone())
            }
            CliMode::StartServer {
                input,
                cache_dir,
                socket_name,
            } => RouterRunner::run_server(&input, cache_dir.clone(), socket_name.clone()),
            CliMode::StartClient {
                routing_mode,
                output,
                socket_name,
                rule_file,
            } => RouterRunner::run_client(
                &routing_mode,
                &output,
                socket_name.clone(),
                rule_file.clone(),
            ),
            #[cfg(feature = "debug-viewer")]
            CliMode::DebugViewer { debug_dir } => {
                Ok(crate::debug::viewer::DebugViewer::run(debug_dir.clone())
                    .map_err(|error| RouterRunnerError::DebugViewer { error })?)
            }
        }
    }
}
