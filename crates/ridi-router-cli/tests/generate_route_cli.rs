use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use ridi_router_test_support::rmdf::{
    create_linear_single_tile_fixture, create_round_trip_single_tile_fixture, unique_test_dir,
};

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

fn base_generate_round_trip_command(
    tiles_dir: &Path,
    rule_file: &Path,
    output_dir: &Path,
    format: &str,
    start_finish: &str,
    bearing: &str,
    distance: &str,
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
        .arg("round-trip")
        .arg("--start-finish")
        .arg(start_finish)
        .arg("--bearing")
        .arg(bearing)
        .arg("--distance")
        .arg(distance);
    command
}

const SYNTHETIC_START: &str = "10.0,20.0";
const SYNTHETIC_FINISH: &str = "10.12,20.0";

fn create_synthetic_success_fixture() -> (PathBuf, PathBuf) {
    let fixture = create_linear_single_tile_fixture("cli-success-fixture");
    (fixture.dir, fixture.rule_file)
}

#[test]
fn generate_route_end_to_end_json_output_dir() {
    let (tiles_dir, rule_file) = create_synthetic_success_fixture();
    let output_dir = unique_test_dir("cli-json-output-dir");
    let output = base_generate_route_command(
        &tiles_dir,
        &rule_file,
        &output_dir,
        "json",
        SYNTHETIC_START,
        SYNTHETIC_FINISH,
    )
    .output()
    .expect("failed to execute ridi-router-cli generate-route");

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

    fs::remove_dir_all(tiles_dir).unwrap();
    fs::remove_dir_all(output_dir).unwrap();
}

#[test]
fn generate_route_end_to_end_gpx_output_dir() {
    let (tiles_dir, rule_file) = create_synthetic_success_fixture();
    let output_dir = unique_test_dir("cli-gpx-output-dir");
    fs::create_dir_all(&output_dir).unwrap();

    let output = base_generate_route_command(
        &tiles_dir,
        &rule_file,
        &output_dir,
        "gpx",
        SYNTHETIC_START,
        SYNTHETIC_FINISH,
    )
    .output()
    .expect("failed to execute ridi-router-cli generate-route");

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

    fs::remove_dir_all(tiles_dir).unwrap();
    fs::remove_dir_all(output_dir).unwrap();
}

#[test]
fn generate_route_non_empty_output_dir_fails() {
    let (tiles_dir, rule_file) = create_synthetic_success_fixture();
    let output_dir = unique_test_dir("cli-non-empty-output-dir");
    fs::create_dir_all(&output_dir).unwrap();
    fs::write(output_dir.join("already-there.txt"), "sentinel").unwrap();

    let output = base_generate_route_command(
        &tiles_dir,
        &rule_file,
        &output_dir,
        "json",
        SYNTHETIC_START,
        SYNTHETIC_FINISH,
    )
    .output()
    .expect("failed to execute ridi-router-cli generate-route");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        stderr.contains("Output directory must be empty"),
        "stderr: {stderr}"
    );
    assert!(output_dir.join("already-there.txt").exists());
    assert_eq!(fs::read_dir(&output_dir).unwrap().count(), 1);

    fs::remove_dir_all(tiles_dir).unwrap();
    fs::remove_dir_all(output_dir).unwrap();
}

#[test]
fn generate_route_missing_manifest_fails() {
    let (fixture_dir, rule_file) = create_synthetic_success_fixture();
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
        .arg(&rule_file)
        .arg("start-finish")
        .arg("--start")
        .arg(SYNTHETIC_START)
        .arg("--finish")
        .arg(SYNTHETIC_FINISH)
        .output()
        .expect("failed to execute ridi-router-cli generate-route");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(stderr.contains("manifest.json"), "stderr: {stderr}");

    fs::remove_dir_all(fixture_dir).unwrap();
    fs::remove_dir_all(tiles_dir).unwrap();
    if output_dir.exists() {
        fs::remove_dir_all(output_dir).unwrap();
    }
}

#[test]
fn generate_route_invalid_tiles_dir_fails() {
    let (fixture_dir, rule_file) = create_synthetic_success_fixture();
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
        .arg(&rule_file)
        .arg("start-finish")
        .arg("--start")
        .arg(SYNTHETIC_START)
        .arg("--finish")
        .arg(SYNTHETIC_FINISH)
        .output()
        .expect("failed to execute ridi-router-cli generate-route");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        stderr.contains("Failed to initialize TileManager"),
        "stderr: {stderr}"
    );

    fs::remove_dir_all(fixture_dir).unwrap();
    if output_dir.exists() {
        fs::remove_dir_all(output_dir).unwrap();
    }
}

#[test]
fn generate_route_does_not_report_no_routes_when_route_exists() {
    let (tiles_dir, rule_file) = create_synthetic_success_fixture();
    let output_dir = unique_test_dir("cli-route-found");
    let output = base_generate_route_command(
        &tiles_dir,
        &rule_file,
        &output_dir,
        "json",
        SYNTHETIC_START,
        SYNTHETIC_FINISH,
    )
    .output()
    .expect("failed to execute ridi-router-cli generate-route");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "stderr: {stderr}");
    assert!(output.stdout.is_empty());
    assert!(!stderr.contains("No routes found"), "stderr: {stderr}");
    assert!(output_dir.is_dir());
    assert!(fs::read_dir(&output_dir).unwrap().count() >= 1);

    fs::remove_dir_all(tiles_dir).unwrap();
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
fn generate_route_round_trip_json_writes_route_file() {
    let fixture = create_round_trip_single_tile_fixture("cli-round-trip-json-fixture");
    let output_dir = unique_test_dir("cli-round-trip-json-output");
    let start_finish = format!("{},{}", fixture.start_finish_lat, fixture.start_finish_lon);

    let output = base_generate_round_trip_command(
        &fixture.dir,
        &fixture.rule_file,
        &output_dir,
        "json",
        &start_finish,
        "45",
        "4000",
    )
    .output()
    .expect("failed to execute synthetic round-trip JSON route generation");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(output.stdout.is_empty(), "stdout should stay empty");
    assert!(!stderr.contains("No routes found"), "stderr: {stderr}");

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
    let coords = json
        .get("coords")
        .and_then(|v| v.as_array())
        .expect("round-trip JSON should contain coords");
    assert!(coords.len() >= 2, "coords: {coords:?}");
    assert!(json.get("stats").is_some());

    fs::remove_dir_all(output_dir).unwrap();
    fs::remove_dir_all(fixture.dir).unwrap();
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

#[test]
fn generate_route_round_trip_gpx_writes_route_file() {
    let fixture = create_round_trip_single_tile_fixture("cli-round-trip-gpx-fixture");
    let output_dir = unique_test_dir("cli-round-trip-gpx-output");
    let start_finish = format!("{},{}", fixture.start_finish_lat, fixture.start_finish_lon);

    let output = base_generate_round_trip_command(
        &fixture.dir,
        &fixture.rule_file,
        &output_dir,
        "gpx",
        &start_finish,
        "45",
        "4000",
    )
    .output()
    .expect("failed to execute synthetic round-trip GPX route generation");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(output.stdout.is_empty(), "stdout should stay empty");
    assert!(!stderr.contains("No routes found"), "stderr: {stderr}");

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
    fs::remove_dir_all(fixture.dir).unwrap();
}
