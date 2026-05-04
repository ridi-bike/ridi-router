use std::{
    fs,
    path::PathBuf,
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use ridi_router_routing::{
    Coords, RouteMode, RouteRequest, RouterRules, RoutingExecutor, RoutingExecutorConfig,
};
use ridi_router_tiles::{generate_tiles, TileGenerationRequest, TileInputSource};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(prefix: &str) -> Self {
        Self {
            path: std::env::temp_dir().join(format!(
                "ridi-router-tiles-public-api-{prefix}-{}-{}",
                process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )),
        }
    }

    fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn fixture_pbf() -> PathBuf {
    workspace_root().join("crates/ridi-router-cli/tests/fixtures/tiny-routing.osm.pbf")
}

#[test]
fn generate_tiles_public_api_writes_manifest_and_supports_routing() {
    let input_file = fixture_pbf();
    assert!(input_file.exists(), "missing fixture: {input_file:?}");

    let output_dir = TestDir::new("tiny-routing-single-file");
    let summary = generate_tiles(TileGenerationRequest {
        input: TileInputSource::File(input_file),
        output_dir: output_dir.path().clone(),
        tile_size_deg: 180.0,
        db_path: None,
    })
    .unwrap();

    assert_eq!(summary.output_dir, *output_dir.path());
    assert!(summary.tile_count > 0);

    let manifest_path = output_dir.path().join("manifest.json");
    assert!(
        manifest_path.exists(),
        "missing manifest: {manifest_path:?}"
    );

    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    let tiles = manifest["tiles"].as_array().unwrap();
    assert_eq!(tiles.len(), summary.tile_count);

    let first_tile = tiles[0]["filename"].as_str().unwrap();
    assert!(output_dir.path().join(first_tile).exists());

    let executor = RoutingExecutor::open(RoutingExecutorConfig {
        tiles_dir: summary.output_dir.clone(),
    })
    .unwrap();
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
            rules: RouterRules::default(),
        })
        .unwrap();

    assert!(
        computation.routes.is_empty()
            || computation
                .routes
                .iter()
                .all(|route| !route.coords.is_empty()),
        "routing returned malformed routes: {:?}",
        computation.routes
    );
}
