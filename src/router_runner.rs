use anyhow::{Context, Result};
use std::{
    fs, io,
    num::ParseFloatError,
    path::{Path, PathBuf},
    str::FromStr,
};

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tracing::{info, trace};

use crate::router::generator::{GeneratorError, WP_LOOKUP_ALLOWED_HWS};
use crate::{
    map_data::graph::MapDataGraph,
    result_writer::{OutputFormat, ResultWriter, ResultWriterError, RouteOutputRequest},
    route_output::RouteComputation,
    router::{
        generator::{Generator, RouteWithStats},
        rules::RouterRules,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum RouterRunnerError {

    #[error("Failed to create output directory '{directory:?}': {error}")]
    OutputDirectoryCreate {
        directory: PathBuf,
        error: io::Error,
    },

    #[error("Output directory invalid '{directory:?}': {reason}")]
    OutputDirectoryInvalid { directory: PathBuf, reason: String },

    #[error("Output directory must be empty '{directory:?}'")]
    OutputDirectoryNotEmpty { directory: PathBuf },

    #[error("Coordinate error for {name}: {cause}{}", .error.as_ref().map(|e| format!(": {}", e)).unwrap_or_default())]
    Coords {
        name: String,
        cause: String,
        error: Option<ParseFloatError>,
    },

    #[error("Could not find {point} on map")]
    PointNotFound { point: String },

    #[error("Failed to write result: {error}")]
    ResultWrite { error: ResultWriterError },

    #[error("Failed to generate routes: {error}")]
    GenerateRoute { error: GeneratorError },
}

#[derive(Debug, Parser)]
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

    fn prepare_output_dir(output_dir: &Path) -> Result<(), RouterRunnerError> {
        if output_dir.exists() {
            let metadata = fs::metadata(output_dir).map_err(|error| {
                RouterRunnerError::OutputDirectoryInvalid {
                    directory: output_dir.to_path_buf(),
                    reason: format!("failed to read metadata: {error}"),
                }
            })?;

            if !metadata.is_dir() {
                return Err(RouterRunnerError::OutputDirectoryInvalid {
                    directory: output_dir.to_path_buf(),
                    reason: "path exists but is not a directory".to_string(),
                });
            }

            let mut entries = fs::read_dir(output_dir).map_err(|error| {
                RouterRunnerError::OutputDirectoryInvalid {
                    directory: output_dir.to_path_buf(),
                    reason: format!("failed to read directory: {error}"),
                }
            })?;

            if entries
                .next()
                .transpose()
                .map_err(|error| RouterRunnerError::OutputDirectoryInvalid {
                    directory: output_dir.to_path_buf(),
                    reason: format!("failed while reading directory contents: {error}"),
                })?
                .is_some()
            {
                return Err(RouterRunnerError::OutputDirectoryNotEmpty {
                    directory: output_dir.to_path_buf(),
                });
            }

            return Ok(());
        }

        fs::create_dir_all(output_dir).map_err(|error| RouterRunnerError::OutputDirectoryCreate {
            directory: output_dir.to_path_buf(),
            error,
        })
    }

    fn write_route_output(
        output_request: RouteOutputRequest,
        computation: RouteComputation,
    ) -> Result<(), RouterRunnerError> {
        Self::prepare_output_dir(&output_request.output_dir)?;

        if computation.routes.is_empty() {
            info!(output_dir = ?output_request.output_dir, "No routes found");
        }

        ResultWriter::write(output_request, computation)
            .map_err(|error| RouterRunnerError::ResultWrite { error })
    }

    #[tracing::instrument(skip_all)]
    fn run_generate_route(
        tiles_dir: PathBuf,
        routing_mode: &RoutingMode,
        output_request: RouteOutputRequest,
        rule_file: Option<PathBuf>,
    ) -> Result<()> {
        Self::prepare_output_dir(&output_request.output_dir)?;

        let rules = RouterRules::read(rule_file).context("Failed to read rules")?;

        info!("Using RMDF tiles from {:?}", tiles_dir);

        MapDataGraph::init(tiles_dir);

        info!("Route generation started");

        let computation = RouterRunner::generate_route(routing_mode, rules).map(RouteComputation::from)?;
        RouterRunner::write_route_output(output_request, computation)?;
        Ok(())
    }

    #[tracing::instrument]
    pub fn run() -> Result<()> {
        let cli = Cli::parse();
        match &cli.mode {
            CliMode::GenerateRoute {
                routing_mode,
                tiles,
                output_dir,
                format,
                rule_file,
            } => RouterRunner::run_generate_route(
                tiles.clone(),
                routing_mode,
                RouteOutputRequest {
                    output_dir: output_dir.clone(),
                    format: *format,
                },
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
    use std::{
        collections::HashMap,
        fs,
        path::PathBuf,
        process,
        time::{SystemTime, UNIX_EPOCH},
    };

    use clap::{error::ErrorKind, Parser};

    use crate::{
        result_writer::{OutputFormat, RouteOutputRequest},
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
    fn validate_output_dir_accepts_missing_then_createable_dir() {
        let output_dir = unique_test_dir();

        super::RouterRunner::prepare_output_dir(&output_dir).unwrap();

        assert!(output_dir.is_dir());
        assert_eq!(fs::read_dir(&output_dir).unwrap().count(), 0);
        fs::remove_dir_all(output_dir).unwrap();
    }

    #[test]
    fn validate_output_dir_accepts_existing_empty_dir() {
        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();

        super::RouterRunner::prepare_output_dir(&output_dir).unwrap();

        fs::remove_dir_all(output_dir).unwrap();
    }

    #[test]
    fn validate_output_dir_rejects_non_empty_dir() {
        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();
        fs::write(output_dir.join("already-there.txt"), "x").unwrap();

        let error = super::RouterRunner::prepare_output_dir(&output_dir).unwrap_err();
        fs::remove_dir_all(output_dir).unwrap();

        assert!(matches!(
            error,
            super::RouterRunnerError::OutputDirectoryNotEmpty { .. }
        ));
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
