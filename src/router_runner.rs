use anyhow::Result;
use std::{num::ParseFloatError, path::PathBuf, str::FromStr};

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    cli::{
        output_dir::{prepare_empty_output_dir, OutputDirError},
        rules::{read_router_rules, RuleFileError},
    },
    result_writer::{OutputFormat, ResultWriter, ResultWriterError, RouteOutputRequest},
    route_output::RouteComputation,
    router::rules::RouterRules,
    routing_api::{
        Coords as RoutingCoords, RouteMode, RouteRequest, RoutingError, RoutingExecutor,
        RoutingExecutorConfig,
    },
    tiles_api::{generate_tiles, TileGenerationError, TileGenerationRequest, TileInputSource},
};

#[derive(Debug, thiserror::Error)]
pub enum RouterRunnerError {
    #[error(transparent)]
    OutputDirectory { error: OutputDirError },

    #[error(transparent)]
    Rules { error: RuleFileError },

    #[error("Coordinate error for {name}: {cause}{}", .error.as_ref().map(|e| format!(": {}", e)).unwrap_or_default())]
    Coords {
        name: String,
        cause: String,
        error: Option<ParseFloatError>,
    },

    #[error("Invalid tile generation arguments: {reason}")]
    InvalidTileGenerationArgs { reason: &'static str },

    #[error(transparent)]
    Routing { error: RoutingError },

    #[error(transparent)]
    TileGeneration { error: TileGenerationError },

    #[error("Failed to write result: {error}")]
    ResultWrite { error: ResultWriterError },
}

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    pub mode: CliMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Coords {
    lat: f32,
    lon: f32,
}

impl From<Coords> for RoutingCoords {
    fn from(value: Coords) -> Self {
        Self {
            lat: value.lat,
            lon: value.lon,
        }
    }
}

impl FromStr for Coords {
    type Err = RouterRunnerError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let mut split = s.split(',');
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

#[derive(Debug, Subcommand)]
enum CliMode {
    /// Load input data and generate a route
    GenerateRoute {
        #[arg(long, value_name = "DIR")]
        /// Tiles directory containing manifest.json and RMDF tiles
        tiles: PathBuf,

        #[arg(long, value_name = "DIR")]
        /// Directory that will receive one output file per route
        output_dir: PathBuf,

        #[arg(long, value_enum)]
        /// Final route output format
        format: OutputFormat,

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
    fn build_route_request(routing_mode: &RoutingMode, rules: RouterRules) -> RouteRequest {
        let mode = match routing_mode {
            RoutingMode::StartFinish { start, finish } => RouteMode::StartFinish {
                start: start.clone().into(),
                finish: finish.clone().into(),
            },
            RoutingMode::RoundTrip {
                start_finish,
                bearing,
                distance,
            } => RouteMode::RoundTrip {
                start_finish: start_finish.clone().into(),
                bearing: *bearing,
                distance: *distance,
            },
        };

        RouteRequest { mode, rules }
    }

    fn build_tile_generation_request(
        input: Option<PathBuf>,
        input_dir: Option<PathBuf>,
        output_dir: PathBuf,
        tile_size_deg: f32,
        db_path: Option<PathBuf>,
    ) -> std::result::Result<TileGenerationRequest, RouterRunnerError> {
        let input = match (input, input_dir) {
            (Some(_), Some(_)) => {
                return Err(RouterRunnerError::InvalidTileGenerationArgs {
                    reason: "cannot use both --input and --input-dir",
                })
            }
            (None, None) => {
                return Err(RouterRunnerError::InvalidTileGenerationArgs {
                    reason: "must provide either --input or --input-dir",
                })
            }
            (Some(file), None) => TileInputSource::File(file),
            (None, Some(directory)) => TileInputSource::Directory(directory),
        };

        Ok(TileGenerationRequest {
            input,
            output_dir,
            tile_size_deg,
            db_path,
        })
    }

    fn write_route_output(
        output_request: RouteOutputRequest,
        computation: RouteComputation,
    ) -> std::result::Result<(), RouterRunnerError> {
        prepare_empty_output_dir(&output_request.output_dir)
            .map_err(|error| RouterRunnerError::OutputDirectory { error })?;

        if computation.routes.is_empty() {
            info!(output_dir = ?output_request.output_dir, "No routes found");
        }

        ResultWriter::write(output_request, computation)
            .map_err(|error| RouterRunnerError::ResultWrite { error })
    }

    fn run_generate_route(
        tiles_dir: PathBuf,
        routing_mode: &RoutingMode,
        output_request: RouteOutputRequest,
        rule_file: Option<PathBuf>,
    ) -> std::result::Result<(), RouterRunnerError> {
        let rules = read_router_rules(rule_file).map_err(|error| RouterRunnerError::Rules { error })?;

        info!(tiles_dir = ?tiles_dir, "Using RMDF tiles");
        let mut executor = RoutingExecutor::open(RoutingExecutorConfig { tiles_dir })
            .map_err(|error| RouterRunnerError::Routing { error })?;

        info!("Route generation started");
        let request = Self::build_route_request(routing_mode, rules);
        let computation = executor
            .generate(request)
            .map_err(|error| RouterRunnerError::Routing { error })?;
        Self::write_route_output(output_request, computation)
    }

    fn run_generate_tiles(
        input: Option<PathBuf>,
        input_dir: Option<PathBuf>,
        output_dir: PathBuf,
        tile_size_deg: f32,
        db_path: Option<PathBuf>,
    ) -> std::result::Result<(), RouterRunnerError> {
        let request = Self::build_tile_generation_request(
            input,
            input_dir,
            output_dir,
            tile_size_deg,
            db_path,
        )?;

        info!(input = ?request.input, output_dir = ?request.output_dir, "Tile generation started");
        let summary = generate_tiles(request).map_err(|error| RouterRunnerError::TileGeneration {
            error,
        })?;
        info!(output_dir = ?summary.output_dir, tile_count = summary.tile_count, "Tile generation complete");

        Ok(())
    }

    #[tracing::instrument]
    pub fn run() -> Result<()> {
        let cli = Cli::parse();
        match cli.mode {
            CliMode::GenerateRoute {
                routing_mode,
                tiles,
                output_dir,
                format,
                rule_file,
            } => Ok(Self::run_generate_route(
                tiles,
                &routing_mode,
                RouteOutputRequest { output_dir, format },
                rule_file,
            )?),
            CliMode::GenerateTiles {
                input,
                input_dir,
                output,
                tile_size_deg,
                db_path,
            } => Ok(Self::run_generate_tiles(input, input_dir, output, tile_size_deg, db_path)?),
            #[cfg(feature = "rule-schema-writer")]
            CliMode::RuleSchemaWrite { destination } => {
                Ok(crate::router::rules::generate_json_schema(&destination)?)
            }
            #[cfg(feature = "rmdf-viewer")]
            CliMode::RmdfViewer { input_dir } => Ok(crate::debug::rmdf_viewer::run(input_dir)?),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        path::PathBuf,
        process,
        time::{SystemTime, UNIX_EPOCH},
    };

    use clap::{error::ErrorKind, Parser};

    use crate::{
        cli::output_dir::{prepare_empty_output_dir, OutputDirError},
        result_writer::{OutputFormat, RouteOutputRequest},
        route_output::RouteComputation,
        router::{
            generator::RouteWithStats,
            route::{segment::Segment, Route, RouteStatElement, RouteStats},
            rules::RouterRules,
        },
        routing_api::{Coords as RoutingCoords, RouteMode},
        test_utils::{
            graph_from_test_dataset, line_is_between_point_ids, set_graph_static, test_dataset_1,
        },
        tiles_api::TileInputSource,
    };

    use super::Coords;

    fn unique_test_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "ridi-router-router-runner-{}-{}",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

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
    fn generate_route_cli_requires_output_dir() {
        let error = super::Cli::try_parse_from([
            "ridi-router",
            "generate-route",
            "--tiles",
            "/tmp/tiles",
            "--format",
            "json",
            "start-finish",
            "--start",
            "48.1,11.5",
            "--finish",
            "48.2,11.6",
        ])
        .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn generate_route_cli_requires_format() {
        let error = super::Cli::try_parse_from([
            "ridi-router",
            "generate-route",
            "--tiles",
            "/tmp/tiles",
            "--output-dir",
            "/tmp/output",
            "start-finish",
            "--start",
            "48.1,11.5",
            "--finish",
            "48.2,11.6",
        ])
        .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn generate_route_cli_rejects_old_output_flag() {
        let error = super::Cli::try_parse_from([
            "ridi-router",
            "generate-route",
            "--tiles",
            "/tmp/tiles",
            "--output",
            "route.json",
            "--output-dir",
            "/tmp/output",
            "--format",
            "json",
            "start-finish",
            "--start",
            "48.1,11.5",
            "--finish",
            "48.2,11.6",
        ])
        .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn route_mode_start_finish_maps_from_cli_inputs() {
        let request = super::RouterRunner::build_route_request(
            &super::RoutingMode::StartFinish {
                start: Coords { lat: 48.1, lon: 11.5 },
                finish: Coords { lat: 48.2, lon: 11.6 },
            },
            RouterRules::default(),
        );

        assert!(matches!(
            request.mode,
            RouteMode::StartFinish {
                start: RoutingCoords { lat: 48.1, lon: 11.5 },
                finish: RoutingCoords { lat: 48.2, lon: 11.6 },
            }
        ));
    }

    #[test]
    fn route_mode_round_trip_maps_from_cli_inputs() {
        let request = super::RouterRunner::build_route_request(
            &super::RoutingMode::RoundTrip {
                start_finish: Coords { lat: 48.1, lon: 11.5 },
                bearing: 90.0,
                distance: 30_000,
            },
            RouterRules::default(),
        );

        assert!(matches!(
            request.mode,
            RouteMode::RoundTrip {
                start_finish: RoutingCoords { lat: 48.1, lon: 11.5 },
                bearing: 90.0,
                distance: 30_000,
            }
        ));
    }

    #[test]
    fn router_runner_builds_route_request_from_cli_args() {
        let rules = RouterRules::default();
        let request = super::RouterRunner::build_route_request(
            &super::RoutingMode::StartFinish {
                start: Coords { lat: 1.0, lon: 2.0 },
                finish: Coords { lat: 3.0, lon: 4.0 },
            },
            rules.clone(),
        );

        match request.mode {
            RouteMode::StartFinish { start, finish } => {
                assert_eq!(start, RoutingCoords { lat: 1.0, lon: 2.0 });
                assert_eq!(finish, RoutingCoords { lat: 3.0, lon: 4.0 });
            }
            _ => panic!("expected start-finish route mode"),
        }
        assert_eq!(request.rules.basic.step_limit.0, rules.basic.step_limit.0);
    }

    #[test]
    fn build_tile_generation_request_maps_file_input() {
        let request = super::RouterRunner::build_tile_generation_request(
            Some(PathBuf::from("/tmp/input.osm.pbf")),
            None,
            PathBuf::from("/tmp/output"),
            1.0,
            None,
        )
        .unwrap();

        assert!(matches!(request.input, TileInputSource::File(_)));
    }

    #[test]
    fn build_tile_generation_request_rejects_missing_input_mode() {
        let error = super::RouterRunner::build_tile_generation_request(
            None,
            None,
            PathBuf::from("/tmp/output"),
            1.0,
            None,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            super::RouterRunnerError::InvalidTileGenerationArgs { .. }
        ));
    }

    #[test]
    fn validate_output_dir_accepts_missing_then_createable_dir() {
        let output_dir = unique_test_dir();

        prepare_empty_output_dir(&output_dir).unwrap();

        assert!(output_dir.is_dir());
        assert_eq!(fs::read_dir(&output_dir).unwrap().count(), 0);
        fs::remove_dir_all(output_dir).unwrap();
    }

    #[test]
    fn validate_output_dir_accepts_existing_empty_dir() {
        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();

        prepare_empty_output_dir(&output_dir).unwrap();

        fs::remove_dir_all(output_dir).unwrap();
    }

    #[test]
    fn validate_output_dir_rejects_non_empty_dir() {
        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();
        fs::write(output_dir.join("already-there.txt"), "x").unwrap();

        let error = prepare_empty_output_dir(&output_dir).unwrap_err();
        fs::remove_dir_all(output_dir).unwrap();

        assert!(matches!(error, OutputDirError::NotEmpty { .. }));
    }

    #[test]
    fn generate_route_zero_routes_is_successful_end_state() {
        let output_dir = unique_test_dir();

        super::RouterRunner::write_route_output(
            RouteOutputRequest {
                output_dir: output_dir.clone(),
                format: OutputFormat::Json,
            },
            RouteComputation { routes: vec![] },
        )
        .unwrap();

        assert!(output_dir.exists());
        assert_eq!(fs::read_dir(&output_dir).unwrap().count(), 0);
        fs::remove_dir_all(output_dir).unwrap();
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

    #[test]
    fn router_runner_source_no_longer_mentions_ipc_types() {
        let source = include_str!("router_runner.rs");
        let transport_module = ["ipc", "_handler::"].concat();
        let transport_error = ["Ipc", "HandlerError"].concat();
        let transport_variant = ["RouterRunnerError::", "Ipc"].concat();

        assert!(!source.contains(&transport_module));
        assert!(!source.contains(&transport_error));
        assert!(!source.contains(&transport_variant));
    }

    #[test]
    fn crate_no_longer_references_dead_ipc_transport() {
        let main_source = include_str!("main.rs");
        let main_module_decl = ["mod ", "ipc", "_handler;"].into_iter().collect::<String>();
        assert!(!main_source.contains(&main_module_decl));

        let cargo_toml =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
        let removed_dependency = ["inter", "process = "].into_iter().collect::<String>();
        assert!(!cargo_toml.contains(&removed_dependency));

        let removed_transport_file = format!(
            "{}/src/{}{}",
            env!("CARGO_MANIFEST_DIR"),
            "ipc_",
            "handler.rs"
        );
        let removed_transport_path = std::path::Path::new(&removed_transport_file);
        assert!(!removed_transport_path.exists());
    }
}
