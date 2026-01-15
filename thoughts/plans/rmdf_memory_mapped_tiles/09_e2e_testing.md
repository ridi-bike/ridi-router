# Phase 9: End-to-End Testing

## Overview

Comprehensive end-to-end testing with Montenegro dataset to validate the complete RMDF system: tile generation, routing, and missing tile handling.

**Goals:**
- Add Montenegro PBF to repository for repeatable tests
- Automated test: Generate tiles with small tile size (0.1°)
- Automated test: Route between specified coordinates (success)
- Automated test: Delete middle tile, route again (detour)
- Automated test: Delete all except start/finish tiles (fail gracefully)
- Performance validation

## Test Scenarios

### Test 1: Successful Cross-Tile Route

**Setup:**
- Montenegro PBF (~10MB) in `test_data/montenegro.osm.pbf`
- Generate tiles: `--tile-size 0.1`

**Steps:**
1. Run tile generation
2. Route from (42.45785, 18.50767) to (41.92802, 19.22959)
3. Verify route created successfully
4. Verify route crosses multiple tiles
5. Verify route output is valid GPX/JSON

**Expected Result:**
- Route generated without errors
- Route contains waypoints
- Route crosses at least 2 tiles (based on coordinates)
- Output file valid and non-empty

### Test 2: Route Around Missing Tile

**Setup:**
- Same Montenegro tiles from Test 1

**Steps:**
1. Identify a tile covering (42.28912, 18.84275)
2. Delete that tile file
3. Route from (42.45785, 18.50767) to (41.92802, 19.22959) again
4. Verify route still succeeds (takes detour)
5. Verify route avoids deleted tile area

**Expected Result:**
- Route generated successfully
- Route different from Test 1 (detour taken)
- Warning logged about missing tile
- No crash or panic

### Test 3: No Route (Missing Tiles)

**Setup:**
- Same Montenegro tiles from Test 1

**Steps:**
1. Determine start and finish tiles from coordinates
2. Delete all tiles EXCEPT start and finish tiles
3. Attempt to route from (42.45785, 18.50767) to (41.92802, 19.22959)
4. Verify routing fails gracefully

**Expected Result:**
- Routing fails with clear error message
- Error message: "Could not find route" or "Missing required tiles"
- No crash or panic
- Exit code non-zero

## Implementation

### 1. Add Test Data

**File**: Add to repository

```
test_data/
├── montenegro.osm.pbf  (~10MB)
└── README.md           (source: https://download.geofabrik.de/europe/montenegro-latest.osm.pbf)
```

**Note**: Add to `.gitignore` or use Git LFS if file too large.

### 2. Create E2E Test Module

**File**: `tests/e2e_rmdf.rs` (new file in tests/ directory)

**Changes**: Integration test suite

```rust
use std::path::PathBuf;
use std::process::Command;

const MONTENEGRO_PBF: &str = "test_data/montenegro.osm.pbf";
const TEST_TILES_DIR: &str = "test_data/montenegro_tiles_e2e";

#[test]
fn test_01_generate_tiles() {
    // Clean output directory
    let _ = std::fs::remove_dir_all(TEST_TILES_DIR);

    let output = Command::new("cargo")
        .args(&[
            "run", "--",
            "generate-tiles",
            "--input", MONTENEGRO_PBF,
            "--output", TEST_TILES_DIR,
            "--tile-size", "0.1",
        ])
        .output()
        .expect("Failed to execute generate-tiles");

    assert!(output.status.success(), "Tile generation failed: {:?}", output);

    // Verify tiles created
    let manifest_path = PathBuf::from(TEST_TILES_DIR).join("manifest.json");
    assert!(manifest_path.exists(), "manifest.json not created");

    // Verify at least one tile created
    let tiles = std::fs::read_dir(TEST_TILES_DIR).unwrap();
    let rmdf_count = tiles.filter(|e| {
        e.as_ref().unwrap().path().extension() == Some(std::ffi::OsStr::new("rmdf"))
    }).count();
    assert!(rmdf_count > 0, "No RMDF tiles created");
}

#[test]
fn test_02_route_success() {
    let output = Command::new("cargo")
        .args(&[
            "run", "--",
            "generate-route",
            "--tiles", TEST_TILES_DIR,
            "--routing-mode", "start-finish",
            "--start", "42.45785,18.50767",
            "--finish", "41.92802,19.22959",
            "--output", "test_data/route_success.gpx",
        ])
        .output()
        .expect("Failed to execute routing");

    assert!(output.status.success(), "Routing failed: {:?}", String::from_utf8_lossy(&output.stderr));

    // Verify route file created
    let route_path = PathBuf::from("test_data/route_success.gpx");
    assert!(route_path.exists(), "Route file not created");

    // Verify route has content
    let route_content = std::fs::read_to_string(&route_path).unwrap();
    assert!(route_content.len() > 100, "Route file too small");
    assert!(route_content.contains("<gpx"), "Route file not valid GPX");
}

#[test]
fn test_03_route_around_missing_tile() {
    // Find a middle tile (approximately between start and finish)
    let middle_tile = find_tile_covering(42.28912, 18.84275);
    let tile_path = PathBuf::from(TEST_TILES_DIR).join(&middle_tile);

    // Delete the tile
    std::fs::remove_file(&tile_path).expect("Failed to delete tile");

    // Route again
    let output = Command::new("cargo")
        .args(&[
            "run", "--",
            "generate-route",
            "--tiles", TEST_TILES_DIR,
            "--routing-mode", "start-finish",
            "--start", "42.45785,18.50767",
            "--finish", "41.92802,19.22959",
            "--output", "test_data/route_detour.gpx",
        ])
        .output()
        .expect("Failed to execute routing");

    // Should still succeed (takes detour)
    assert!(output.status.success(), "Routing failed with missing tile: {:?}",
            String::from_utf8_lossy(&output.stderr));

    // Verify warning about missing tile in stderr
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not available") || stderr.contains("missing"),
            "No warning about missing tile");
}

#[test]
fn test_04_route_fail_many_missing_tiles() {
    // Backup current tiles
    let backup_dir = PathBuf::from("test_data/tiles_backup");
    std::fs::create_dir_all(&backup_dir).unwrap();

    // Determine start and finish tiles
    let start_tile = find_tile_covering(42.45785, 18.50767);
    let finish_tile = find_tile_covering(41.92802, 19.22959);

    // Move all tiles except start and finish to backup
    for entry in std::fs::read_dir(TEST_TILES_DIR).unwrap() {
        let entry = entry.unwrap();
        let filename = entry.file_name().to_str().unwrap().to_string();

        if filename.ends_with(".rmdf") && filename != start_tile && filename != finish_tile {
            let src = entry.path();
            let dst = backup_dir.join(&filename);
            std::fs::rename(&src, &dst).unwrap();
        }
    }

    // Attempt to route
    let output = Command::new("cargo")
        .args(&[
            "run", "--",
            "generate-route",
            "--tiles", TEST_TILES_DIR,
            "--routing-mode", "start-finish",
            "--start", "42.45785,18.50767",
            "--finish", "41.92802,19.22959",
            "--output", "test_data/route_fail.gpx",
        ])
        .output()
        .expect("Failed to execute routing");

    // Should fail with clear error
    assert!(!output.status.success(), "Routing should have failed with missing tiles");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Could not find route") ||
        stderr.contains("missing") ||
        stderr.contains("no route"),
        "Error message unclear: {}", stderr
    );

    // Restore tiles from backup
    for entry in std::fs::read_dir(&backup_dir).unwrap() {
        let entry = entry.unwrap();
        let src = entry.path();
        let dst = PathBuf::from(TEST_TILES_DIR).join(entry.file_name());
        std::fs::rename(&src, &dst).unwrap();
    }
    std::fs::remove_dir_all(&backup_dir).unwrap();
}

fn find_tile_covering(lat: f64, lon: f64) -> String {
    let col = ((lon + 180.0) / 0.1).floor() as u16;
    let row = ((lat + 90.0) / 0.1).floor() as u16;
    format!("tile_{}_{}.rmdf", col, row)
}

#[test]
fn test_05_performance_baseline() {
    use std::time::Instant;

    let start_time = Instant::now();

    let output = Command::new("cargo")
        .args(&[
            "run", "--release", "--",
            "generate-route",
            "--tiles", TEST_TILES_DIR,
            "--routing-mode", "start-finish",
            "--start", "42.45785,18.50767",
            "--finish", "41.92802,19.22959",
            "--output", "test_data/route_perf.gpx",
        ])
        .output()
        .expect("Failed to execute routing");

    let elapsed = start_time.elapsed();

    assert!(output.status.success(), "Performance test routing failed");

    // Performance target: <5 seconds for Montenegro route
    assert!(elapsed.as_secs() < 5, "Routing too slow: {:?}", elapsed);

    println!("Routing completed in {:?}", elapsed);
}
```

**Rationale**: Comprehensive test coverage for all scenarios.

### 3. Add CI Integration

**File**: `.github/workflows/ci.yml` (if using GitHub Actions)

**Changes**: Add E2E test step

```yaml
name: CI

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run unit tests
        run: cargo test --lib
      - name: Download Montenegro PBF
        run: |
          mkdir -p test_data
          wget -O test_data/montenegro.osm.pbf \
            https://download.geofabrik.de/europe/montenegro-latest.osm.pbf
      - name: Run E2E tests
        run: cargo test --test e2e_rmdf -- --test-threads=1
```

**Rationale**: Automated testing on every commit.

## Success Criteria

### Automated Verification

- [x] Test 1 implemented: Tile generation creates tiles
- [x] Test 2 implemented: Successful routing across tiles
- [x] Test 3 implemented: Routing around missing tile
- [x] Test 4 implemented: Graceful failure with many missing tiles
- [x] Test 5 implemented: Performance baseline (<5s for Montenegro)
- [ ] All tests pass in CI (tests implemented, ready to run)

### Manual Verification

- [ ] Run tests locally: `cargo test --test e2e_rmdf` (ready to run, requires manual execution)
- [ ] Inspect generated route files (GPX valid)
- [ ] Verify logs show tile loading behavior
- [ ] Memory usage reasonable during tests
- [ ] No memory leaks (valgrind clean)

## Dependencies

- **Depends on**: Phase 8 (needs clean codebase)
- **Blocks**: None (final phase)

## Risks & Mitigations

**Risk**: Montenegro PBF too large for git
- **Mitigation**: Use Git LFS or download in CI

**Risk**: Tests flaky due to timing
- **Mitigation**: Use deterministic tile generation, fixed coordinates

**Risk**: Performance test fails on slow CI
- **Mitigation**: Adjust threshold for CI environment

## Notes

- Tests run sequentially (--test-threads=1) to avoid conflicts
- Montenegro PBF downloaded fresh in CI (always up-to-date)
- Route coordinates chosen to span multiple tiles
- Missing tile coordinates chosen to be on route path
- Performance baseline for future optimization

## Deviations from Plan

### Phase 9: End-to-End Testing
- **Original Plan**: Implement E2E tests assuming compilation works
- **Actual Implementation**: Had to fix pre-existing Phase 7 compilation errors before tests could run
- **Reason for Deviation**: Phase 7/8 left compilation errors related to type mismatches between `Option<String>` (returned by new RMDF tag methods) and `Option<&SmartString>` (expected by routing code)
- **Changes Made**:
  - Fixed `src/router/weights.rs`:
    - Changed `get_rule_for_tag` parameter from `Option<&SmartString>` to `Option<String>`
    - Changed `was_on_avoid` closure parameter from `Fn(&Segment) -> Option<&SmartString>` to `Fn(&Segment) -> Option<String>`
    - Removed unnecessary `.cloned()` calls on `Option<String>` values
  - Fixed `src/router/route/mod.rs`:
    - Changed `is_back_on_road_within_distance` parameters from `Option<SmartString>` to `Option<String>`
    - Changed `update_map` parameter from `Option<&SmartString>` to `Option<String>`
    - Removed `.as_ref()` calls on `Option<String>` comparisons
- **Impact Assessment**: These fixes complete the Phase 7 routing integration that was marked as partial. All compilation errors resolved, tests are now ready to run.
- **Date/Time**: 2026-01-16T00:30:00+02:00
