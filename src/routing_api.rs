use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::OnceLock,
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
};

static OPEN_TILES_DIR: OnceLock<PathBuf> = OnceLock::new();

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Coords {
    pub lat: f32,
    pub lon: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RouteMode {
    StartFinish { start: Coords, finish: Coords },
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

#[derive(Debug, Clone)]
pub struct RoutingExecutor {
    tiles_dir: PathBuf,
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

    #[error(
        "Routing graph already opened for '{existing_tiles_dir:?}', cannot reopen with '{requested_tiles_dir:?}'"
    )]
    ConflictingTilesDir {
        existing_tiles_dir: PathBuf,
        requested_tiles_dir: PathBuf,
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
    pub fn open(config: RoutingExecutorConfig) -> Result<Self, RoutingError> {
        let requested_tiles_dir = normalize_tiles_dir(&config.tiles_dir);

        if let Some(existing_tiles_dir) = OPEN_TILES_DIR.get() {
            if existing_tiles_dir != &requested_tiles_dir {
                return Err(RoutingOpenError::ConflictingTilesDir {
                    existing_tiles_dir: existing_tiles_dir.clone(),
                    requested_tiles_dir,
                }
                .into());
            }

            return Ok(Self {
                tiles_dir: existing_tiles_dir.clone(),
            });
        }

        validate_tiles_manifest(&config.tiles_dir)?;
        MapDataGraph::init(config.tiles_dir);
        let _ = OPEN_TILES_DIR.set(requested_tiles_dir.clone());

        Ok(Self {
            tiles_dir: requested_tiles_dir,
        })
    }

    pub fn generate(&mut self, request: RouteRequest) -> Result<RouteComputation, RoutingError> {
        trace!(tiles_dir = ?self.tiles_dir, "Generating route");

        let (start_coords, finish_coords, round_trip) = match request.mode {
            RouteMode::StartFinish { start, finish } => (start, finish, None),
            RouteMode::RoundTrip {
                start_finish,
                bearing,
                distance,
            } => (start_finish, start_finish, Some((bearing, distance))),
        };

        let start = MapDataGraph::get()
            .get_closest_to_coords(
                start_coords.lat,
                start_coords.lon,
                &request.rules,
                false,
                Some(&WP_LOOKUP_ALLOWED_HWS),
            )
            .ok_or(RoutingGenerationError::PointNotFound { point: "start point" })?;

        let finish = MapDataGraph::get()
            .get_closest_to_coords(
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
            .generate_routes()
            .map_err(|error| RoutingGenerationError::RouteGeneration { error })?;

        Ok(RouteComputation::from(routes))
    }
}

fn normalize_tiles_dir(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn validate_tiles_manifest(tiles_dir: &Path) -> Result<TileManifest, RoutingOpenError> {
    let manifest_path = tiles_dir.join("manifest.json");
    let file = std::fs::File::open(&manifest_path).map_err(|error| RoutingOpenError::ManifestRead {
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
    use std::{
        fs,
        path::{Path, PathBuf},
        process,
        sync::OnceLock,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::router::rules::RouterRules;
    use rusty_fork::rusty_fork_test;

    use super::{Coords, RouteMode, RouteRequest, RoutingError, RoutingExecutor, RoutingExecutorConfig};

    const RMDF_HEADER_SIZE: u64 = 112;
    const POINT_RECORD_SIZE: u64 = 48;
    const LINE_RECORD_SIZE: u64 = 40;
    const TILE_FILENAME: &str = "tile_200_100.rmdf";

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct TileBoundsRecord {
        lat_min: f32,
        lat_max: f32,
        lon_min: f32,
        lon_max: f32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct PointRecord {
        osm_id: u64,
        lat: f32,
        lon: f32,
        lines_offset: u64,
        lines_count: u32,
        padding1: u32,
        rules_offset: u64,
        rules_count: u32,
        flags: u16,
        padding2: u16,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct LineRecord {
        point_a_osm_id: u64,
        point_a_lat: f32,
        point_a_lon: f32,
        point_b_osm_id: u64,
        point_b_lat: f32,
        point_b_lon: f32,
        direction: u8,
        padding1: u8,
        padding2: u16,
        tag_set_index: u32,
    }

    static SYNTHETIC_TILES_DIR: OnceLock<PathBuf> = OnceLock::new();

    fn synthetic_tiles_dir() -> PathBuf {
        SYNTHETIC_TILES_DIR
            .get_or_init(create_synthetic_tiles_fixture)
            .clone()
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ridi-router-routing-api-{prefix}-{}-{}",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn push_u16(buf: &mut Vec<u8>, value: u16) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(buf: &mut Vec<u8>, value: u32) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u64(buf: &mut Vec<u8>, value: u64) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn push_f32(buf: &mut Vec<u8>, value: f32) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn write_tile_bounds(buf: &mut Vec<u8>, bounds: TileBoundsRecord) {
        push_f32(buf, bounds.lat_min);
        push_f32(buf, bounds.lat_max);
        push_f32(buf, bounds.lon_min);
        push_f32(buf, bounds.lon_max);
    }

    fn write_point_record(buf: &mut Vec<u8>, point: PointRecord) {
        push_u64(buf, point.osm_id);
        push_f32(buf, point.lat);
        push_f32(buf, point.lon);
        push_u64(buf, point.lines_offset);
        push_u32(buf, point.lines_count);
        push_u32(buf, point.padding1);
        push_u64(buf, point.rules_offset);
        push_u32(buf, point.rules_count);
        push_u16(buf, point.flags);
        push_u16(buf, point.padding2);
    }

    fn write_line_record(buf: &mut Vec<u8>, line: LineRecord) {
        push_u64(buf, line.point_a_osm_id);
        push_f32(buf, line.point_a_lat);
        push_f32(buf, line.point_a_lon);
        push_u64(buf, line.point_b_osm_id);
        push_f32(buf, line.point_b_lat);
        push_f32(buf, line.point_b_lon);
        buf.push(line.direction);
        buf.push(line.padding1);
        push_u16(buf, line.padding2);
        push_u32(buf, line.tag_set_index);
    }

    fn create_synthetic_tiles_fixture() -> PathBuf {
        let fixture_dir = unique_test_dir("tiles-fixture");
        fs::create_dir_all(&fixture_dir).unwrap();

        let mut points = Vec::new();
        let mut line_refs = Vec::new();
        let mut lines = Vec::new();

        for idx in 0..=12_u64 {
            let lat = 10.0 + idx as f32 * 0.01;
            let mut refs = Vec::new();
            if idx > 0 {
                refs.push(idx - 1);
            }
            if idx < 12 {
                refs.push(idx);
            }

            points.push(PointRecord {
                osm_id: 1000 + idx,
                lat,
                lon: 20.0,
                lines_offset: line_refs.len() as u64,
                lines_count: refs.len() as u32,
                padding1: 0,
                rules_offset: 0,
                rules_count: 0,
                flags: 0,
                padding2: 0,
            });
            line_refs.extend(refs);
        }

        for idx in 0..12_u64 {
            let point_a_lat = 10.0 + idx as f32 * 0.01;
            let point_b_lat = 10.0 + (idx + 1) as f32 * 0.01;
            lines.push(LineRecord {
                point_a_osm_id: 1000 + idx,
                point_a_lat,
                point_a_lon: 20.0,
                point_b_osm_id: 1001 + idx,
                point_b_lat,
                point_b_lon: 20.0,
                direction: 0,
                padding1: 0,
                padding2: 0,
                tag_set_index: 0,
            });
        }

        let points_offset = RMDF_HEADER_SIZE;
        let lines_offset = points_offset + POINT_RECORD_SIZE * points.len() as u64;
        let line_refs_offset = lines_offset + LINE_RECORD_SIZE * lines.len() as u64;
        let tag_values_offset = line_refs_offset + 8 * line_refs.len() as u64;

        let mut tile_bytes = Vec::new();
        tile_bytes.extend_from_slice(b"RMDF");
        push_u32(&mut tile_bytes, 1);
        write_tile_bounds(
            &mut tile_bytes,
            TileBoundsRecord {
                lat_min: 10.0,
                lat_max: 11.0,
                lon_min: 20.0,
                lon_max: 21.0,
            },
        );
        push_u64(&mut tile_bytes, points.len() as u64);
        push_u64(&mut tile_bytes, lines.len() as u64);
        push_u32(&mut tile_bytes, 0);
        push_u32(&mut tile_bytes, 0);
        push_u32(&mut tile_bytes, 0);
        push_u32(&mut tile_bytes, 0);
        for offset in [
            RMDF_HEADER_SIZE,
            points_offset,
            lines_offset,
            line_refs_offset,
            tag_values_offset,
            tag_values_offset,
            tag_values_offset,
        ] {
            push_u64(&mut tile_bytes, offset);
        }

        assert_eq!(tile_bytes.len() as u64, RMDF_HEADER_SIZE);

        for point in points {
            write_point_record(&mut tile_bytes, point);
        }
        for line in lines {
            write_line_record(&mut tile_bytes, line);
        }
        for line_ref in line_refs {
            push_u64(&mut tile_bytes, line_ref);
        }

        fs::write(fixture_dir.join(TILE_FILENAME), tile_bytes).unwrap();

        write_manifest(&fixture_dir, TILE_FILENAME);
        fixture_dir
    }

    fn write_manifest(dir: &Path, tile_filename: &str) {
        let manifest = serde_json::json!({
            "version": "test",
            "tile_size_degrees": 1.0,
            "format_version": 1,
            "generated_at": "2026-04-03T00:00:00Z",
            "source_files": ["synthetic"],
            "tiles": [{
                "filename": tile_filename,
                "col": 200,
                "row": 100,
                "bounds": {
                    "lat_min": 10.0,
                    "lat_max": 11.0,
                    "lon_min": 20.0,
                    "lon_max": 21.0
                },
                "neighbors": {
                    "north": null,
                    "south": null,
                    "east": null,
                    "west": null,
                    "northeast": null,
                    "northwest": null,
                    "southeast": null,
                    "southwest": null
                },
                "size_bytes": fs::metadata(dir.join(tile_filename)).unwrap().len(),
                "point_count": 13,
                "line_count": 12,
                "checksum": "sha256:test"
            }]
        });

        fs::write(dir.join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
    }

    #[test]
    fn routing_executor_open_rejects_conflicting_tiles_dir() {
        let existing_tiles_dir = synthetic_tiles_dir();
        let requested_tiles_dir = unique_test_dir("conflicting-open");
        fs::create_dir_all(&requested_tiles_dir).unwrap();

        RoutingExecutor::open(RoutingExecutorConfig {
            tiles_dir: existing_tiles_dir.clone(),
        })
        .unwrap();

        let error = RoutingExecutor::open(RoutingExecutorConfig {
            tiles_dir: requested_tiles_dir.clone(),
        })
        .unwrap_err();

        assert!(matches!(
            error,
            RoutingError::Open(super::RoutingOpenError::ConflictingTilesDir {
                existing_tiles_dir: actual_existing,
                requested_tiles_dir: actual_requested,
            }) if actual_existing == existing_tiles_dir.canonicalize().unwrap_or(existing_tiles_dir)
                && actual_requested == requested_tiles_dir
        ));

        fs::remove_dir_all(requested_tiles_dir).unwrap();
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

    rusty_fork_test! {
        #[test]
        fn routing_executor_generate_returns_route_computation() {
            let tiles_dir = synthetic_tiles_dir();
            let mut executor = RoutingExecutor::open(RoutingExecutorConfig { tiles_dir }).unwrap();

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

    fn synthetic_rules() -> RouterRules {
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
