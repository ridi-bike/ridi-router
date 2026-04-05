use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use tracing::trace;

use crate::{
    map_data::graph::MapDataGraph,
    rmdf::generator::manifest::TileManifest,
    route_output::RouteComputation,
    router::{
        generator::{Generator, GeneratorError, WP_LOOKUP_ALLOWED_HWS},
        rules::RouterRules,
    },
    RoutingContext,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Coords {
    pub lat: f32,
    pub lon: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RouteMode {
    StartFinish {
        start: Coords,
        finish: Coords,
    },
    RoundTrip {
        start_finish: Coords,
        bearing: f32,
        distance: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRequest {
    pub mode: RouteMode,
    pub rules: RouterRules,
}

#[derive(Debug, Clone)]
pub struct RoutingExecutorConfig {
    pub tiles_dir: PathBuf,
}

#[derive(Clone)]
pub struct RoutingExecutor {
    graph: Arc<MapDataGraph>,
}

#[derive(Debug, thiserror::Error)]
pub enum RoutingError {
    #[error(transparent)]
    Open(#[from] RoutingOpenError),

    #[error(transparent)]
    Generate(#[from] RoutingGenerationError),
}

#[derive(Debug, thiserror::Error)]
pub enum RoutingOpenError {
    #[error("Failed to initialize TileManager from '{manifest_path:?}': {error}")]
    ManifestRead {
        manifest_path: PathBuf,
        error: io::Error,
    },

    #[error("Failed to initialize TileManager from '{manifest_path:?}': {error}")]
    ManifestParse {
        manifest_path: PathBuf,
        error: serde_json::Error,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum RoutingGenerationError {
    #[error("Could not find {point} on map")]
    PointNotFound { point: &'static str },

    #[error("Failed to generate routes: {error}")]
    RouteGeneration { error: GeneratorError },
}

impl RoutingExecutor {
    pub(crate) fn new(graph: Arc<MapDataGraph>) -> Self {
        Self { graph }
    }

    pub fn open(config: RoutingExecutorConfig) -> Result<Self, RoutingError> {
        let manifest = validate_tiles_manifest(&config.tiles_dir)?;
        let graph = Arc::new(MapDataGraph::open(config.tiles_dir, manifest));

        Ok(Self::new(graph))
    }

    pub fn generate(&self, request: RouteRequest) -> Result<RouteComputation, RoutingError> {
        trace!("Generating route");

        let (start_coords, finish_coords, round_trip) = match request.mode {
            RouteMode::StartFinish { start, finish } => (start, finish, None),
            RouteMode::RoundTrip {
                start_finish,
                bearing,
                distance,
            } => (start_finish, start_finish, Some((bearing, distance))),
        };

        let ctx = RoutingContext::new(self.graph.as_ref());

        let start = ctx
            .closest_to_coords(
                start_coords.lat,
                start_coords.lon,
                &request.rules,
                false,
                Some(&WP_LOOKUP_ALLOWED_HWS),
            )
            .ok_or(RoutingGenerationError::PointNotFound {
                point: "start point",
            })?;

        let finish = ctx
            .closest_to_coords(
                finish_coords.lat,
                finish_coords.lon,
                &request.rules,
                false,
                Some(&WP_LOOKUP_ALLOWED_HWS),
            )
            .ok_or(RoutingGenerationError::PointNotFound {
                point: "finish point",
            })?;

        let routes = Generator::new(start, finish, round_trip, request.rules)
            .generate_routes(&ctx)
            .map_err(|error| RoutingGenerationError::RouteGeneration { error })?;

        Ok(RouteComputation::from_routes(&ctx, routes))
    }
}

fn validate_tiles_manifest(tiles_dir: &Path) -> Result<TileManifest, RoutingOpenError> {
    let manifest_path = tiles_dir.join("manifest.json");
    let file =
        std::fs::File::open(&manifest_path).map_err(|error| RoutingOpenError::ManifestRead {
            manifest_path: manifest_path.clone(),
            error,
        })?;

    serde_json::from_reader(file).map_err(|error| RoutingOpenError::ManifestParse {
        manifest_path,
        error,
    })
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, sync::OnceLock};

    use crate::{map_data::graph::MapDataGraph, router::rules::RouterRules, RoutingContext};
    use ridi_router_test_support::rmdf::create_linear_single_tile_fixture;
    use rusty_fork::rusty_fork_test;

    use super::{
        validate_tiles_manifest, Coords, RouteMode, RouteRequest, RoutingExecutor,
        RoutingExecutorConfig, WP_LOOKUP_ALLOWED_HWS,
    };

    static SYNTHETIC_TILES_DIR: OnceLock<PathBuf> = OnceLock::new();

    pub(super) fn synthetic_tiles_dir() -> PathBuf {
        SYNTHETIC_TILES_DIR
            .get_or_init(|| create_linear_single_tile_fixture("routing-api-tiles-fixture").dir)
            .clone()
    }

    #[test]
    fn routing_executor_open_allows_multiple_datasets_in_one_process() {
        let first_tiles_dir = synthetic_tiles_dir();
        let second_tiles_dir =
            create_linear_single_tile_fixture("routing-api-tiles-fixture-second").dir;

        let first = RoutingExecutor::open(RoutingExecutorConfig {
            tiles_dir: first_tiles_dir.clone(),
        })
        .unwrap();
        let second = RoutingExecutor::open(RoutingExecutorConfig {
            tiles_dir: second_tiles_dir.clone(),
        })
        .unwrap();

        assert_ne!(first_tiles_dir, second_tiles_dir);
        let _ = (first, second);

        fs::remove_dir_all(second_tiles_dir).unwrap();
    }

    #[test]
    fn routing_executor_open_allows_reopening_same_tiles_dir() {
        let tiles_dir = synthetic_tiles_dir();

        RoutingExecutor::open(RoutingExecutorConfig {
            tiles_dir: tiles_dir.clone(),
        })
        .unwrap();

        RoutingExecutor::open(RoutingExecutorConfig { tiles_dir }).unwrap();
    }

    #[test]
    fn synthetic_fixture_points_are_snappable_with_wp_lookup_allowed_highways() {
        let tiles_dir = synthetic_tiles_dir();
        let manifest = validate_tiles_manifest(&tiles_dir).unwrap();
        let graph = MapDataGraph::open(tiles_dir, manifest);
        let ctx = RoutingContext::new(&graph);
        let rules = synthetic_rules();

        let start = ctx.closest_to_coords(10.0, 20.0, &rules, false, Some(&WP_LOOKUP_ALLOWED_HWS));
        let finish =
            ctx.closest_to_coords(10.12, 20.0, &rules, false, Some(&WP_LOOKUP_ALLOWED_HWS));

        assert_eq!(start.unwrap().get_element_id(), 1000);
        assert_eq!(finish.unwrap().get_element_id(), 1012);
    }

    rusty_fork_test! {
        #[test]
        fn routing_executor_generate_returns_route_computation() {
            let tiles_dir = synthetic_tiles_dir();
            let executor = RoutingExecutor::open(RoutingExecutorConfig { tiles_dir }).unwrap();

            let computation = executor
                .generate(RouteRequest {
                    mode: RouteMode::StartFinish {
                        start: Coords {
                            lat: 10.0,
                            lon: 20.0,
                        },
                        finish: Coords {
                            lat: 10.12,
                            lon: 20.0,
                        },
                    },
                    rules: synthetic_rules(),
                })
                .unwrap();

            assert_eq!(computation.routes.len(), 1);
            assert!(!computation.routes[0].coords.is_empty());
        }
    }

    pub(super) fn synthetic_rules() -> RouterRules {
        serde_json::from_value(serde_json::json!({
            "basic": { "step_limit": 50 },
            "generation": {
                "waypoint_generation": {
                    "start_finish": {
                        "variation_distances_m": [],
                        "variation_bearing_deg": []
                    }
                },
                "route_generation_retry": {
                    "trigger_min_route_count": 1,
                    "round_trip_adjustment_bearing_deg": [],
                    "avoid_residential": [false]
                }
            },
            "highway": null,
            "surface": null,
            "smoothness": null
        }))
        .unwrap()
    }
}

#[cfg(test)]
mod routing_executor_tests {
    use rusty_fork::rusty_fork_test;

    use super::{
        tests::synthetic_rules, tests::synthetic_tiles_dir, Coords, RouteMode, RouteRequest,
        RoutingExecutor, RoutingExecutorConfig,
    };

    #[test]
    fn routing_executor_open_reads_tiles_dir_manifest() {
        let tiles_dir = synthetic_tiles_dir();
        RoutingExecutor::open(RoutingExecutorConfig { tiles_dir }).unwrap();
    }

    rusty_fork_test! {
        #[test]
        fn routing_executor_generate_start_finish_returns_route_computation() {
            let tiles_dir = synthetic_tiles_dir();
            let executor = RoutingExecutor::open(RoutingExecutorConfig { tiles_dir }).unwrap();

            let computation = executor
                .generate(RouteRequest {
                    mode: RouteMode::StartFinish {
                        start: Coords { lat: 10.0, lon: 20.0 },
                        finish: Coords { lat: 10.12, lon: 20.0 },
                    },
                    rules: synthetic_rules(),
                })
                .unwrap();

            assert_eq!(computation.routes.len(), 1);
            assert!(!computation.routes[0].coords.is_empty());
        }

        #[test]
        fn routing_executor_generate_round_trip_returns_route_computation() {
            let tiles_dir = synthetic_tiles_dir();
            let executor = RoutingExecutor::open(RoutingExecutorConfig { tiles_dir }).unwrap();

            let computation = executor
                .generate(RouteRequest {
                    mode: RouteMode::RoundTrip {
                        start_finish: Coords { lat: 10.06, lon: 20.0 },
                        bearing: 90.0,
                        distance: 2_000,
                    },
                    rules: synthetic_rules(),
                })
                .unwrap();

            let _as_route_computation = computation;
        }

        #[test]
        fn routing_executor_reuse_is_sequential_and_supported() {
            let tiles_dir = synthetic_tiles_dir();
            let executor = RoutingExecutor::open(RoutingExecutorConfig { tiles_dir }).unwrap();

            let first = executor
                .generate(RouteRequest {
                    mode: RouteMode::StartFinish {
                        start: Coords { lat: 10.0, lon: 20.0 },
                        finish: Coords { lat: 10.12, lon: 20.0 },
                    },
                    rules: synthetic_rules(),
                })
                .unwrap();
            let second = executor
                .generate(RouteRequest {
                    mode: RouteMode::StartFinish {
                        start: Coords { lat: 10.01, lon: 20.0 },
                        finish: Coords { lat: 10.11, lon: 20.0 },
                    },
                    rules: synthetic_rules(),
                })
                .unwrap();

            assert_eq!(first.routes.len(), 1);
            assert_eq!(second.routes.len(), 1);
        }
    }
}
