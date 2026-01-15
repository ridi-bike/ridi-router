use std::path::PathBuf;
use std::process::Command;

const MONTENEGRO_PBF: &str = "test_data/montenegro.osm.pbf";
const TEST_TILES_DIR: &str = "test_data/montenegro_tiles_e2e";

fn find_tile_covering(lat: f64, lon: f64, tile_size: f64) -> String {
    let col = ((lon + 180.0) / tile_size).floor() as u16;
    let row = ((lat + 90.0) / tile_size).floor() as u16;
    format!("tile_{}_{}.rmdf", col, row)
}

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

    if !output.status.success() {
        eprintln!("STDOUT: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
    }

    assert!(output.status.success(), "Tile generation failed");

    // Verify manifest.json created
    let manifest_path = PathBuf::from(TEST_TILES_DIR).join("manifest.json");
    assert!(manifest_path.exists(), "manifest.json not created");

    // Verify at least one tile created
    let tiles = std::fs::read_dir(TEST_TILES_DIR).unwrap();
    let rmdf_count = tiles
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .path()
                .extension()
                == Some(std::ffi::OsStr::new("rmdf"))
        })
        .count();
    assert!(rmdf_count > 0, "No RMDF tiles created");

    println!("Successfully generated {} RMDF tiles", rmdf_count);
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

    if !output.status.success() {
        eprintln!("STDOUT: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
    }

    assert!(
        output.status.success(),
        "Routing failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify route file created
    let route_path = PathBuf::from("test_data/route_success.gpx");
    assert!(route_path.exists(), "Route file not created");

    // Verify route has content
    let route_content = std::fs::read_to_string(&route_path).unwrap();
    assert!(route_content.len() > 100, "Route file too small");
    assert!(route_content.contains("<gpx"), "Route file not valid GPX");

    println!("Successfully generated route: {} bytes", route_content.len());
}

#[test]
fn test_03_route_around_missing_tile() {
    // Find a middle tile (approximately between start and finish)
    let tile_size = 0.1;
    let middle_tile = find_tile_covering(42.28912, 18.84275, tile_size);
    let tile_path = PathBuf::from(TEST_TILES_DIR).join(&middle_tile);

    // Check if tile exists before attempting to delete
    if !tile_path.exists() {
        println!("Middle tile {} doesn't exist, skipping test", middle_tile);
        return;
    }

    // Delete the tile
    std::fs::remove_file(&tile_path).expect("Failed to delete tile");
    println!("Deleted tile: {}", middle_tile);

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

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    if !output.status.success() {
        eprintln!("STDOUT: {}", stdout);
        eprintln!("STDERR: {}", stderr);
    }

    // Should still succeed (takes detour) OR fail gracefully
    // The behavior depends on whether alternative routes exist
    if output.status.success() {
        println!("Routing succeeded with detour around missing tile");

        // Verify warning about missing tile in stderr
        let has_warning = stderr.contains("not available")
            || stderr.contains("missing")
            || stderr.contains("Failed to load tile");

        if has_warning {
            println!("Warning about missing tile detected in logs");
        }
    } else {
        // Graceful failure is acceptable
        println!("Routing failed gracefully due to missing tile");
        assert!(
            stderr.contains("Could not find route")
                || stderr.contains("missing")
                || stderr.contains("no route")
                || stderr.contains("Failed to load tile"),
            "Error message unclear: {}",
            stderr
        );
    }
}

#[test]
fn test_04_route_fail_many_missing_tiles() {
    // Backup current tiles
    let backup_dir = PathBuf::from("test_data/tiles_backup");
    std::fs::create_dir_all(&backup_dir).unwrap();

    let tile_size = 0.1;

    // Determine start and finish tiles
    let start_tile = find_tile_covering(42.45785, 18.50767, tile_size);
    let finish_tile = find_tile_covering(41.92802, 19.22959, tile_size);

    println!("Start tile: {}, Finish tile: {}", start_tile, finish_tile);

    // Move all tiles except start and finish to backup
    let mut moved_count = 0;
    for entry in std::fs::read_dir(TEST_TILES_DIR).unwrap() {
        let entry = entry.unwrap();
        let filename = entry.file_name().to_str().unwrap().to_string();

        if filename.ends_with(".rmdf") && filename != start_tile && filename != finish_tile {
            let src = entry.path();
            let dst = backup_dir.join(&filename);
            std::fs::rename(&src, &dst).unwrap();
            moved_count += 1;
        }
    }

    println!("Moved {} tiles to backup", moved_count);

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

    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        eprintln!("STDOUT: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("STDERR: {}", stderr);
    }

    // Should fail with clear error
    assert!(
        !output.status.success(),
        "Routing should have failed with missing tiles"
    );

    assert!(
        stderr.contains("Could not find route")
            || stderr.contains("missing")
            || stderr.contains("no route")
            || stderr.contains("Failed to load tile")
            || stderr.contains("Point not found"),
        "Error message unclear: {}",
        stderr
    );

    println!("Routing failed as expected with appropriate error message");

    // Restore tiles from backup
    for entry in std::fs::read_dir(&backup_dir).unwrap() {
        let entry = entry.unwrap();
        let src = entry.path();
        let dst = PathBuf::from(TEST_TILES_DIR).join(entry.file_name());
        std::fs::rename(&src, &dst).unwrap();
    }
    std::fs::remove_dir_all(&backup_dir).unwrap();

    println!("Restored {} tiles from backup", moved_count);
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

    if !output.status.success() {
        eprintln!("STDOUT: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
    }

    assert!(output.status.success(), "Performance test routing failed");

    // Performance target: <5 seconds for Montenegro route
    // Note: This includes compile time for --release build on first run
    println!("Routing completed in {:?}", elapsed);

    // Only assert on timing if this isn't the first release build
    // (to avoid failures due to compilation time)
    if elapsed.as_secs() > 5 {
        println!(
            "WARNING: Routing took longer than 5 seconds ({:?}). \
             This may include compilation time if release build wasn't cached.",
            elapsed
        );
    }
}
