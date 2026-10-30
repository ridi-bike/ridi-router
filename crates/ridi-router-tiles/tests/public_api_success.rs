use std::{
    fs,
    path::PathBuf,
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use ridi_router_routing::{
    Coords, RouteMode, RouteRequest, RouterRules, RoutingExecutor, RoutingExecutorConfig,
};
use ridi_router_tiles::{
    generate_tiles, TileGenerationError, TileGenerationRequest, TileInputSource,
};

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
        .generate(
            RouteRequest {
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
            },
            None,
        )
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

/// Directory holding the routing fixture plus a corrupt PBF that cannot be parsed.
fn input_dir_with_broken_pbf(test_dir: &TestDir) -> (PathBuf, PathBuf) {
    let input_dir = test_dir.path().join("input");
    fs::create_dir_all(&input_dir).unwrap();
    fs::copy(fixture_pbf(), input_dir.join("a-tiny-routing.osm.pbf")).unwrap();
    let broken_pbf = input_dir.join("b-broken.osm.pbf");
    fs::write(&broken_pbf, b"not-a-real-pbf").unwrap();
    (input_dir, broken_pbf)
}

#[test]
fn generate_tiles_directory_input_fails_on_unreadable_pbf() {
    let test_dir = TestDir::new("broken-pbf");
    let (input_dir, _) = input_dir_with_broken_pbf(&test_dir);

    let error = generate_tiles(TileGenerationRequest {
        input: TileInputSource::Directory(input_dir),
        output_dir: test_dir.path().join("output"),
        tile_size_deg: 180.0,
    })
    .unwrap_err();

    assert!(
        matches!(&error, TileGenerationError::GenerationFailed { message, .. } if message.contains("b-broken.osm.pbf")),
        "unexpected error: {error}"
    );
}

#[test]
fn generate_tiles_directory_input_lists_every_source_file() {
    let test_dir = TestDir::new("two-copies");
    let input_dir = test_dir.path().join("input");
    fs::create_dir_all(&input_dir).unwrap();
    // the same extract twice: every object is present in both files and must be kept once
    fs::copy(fixture_pbf(), input_dir.join("a.osm.pbf")).unwrap();
    fs::copy(fixture_pbf(), input_dir.join("b.osm.pbf")).unwrap();
    let single_dir = test_dir.path().join("single");
    let dir_out = test_dir.path().join("dir");

    let single = generate_tiles(TileGenerationRequest {
        input: TileInputSource::File(fixture_pbf()),
        output_dir: single_dir.clone(),
        tile_size_deg: 0.1,
    })
    .unwrap();
    let both = generate_tiles(TileGenerationRequest {
        input: TileInputSource::Directory(input_dir),
        output_dir: dir_out.clone(),
        tile_size_deg: 0.1,
    })
    .unwrap();

    assert_eq!(single.tile_count, both.tile_count);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(dir_out.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["source_files"],
        serde_json::json!(["a.osm.pbf", "b.osm.pbf"])
    );
    for tile in manifest["tiles"].as_array().unwrap() {
        let name = tile["filename"].as_str().unwrap();
        assert_eq!(
            fs::read(single_dir.join(name)).unwrap().len(),
            fs::read(dir_out.join(name)).unwrap().len(),
            "tile {name} differs in size between single-file and duplicated-directory input"
        );
    }
}
