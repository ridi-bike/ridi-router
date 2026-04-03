use std::{
    fs,
    path::PathBuf,
    process::{self, Command},
    time::{SystemTime, UNIX_EPOCH},
};

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

#[test]
fn generate_tiles_cli_renders_typed_tile_generation_error() {
    let input_file = unique_test_path("missing-input.osm.pbf");
    let output_dir = unique_test_path("tiles-output");

    let output = Command::new(env!("CARGO_BIN_EXE_ridi-router-cli"))
        .arg("generate-tiles")
        .arg("--input")
        .arg(&input_file)
        .arg("--output")
        .arg(&output_dir)
        .output()
        .expect("failed to execute ridi-router-cli generate-tiles");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        stderr.contains("Tile generation error:"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("missing-input.osm.pbf"), "stderr: {stderr}");

    if output_dir.exists() {
        fs::remove_dir_all(output_dir).unwrap();
    }
}

#[test]
fn generate_tiles_cli_requires_input_or_input_dir() {
    let output_dir = unique_test_path("tiles-output-missing-input");

    let output = Command::new(env!("CARGO_BIN_EXE_ridi-router-cli"))
        .arg("generate-tiles")
        .arg("--output")
        .arg(&output_dir)
        .output()
        .expect("failed to execute ridi-router-cli generate-tiles");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        stderr.contains(
            "Tile generation argument error: Invalid tile generation arguments: must provide either --input or --input-dir",
        ),
        "stderr: {stderr}"
    );

    if output_dir.exists() {
        fs::remove_dir_all(output_dir).unwrap();
    }
}
