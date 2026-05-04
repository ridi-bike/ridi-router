use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use ridi_router_test_support::rmdf::{create_linear_single_tile_fixture, unique_test_dir};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn repo_path(relative: &str) -> PathBuf {
    workspace_root().join(relative)
}

fn fixture_tiles_dir() -> PathBuf {
    repo_path("map-data/output")
}

fn fixture_rule_file() -> PathBuf {
    repo_path("rule-examples/rules-default.json")
}

fn repo_fixture_tiles_available() -> bool {
    fixture_tiles_dir().join("manifest.json").exists() && fixture_rule_file().exists()
}

fn base_generate_route_command(
    tiles_dir: &Path,
    rule_file: &Path,
    output_dir: &Path,
    format: &str,
    start: &str,
    finish: &str,
) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ridi-router-cli"));
    command
        .arg("generate-route")
        .arg("--tiles")
        .arg(tiles_dir)
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--format")
        .arg(format)
        .arg("--rule-file")
        .arg(rule_file)
        .arg("start-finish")
        .arg("--start")
        .arg(start)
        .arg("--finish")
        .arg(finish);
    command
}

fn run_generate_route(output_dir: &Path, format: &str) -> Output {
    base_generate_route_command(
        &fixture_tiles_dir(),
        &fixture_rule_file(),
        output_dir,
        format,
        "56.951861,24.113821",
        "57.313103,25.281460",
    )
    .output()
    .expect("failed to execute ridi-router-cli generate-route")
}

const SYNTHETIC_START: &str = "10.0,20.0";
const SYNTHETIC_FINISH: &str = "10.12,20.0";

fn create_synthetic_success_fixture() -> (PathBuf, PathBuf) {
    let fixture = create_linear_single_tile_fixture("cli-success-fixture");
    (fixture.dir, fixture.rule_file)
}

#[test]
fn generate_route_end_to_end_json_output_dir() {
    if !repo_fixture_tiles_available() {
        eprintln!("Skipping test: map-data/output fixture is not available");
        return;
    }
    let output_dir = unique_test_dir("cli-json-output-dir");
    let output = run_generate_route(&output_dir, "json");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "stdout should stay empty for final JSON output"
    );
    assert!(output_dir.is_dir());
    assert!(fs::read_dir(&output_dir).unwrap().count() >= 1);

    fs::remove_dir_all(output_dir).unwrap();
}

#[test]
fn generate_route_end_to_end_gpx_output_dir() {
    if !repo_fixture_tiles_available() {
        eprintln!("Skipping test: map-data/output fixture is not available");
        return;
    }
    let output_dir = unique_test_dir("cli-gpx-output-dir");
    fs::create_dir_all(&output_dir).unwrap();

    let output = run_generate_route(&output_dir, "gpx");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "stdout should stay empty for final GPX output"
    );
    assert!(output_dir.is_dir());
    assert!(fs::read_dir(&output_dir).unwrap().count() >= 1);

    fs::remove_dir_all(output_dir).unwrap();
}

#[test]
fn generate_route_non_empty_output_dir_fails() {
    if !repo_fixture_tiles_available() {
        eprintln!("Skipping test: map-data/output fixture is not available");
        return;
    }
    let output_dir = unique_test_dir("cli-non-empty-output-dir");
    fs::create_dir_all(&output_dir).unwrap();
    fs::write(output_dir.join("already-there.txt"), "sentinel").unwrap();

    let output = run_generate_route(&output_dir, "json");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        stderr.contains("Output directory must be empty"),
        "stderr: {stderr}"
    );
    assert!(output_dir.join("already-there.txt").exists());
    assert_eq!(fs::read_dir(&output_dir).unwrap().count(), 1);

    fs::remove_dir_all(output_dir).unwrap();
}

#[test]
fn generate_route_missing_manifest_fails() {
    let tiles_dir = unique_test_dir("cli-missing-manifest-tiles");
    let output_dir = unique_test_dir("cli-missing-manifest-output");
    fs::create_dir_all(&tiles_dir).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ridi-router-cli"))
        .arg("generate-route")
        .arg("--tiles")
        .arg(&tiles_dir)
        .arg("--output-dir")
        .arg(&output_dir)
        .arg("--format")
        .arg("json")
        .arg("--rule-file")
        .arg(fixture_rule_file())
        .arg("start-finish")
        .arg("--start")
        .arg("56.951861,24.113821")
        .arg("--finish")
        .arg("57.313103,25.281460")
        .output()
        .expect("failed to execute ridi-router-cli generate-route");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(stderr.contains("manifest.json"), "stderr: {stderr}");

    fs::remove_dir_all(tiles_dir).unwrap();
    if output_dir.exists() {
        fs::remove_dir_all(output_dir).unwrap();
    }
}

#[test]
fn generate_route_invalid_tiles_dir_fails() {
    let tiles_dir = unique_test_dir("cli-invalid-tiles-dir");
    let output_dir = unique_test_dir("cli-invalid-tiles-output");

    let output = Command::new(env!("CARGO_BIN_EXE_ridi-router-cli"))
        .arg("generate-route")
        .arg("--tiles")
        .arg(&tiles_dir)
        .arg("--output-dir")
        .arg(&output_dir)
        .arg("--format")
        .arg("json")
        .arg("--rule-file")
        .arg(fixture_rule_file())
        .arg("start-finish")
        .arg("--start")
        .arg("56.951861,24.113821")
        .arg("--finish")
        .arg("57.313103,25.281460")
        .output()
        .expect("failed to execute ridi-router-cli generate-route");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        stderr.contains("Failed to initialize TileManager"),
        "stderr: {stderr}"
    );

    if output_dir.exists() {
        fs::remove_dir_all(output_dir).unwrap();
    }
}

#[test]
fn generate_route_zero_routes_succeeds() {
    if !repo_fixture_tiles_available() {
        eprintln!("Skipping test: map-data/output fixture is not available");
        return;
    }
    let output_dir = unique_test_dir("cli-zero-routes");
    let output = run_generate_route(&output_dir, "json");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "stderr: {stderr}");
    assert!(output.stdout.is_empty());
    assert!(!stderr.contains("No routes found"), "stderr: {stderr}");
    assert!(output_dir.is_dir());
    assert!(fs::read_dir(&output_dir).unwrap().count() >= 1);

    fs::remove_dir_all(output_dir).unwrap();
}

#[test]
fn generate_route_synthetic_json_writes_route_file() {
    let (tiles_dir, rule_file) = create_synthetic_success_fixture();
    let output_dir = unique_test_dir("cli-synthetic-json-output");

    let output = base_generate_route_command(
        &tiles_dir,
        &rule_file,
        &output_dir,
        "json",
        SYNTHETIC_START,
        SYNTHETIC_FINISH,
    )
    .output()
    .expect("failed to execute synthetic JSON route generation");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(output.stdout.is_empty(), "stdout should stay empty");

    let files = fs::read_dir(&output_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1, "expected one route file, got: {files:?}");
    assert_eq!(
        files[0].extension().and_then(|ext| ext.to_str()),
        Some("json")
    );

    let content = fs::read_to_string(&files[0]).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(json.get("coords").and_then(|v| v.as_array()).is_some());
    assert!(json.get("stats").is_some());

    fs::remove_dir_all(output_dir).unwrap();
    fs::remove_dir_all(tiles_dir).unwrap();
}

#[test]
fn generate_route_synthetic_gpx_writes_route_file() {
    let (tiles_dir, rule_file) = create_synthetic_success_fixture();
    let output_dir = unique_test_dir("cli-synthetic-gpx-output");

    let output = base_generate_route_command(
        &tiles_dir,
        &rule_file,
        &output_dir,
        "gpx",
        SYNTHETIC_START,
        SYNTHETIC_FINISH,
    )
    .output()
    .expect("failed to execute synthetic GPX route generation");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(output.stdout.is_empty(), "stdout should stay empty");

    let files = fs::read_dir(&output_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1, "expected one route file, got: {files:?}");
    assert_eq!(
        files[0].extension().and_then(|ext| ext.to_str()),
        Some("gpx")
    );

    let content = fs::read_to_string(&files[0]).unwrap();
    assert!(content.contains("<gpx"), "content: {content}");
    assert!(content.contains("<rtept"), "content: {content}");

    fs::remove_dir_all(output_dir).unwrap();
    fs::remove_dir_all(tiles_dir).unwrap();
}
