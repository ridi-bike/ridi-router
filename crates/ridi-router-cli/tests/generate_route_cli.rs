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
    repo_path("rule-examples/rules-empty.json")
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

const RMDF_HEADER_SIZE: u64 = 112;
const POINT_RECORD_SIZE: u64 = 48;
const LINE_RECORD_SIZE: u64 = 40;
const TILE_FILENAME: &str = "tile_200_100.rmdf";
const SYNTHETIC_START: &str = "10.0,20.0";
const SYNTHETIC_FINISH: &str = "10.12,20.0";

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

fn create_synthetic_success_fixture() -> (PathBuf, PathBuf) {
    let fixture_dir = unique_test_dir("cli-success-fixture");
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

    let manifest = serde_json::json!({
        "version": "test",
        "tile_size_degrees": 1.0,
        "format_version": 1,
        "generated_at": "2026-04-03T00:00:00Z",
        "source_files": ["synthetic"],
        "tiles": [{
            "filename": TILE_FILENAME,
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
            "size_bytes": fs::metadata(fixture_dir.join(TILE_FILENAME)).unwrap().len(),
            "point_count": 13,
            "line_count": 12,
            "checksum": "sha256:test"
        }]
    });
    fs::write(
        fixture_dir.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();

    let rules = serde_json::json!({
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
    });
    let rule_file = fixture_dir.join("rules.json");
    fs::write(&rule_file, serde_json::to_vec(&rules).unwrap()).unwrap();

    (fixture_dir, rule_file)
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
