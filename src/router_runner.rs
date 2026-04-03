use anyhow::{Context, Result};
use std::{num::ParseFloatError, path::PathBuf, str::FromStr};

use clap::Parser;
use serde::{Deserialize, Serialize};
use tracing::{info, trace};

use crate::router::generator::{GeneratorError, WP_LOOKUP_ALLOWED_HWS};
use crate::{
    ipc_handler::IpcHandlerError,
    map_data::graph::MapDataGraph,
    result_writer::{DataDestination, ResultWriter, ResultWriterError},
    route_output::RouteComputation,
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

    #[error("Failed to generate routes: {error}")]
    GenerateRoute { error: GeneratorError },
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
        Err(RouterRunnerError::OutputFileFormatIncorrect { filename: file })
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
        #[arg(long, value_name = "DIR")]
        /// Tiles directory containing manifest.json and RMDF tiles
        tiles: PathBuf,

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

        #[command(subcommand)]
        /// Routing mode to generate a route between start and finish coordinates or a round trip
        /// mode to generate a route with the same start and finish coordinates
        routing_mode: RoutingMode,
    },
    /// Generate RMDF tiles from OSM PBF file(s)
    ///
    /// Single file mode: --input FILE
    /// Multi-file mode: --input-dir DIR
    GenerateTiles {
        #[arg(short, long, value_name = "FILE", conflicts_with = "input_dir")]
        /// Input OSM PBF file (single file mode)
        input: Option<PathBuf>,

        #[arg(long, value_name = "DIR", conflicts_with = "input")]
        /// Input directory containing OSM PBF files (multi-file mode)
        input_dir: Option<PathBuf>,

        #[arg(short, long, visible_alias = "output-dir", value_name = "DIR")]
        /// Output directory for tiles and manifest
        output: PathBuf,

        #[arg(long, default_value = "1.0")]
        /// Tile size in degrees (e.g., 0.1, 1.0)
        tile_size_deg: f32,

        #[arg(long, value_name = "FILE")]
        /// Path for intermediate database (multi-file mode only)
        /// Default: temp file in output directory
        db_path: Option<PathBuf>,
    },
    /// Generate JSON schema file for rule files
    #[cfg(feature = "rule-schema-writer")]
    RuleSchemaWrite {
        #[arg(long, value_name = "FILE")]
        /// Destination location of the JSON schema file for the rule file
        destination: PathBuf,
    },
    /// Start RMDF debug viewer web server
    #[cfg(feature = "rmdf-viewer")]
    RmdfViewer {
        #[arg(long, value_name = "DIR")]
        /// Directory containing manifest.json and RMDF tile files
        input_dir: PathBuf,
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
            .get_closest_to_coords(
                start_lat,
                start_lon,
                &rules,
                false,
                Some(&WP_LOOKUP_ALLOWED_HWS),
            )
            .ok_or(RouterRunnerError::PointNotFound {
                point: "Start point".to_string(),
            })?;

        trace!("Start point {start}");

        let finish = MapDataGraph::get()
            .get_closest_to_coords(
                finish_lat,
                finish_lon,
                &rules,
                false,
                Some(&WP_LOOKUP_ALLOWED_HWS),
            )
            .ok_or(RouterRunnerError::PointNotFound {
                point: "Finish point".to_string(),
            })?;

        trace!("Finish point {finish}");

        let round_trip = if let RoutingMode::RoundTrip {
            bearing, distance, ..
        } = routing_mode
        {
            Some((*bearing, *distance))
        } else {
            None
        };
        let route_generator = Generator::new(start.clone(), finish.clone(), round_trip, rules);
        let routes = route_generator
            .generate_routes()
            .map_err(|error| RouterRunnerError::GenerateRoute { error })?;
        Ok(routes)
    }

    #[tracing::instrument(skip_all)]
    fn generate_route_with_tiles(
        _tile_manager: &'static mut crate::rmdf::TileManager,
        routing_mode: &RoutingMode,
        rules: RouterRules,
    ) -> Result<Vec<RouteWithStats>, RouterRunnerError> {
        // Note: TileManager is already initialized in MapDataGraph
        // The routing code will access it via MapDataGraph::get()
        // The _tile_manager parameter is kept for API consistency but unused

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

        // Find start point using MapDataGraph (which uses TileManager internally)
        let start = MapDataGraph::get()
            .get_closest_to_coords(
                start_lat,
                start_lon,
                &rules,
                false,
                Some(&WP_LOOKUP_ALLOWED_HWS),
            )
            .ok_or(RouterRunnerError::PointNotFound {
                point: "Start point".to_string(),
            })?;

        trace!("Start point {start}");

        let finish = MapDataGraph::get()
            .get_closest_to_coords(
                finish_lat,
                finish_lon,
                &rules,
                false,
                Some(&WP_LOOKUP_ALLOWED_HWS),
            )
            .ok_or(RouterRunnerError::PointNotFound {
                point: "Finish point".to_string(),
            })?;

        trace!("Finish point {finish}");

        let round_trip = if let RoutingMode::RoundTrip {
            bearing, distance, ..
        } = routing_mode
        {
            Some((*bearing, *distance))
        } else {
            None
        };

        let route_generator = Generator::new(start.clone(), finish.clone(), round_trip, rules);
        let routes = route_generator
            .generate_routes()
            .map_err(|error| RouterRunnerError::GenerateRoute { error })?;

        Ok(routes)
    }

    #[tracing::instrument(skip_all)]
    fn run_generate_route(
        tiles_dir: PathBuf,
        routing_mode: &RoutingMode,
        data_destination: &DataDestination,
        rule_file: Option<PathBuf>,
    ) -> Result<()> {
        let rules = RouterRules::read(rule_file).context("Failed to read rules")?;

        info!("Using RMDF tiles from {:?}", tiles_dir);

        let mut tile_manager =
            crate::rmdf::TileManager::new(tiles_dir).context("Failed to initialize TileManager")?;

        // SAFETY: TileManager lives for entire routing request
        // This is safe because we control the execution flow
        let tile_manager_static: &'static mut crate::rmdf::TileManager =
            unsafe { std::mem::transmute(&mut tile_manager) };

        info!("Route generation started");

        let route_result =
            RouterRunner::generate_route_with_tiles(tile_manager_static, routing_mode, rules)
                .map(RouteComputation::from)
                .map_err(|error| format!("Error generating route {:?}", error));
        ResultWriter::write(data_destination.clone(), route_result)
            .map_err(|error| RouterRunnerError::ResultWrite { error })?;
        Ok(())
    }

    #[tracing::instrument]
    pub fn run() -> Result<()> {
        let cli = Cli::parse();
        match &cli.mode {
            CliMode::GenerateRoute {
                routing_mode,
                tiles,
                rule_file,
                output,
            } => RouterRunner::run_generate_route(
                tiles.clone(),
                routing_mode,
                output,
                rule_file.clone(),
            ),
            CliMode::GenerateTiles {
                input,
                input_dir,
                output,
                tile_size_deg,
                db_path,
            } => {
                // Validate exactly one input option is provided
                match (&input, &input_dir) {
                    (Some(_), Some(_)) => {
                        anyhow::bail!("Cannot use both --input and --input-dir");
                    }
                    (None, None) => {
                        anyhow::bail!("Must provide either --input or --input-dir");
                    }
                    _ => {}
                }

                if let Some(input_file) = input {
                    // Single-PBF mode (existing behavior)
                    use crate::rmdf::generator::TileGenerator;

                    // Validate that input is a file, not a directory
                    if input_file.is_dir() {
                        anyhow::bail!(
                            "--input expects a PBF file, but {:?} is a directory. Use --input-dir for multi-file mode.",
                            input_file
                        );
                    }

                    info!(
                        "Generating tiles from {:?} to {:?} (tile_size_deg={}°)",
                        input_file, output, tile_size_deg
                    );

                    let generator =
                        TileGenerator::new(input_file.clone(), output.clone(), *tile_size_deg)?;
                    generator.generate()?;

                    info!("Tile generation complete");
                } else if let Some(input_dir) = input_dir {
                    // Multi-PBF mode (new behavior)
                    use crate::rmdf::generator::MultiPbfGenerator;

                    info!(
                        "Generating tiles from directory {:?} to {:?} (tile_size_deg={}°)",
                        input_dir, output, tile_size_deg
                    );

                    // Use provided db_path or default to temp location in output directory
                    let db_path = db_path
                        .clone()
                        .unwrap_or_else(|| output.join(".intermediate.redb"));

                    let generator = MultiPbfGenerator::new(
                        input_dir.clone(),
                        output.clone(),
                        *tile_size_deg,
                        db_path,
                    )?;
                    generator.generate()?;

                    info!("Multi-PBF tile generation complete");
                }

                Ok(())
            }
            #[cfg(feature = "rule-schema-writer")]
            CliMode::RuleSchemaWrite { destination } => {
                Ok(crate::router::rules::generate_json_schema(destination)?)
            }
            #[cfg(feature = "rmdf-viewer")]
            CliMode::RmdfViewer { input_dir } => crate::debug::rmdf_viewer::run(input_dir.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{
        route_output::RouteComputation,
        router::{
            generator::RouteWithStats,
            route::{segment::Segment, Route, RouteStatElement, RouteStats},
        },
        test_utils::{
            graph_from_test_dataset, line_is_between_point_ids, set_graph_static, test_dataset_1,
        },
    };

    use super::Coords;

    fn test_route_stats() -> RouteStats {
        RouteStats {
            len_m: 1_234.0,
            junction_count: 2,
            highway: HashMap::from([(
                "primary".to_string(),
                RouteStatElement {
                    len_m: 1_234.0,
                    percentage: 100.0,
                },
            )]),
            surface: HashMap::new(),
            smoothness: HashMap::new(),
            score: 5.0,
            cluster: Some(0),
            approximated_route: Vec::new(),
        }
    }

    #[test]
    fn coords_from_str_parses_valid_lat_lon() {
        let coords: Coords = "48.123,11.456".parse().unwrap();

        assert_eq!(coords.lat, 48.123);
        assert_eq!(coords.lon, 11.456);
    }

    #[test]
    fn coords_from_str_rejects_missing_lon() {
        let error = "48.123".parse::<Coords>().unwrap_err();

        assert!(matches!(
            error,
            super::RouterRunnerError::Coords { cause, .. } if cause == "missing"
        ));
    }

    #[test]
    fn router_runner_maps_generated_routes_into_transport_neutral_model() {
        let map_data = set_graph_static(graph_from_test_dataset(test_dataset_1()));
        let point_1 = map_data.test_get_point_ref_by_id(&1).unwrap();
        let point_2 = map_data.test_get_point_ref_by_id(&2).unwrap();
        let point_3 = map_data.test_get_point_ref_by_id(&3).unwrap();

        let line_12 = map_data
            .get_adjacent(point_1.clone())
            .into_iter()
            .find_map(|(line, other_point)| {
                if other_point == point_2 && line_is_between_point_ids(&line, 1, 2) {
                    Some(line)
                } else {
                    None
                }
            })
            .unwrap();

        let line_23 = map_data
            .get_adjacent(point_2.clone())
            .into_iter()
            .find_map(|(line, other_point)| {
                if other_point == point_3 && line_is_between_point_ids(&line, 2, 3) {
                    Some(line)
                } else {
                    None
                }
            })
            .unwrap();

        let mut route = Route::new();
        route.add_segment(Segment::new(line_12, point_2));
        route.add_segment(Segment::new(line_23, point_3));

        let computation = RouteComputation::from(vec![RouteWithStats {
            stats: test_route_stats(),
            route,
        }]);

        assert_eq!(computation.routes.len(), 1);
        assert_eq!(computation.routes[0].coords, vec![(2.0, 2.0), (3.0, 3.0)]);
        assert_eq!(computation.routes[0].stats.len_m, 1_234.0);
    }
}
