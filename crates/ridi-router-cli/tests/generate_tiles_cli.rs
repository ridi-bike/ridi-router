use std::{
    fs,
    path::{Path, PathBuf},
    process::{self, Command, Output},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(prefix: &str) -> Self {
        Self {
            path: unique_test_path(prefix),
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn unique_test_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "ridi-router-{prefix}-{}-{}",
        process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn repo_path(relative: &str) -> PathBuf {
    workspace_root().join(relative)
}

fn heavy_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

fn tiny_routing_pbf() -> PathBuf {
    repo_path("crates/ridi-router-cli/tests/fixtures/tiny-routing.osm.pbf")
}

fn fixture_rule_file() -> PathBuf {
    repo_path("rule-examples/rules-default.json")
}

fn run_generate_tiles(command_args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ridi-router-cli"))
        .arg("generate-tiles")
        .args(command_args)
        .output()
        .expect("failed to execute ridi-router-cli generate-tiles")
}

fn run_generate_route(command_args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ridi-router-cli"))
        .arg("generate-route")
        .args(command_args)
        .output()
        .expect("failed to execute ridi-router-cli generate-route")
}

fn assert_generated_tiles(output_dir: &Path) {
    let manifest_path = output_dir.join("manifest.json");
    assert!(
        manifest_path.exists(),
        "missing manifest: {manifest_path:?}"
    );

    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    let tiles = manifest["tiles"].as_array().unwrap();
    assert!(!tiles.is_empty(), "manifest should list at least one tile");

    let tile_files = fs::read_dir(output_dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("rmdf"))
        .collect::<Vec<_>>();

    assert!(!tile_files.is_empty(), "expected at least one RMDF tile");
    assert_eq!(
        tile_files.len(),
        tiles.len(),
        "manifest tile count should match written tile files"
    );
}

fn link_or_copy_fixture(src: &Path, dst: &Path) {
    if fs::hard_link(src, dst).is_err() {
        fs::copy(src, dst).unwrap();
    }
}

#[test]
fn generate_tiles_cli_renders_typed_tile_generation_error() {
    let input_file = unique_test_path("missing-input.osm.pbf");
    let output_dir = TestDir::new("tiles-output");

    let output = run_generate_tiles(&[
        "--input",
        input_file.to_str().unwrap(),
        "--output",
        output_dir.path().to_str().unwrap(),
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        stderr.contains("Tile generation error:"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("missing-input.osm.pbf"), "stderr: {stderr}");
}

#[test]
fn generate_tiles_cli_requires_input_or_input_dir() {
    let output_dir = TestDir::new("tiles-output-missing-input");

    let output = run_generate_tiles(&["--output", output_dir.path().to_str().unwrap()]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        stderr.contains(
            "Tile generation argument error: Invalid tile generation arguments: must provide either --input or --input-dir",
        ),
        "stderr: {stderr}"
    );
}

#[test]
fn generate_tiles_cli_single_file_writes_manifest_and_tiles() {
    let input_file = tiny_routing_pbf();
    if !input_file.exists() {
        eprintln!("Skipping test: missing fixture {input_file:?}");
        return;
    }

    let _heavy_test_lock = heavy_test_lock();
    let output_dir = TestDir::new("tiles-cli-single-file-success");
    let output = run_generate_tiles(&[
        "--input",
        input_file.to_str().unwrap(),
        "--output",
        output_dir.path().to_str().unwrap(),
        "--tile-size-deg",
        "180",
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(output.stdout.is_empty());
    assert_generated_tiles(output_dir.path());
}

#[test]
fn generate_tiles_directory_input_then_generate_route_end_to_end() {
    let fixture_pbf = tiny_routing_pbf();
    if !fixture_pbf.exists() {
        eprintln!("Skipping test: missing fixture {fixture_pbf:?}");
        return;
    }

    let _heavy_test_lock = heavy_test_lock();
    let input_dir = TestDir::new("tiles-cli-directory-input");
    fs::create_dir_all(input_dir.path()).unwrap();
    let linked_pbf = input_dir.path().join("tiny-routing.osm.pbf");
    link_or_copy_fixture(&fixture_pbf, &linked_pbf);

    let tiles_dir = TestDir::new("tiles-cli-directory-output");
    let db_path = unique_test_path("tiles-cli-directory-output.redb");
    let generate_output = run_generate_tiles(&[
        "--input-dir",
        input_dir.path().to_str().unwrap(),
        "--output",
        tiles_dir.path().to_str().unwrap(),
        "--tile-size-deg",
        "180",
        "--db-path",
        db_path.to_str().unwrap(),
    ]);

    let generate_stderr = String::from_utf8_lossy(&generate_output.stderr);
    assert!(
        generate_output.status.success(),
        "stderr: {generate_stderr}"
    );
    assert!(generate_output.stdout.is_empty());
    assert_generated_tiles(tiles_dir.path());

    let route_output_dir = TestDir::new("tiles-cli-route-output");
    let route_output = run_generate_route(&[
        "--tiles",
        tiles_dir.path().to_str().unwrap(),
        "--output-dir",
        route_output_dir.path().to_str().unwrap(),
        "--format",
        "json",
        "--rule-file",
        fixture_rule_file().to_str().unwrap(),
        "start-finish",
        "--start",
        "10.0,20.0",
        "--finish",
        "10.12,20.0",
    ]);

    let route_stderr = String::from_utf8_lossy(&route_output.stderr);
    assert!(route_output.status.success(), "stderr: {route_stderr}");
    assert!(route_output.stdout.is_empty());
    assert!(route_output_dir.path().is_dir());

    let route_files = fs::read_dir(route_output_dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();

    assert!(
        !route_files.is_empty() || route_stderr.contains("No routes found"),
        "expected route output or explicit no-route success, stderr: {route_stderr}",
    );

    if db_path.exists() {
        fs::remove_file(db_path).unwrap();
    }
}
