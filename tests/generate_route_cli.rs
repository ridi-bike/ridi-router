use std::{
    fs,
    path::{Path, PathBuf},
    process::{self, Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn unique_test_dir(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "ridi-router-{prefix}-{}-{}",
        process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn fixture_tiles_dir() -> PathBuf {
    repo_path("map-data/output")
}

fn fixture_rule_file() -> PathBuf {
    repo_path("rule-examples/rules-empty.json")
}

fn base_generate_route_command(output_dir: &Path, format: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ridi-router"));
    command
        .arg("generate-route")
        .arg("--tiles")
        .arg(fixture_tiles_dir())
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--format")
        .arg(format)
        .arg("--rule-file")
        .arg(fixture_rule_file())
        .arg("start-finish")
        .arg("--start")
        .arg("56.951861,24.113821")
        .arg("--finish")
        .arg("57.313103,25.281460");
    command
}

fn run_generate_route(output_dir: &Path, format: &str) -> Output {
    base_generate_route_command(output_dir, format)
        .output()
        .expect("failed to execute ridi-router generate-route")
}

#[test]
fn generate_route_end_to_end_json_output_dir() {
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
    assert_eq!(fs::read_dir(&output_dir).unwrap().count(), 0);

    fs::remove_dir_all(output_dir).unwrap();
}

#[test]
fn generate_route_end_to_end_gpx_output_dir() {
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
    assert_eq!(fs::read_dir(&output_dir).unwrap().count(), 0);

    fs::remove_dir_all(output_dir).unwrap();
}

#[test]
fn generate_route_non_empty_output_dir_fails() {
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

    let output = Command::new(env!("CARGO_BIN_EXE_ridi-router"))
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
        .expect("failed to execute ridi-router generate-route");

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

    let output = Command::new(env!("CARGO_BIN_EXE_ridi-router"))
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
        .expect("failed to execute ridi-router generate-route");

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
    let output_dir = unique_test_dir("cli-zero-routes");
    let output = run_generate_route(&output_dir, "json");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "stderr: {stderr}");
    assert!(output.stdout.is_empty());
    assert!(stderr.contains("No routes found"), "stderr: {stderr}");
    assert!(output_dir.is_dir());
    assert_eq!(fs::read_dir(&output_dir).unwrap().count(), 0);

    fs::remove_dir_all(output_dir).unwrap();
}
