//! Golden regression tests: generate tiles from the small fixtures in `tests/fixtures` and compare
//! a semantic digest of every tile (see `rmdf_diff::digest_dir`) with `tests/goldens/<case>.json`.
//!
//! Tile bytes are not deterministic (line, tag and rule order), so goldens hold one hash of each
//! tile's decoded content instead of the tiles. Every case also checks the tiles' internal
//! consistency and generator invariants (points inside the buffered tile bounds, no line across
//! the antimeridian, sorted spatial index), the tile grid range, and runs with
//! `RIDI_TILES_VERIFY_PROXIMITY=1`, which checks every proximity shortcut against the reference
//! maths.
//!
//! After an intended output change, regenerate the goldens with
//! `RIDI_UPDATE_GOLDENS=1 cargo test -p ridi-router-tiles --test goldens` and review the diff.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Once;

use ridi_router_common::manifest::TileManifest;
use ridi_router_tiles::{generate_tiles, TileGenerationRequest, TileInputSource};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> PathBuf {
    crate_dir().join("tests/fixtures").join(name)
}

fn setup() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::env::set_var("RIDI_TILES_VERIFY_PROXIMITY", "1");
        std::env::set_var("RIDI_TILES_THREADS", "4");
    });
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(case: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("ridi-tiles-golden-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn check(case: &str, input: TileInputSource, tile_size_deg: f32) -> rmdf_diff::Totals {
    setup();
    let tmp = TempDir::new(case);
    let out = tmp.0.join("tiles");
    let summary = generate_tiles(TileGenerationRequest {
        input,
        output_dir: out.clone(),
        tile_size_deg,
    })
    .unwrap_or_else(|e| panic!("{case}: generation failed: {e}"));
    assert!(summary.tile_count > 0, "{case}: no tiles");

    // tile grid range: cols < ceil(360 / ts), rows < ceil(180 / ts)
    let manifest: TileManifest =
        serde_json::from_slice(&std::fs::read(out.join("manifest.json")).unwrap()).unwrap();
    let max_col = (360.0 / tile_size_deg as f64).ceil() as u32;
    let max_row = (180.0 / tile_size_deg as f64).ceil() as u32;
    for t in &manifest.tiles {
        assert!(
            (t.col as u32) < max_col && (t.row as u32) < max_row,
            "{case}: tile {} outside the tile grid",
            t.filename
        );
    }

    let digest = rmdf_diff::digest_dir(&out).unwrap();
    assert!(
        digest.issues.is_empty(),
        "{case}: tile consistency issues {:?}, e.g. {:?}",
        digest.issues,
        digest.issue_examples
    );
    // (the manifest may also list empty tiles, as the legacy generator did)
    let listed: std::collections::HashSet<&str> =
        manifest.tiles.iter().map(|t| t.filename.as_str()).collect();
    for name in digest.tiles.keys() {
        assert!(
            listed.contains(name.as_str()),
            "{case}: non-empty tile {name} missing from the manifest"
        );
    }
    compare_golden(case, &digest.tiles);
    digest.totals
}

fn compare_golden(case: &str, tiles: &BTreeMap<String, String>) {
    let path = crate_dir()
        .join("tests/goldens")
        .join(format!("{case}.json"));
    if std::env::var("RIDI_UPDATE_GOLDENS").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string_pretty(tiles).unwrap() + "\n").unwrap();
        return;
    }
    let golden: BTreeMap<String, String> = serde_json::from_slice(
        &std::fs::read(&path).unwrap_or_else(|e| panic!("{case}: missing golden {path:?}: {e}")),
    )
    .unwrap();
    if &golden == tiles {
        return;
    }
    let changed: Vec<&String> = golden
        .keys()
        .chain(tiles.keys())
        .filter(|k| golden.get(*k) != tiles.get(*k))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    panic!(
        "{case}: {} of {} tiles differ from {path:?}: {:?}{}\n(compare with rmdf-diff against tiles from the previous version; update with RIDI_UPDATE_GOLDENS=1)",
        changed.len(),
        golden.len().max(tiles.len()),
        &changed[..changed.len().min(10)],
        if changed.len() > 10 { " ..." } else { "" }
    );
}

fn file(name: &str) -> TileInputSource {
    TileInputSource::File(fixture(name))
}

#[test]
fn golden_synthetic() {
    for ts in [0.05, 0.1, 0.25] {
        check(&format!("synthetic-{ts}"), file("synthetic.osm.pbf"), ts);
    }
    // the scenarios must actually exercise what they are for (see fixtures/synthetic.py)
    let totals = check("synthetic-0.1", file("synthetic.osm.pbf"), 0.1);
    assert!(totals.points_with_flag[0] > 0, "no residential flags");
    assert!(totals.points_with_flag[1] > 0, "no nogo flags");
    // the restriction is materialized in the via node's tile and in the tile whose buffer holds it
    assert_eq!(totals.rules, 2);
}

#[test]
fn golden_riga_center() {
    for ts in [0.05, 0.1, 0.25] {
        check(
            &format!("riga-center-{ts}"),
            file("riga-center.osm.pbf"),
            ts,
        );
    }
}

#[test]
fn golden_fiji_east_of_antimeridian() {
    check("fiji-east-180-0.1", file("fiji-east-180.osm.pbf"), 0.1);
}

#[test]
fn golden_fiji_west_of_antimeridian() {
    check("fiji-west-180-0.1", file("fiji-west-180.osm.pbf"), 0.1);
}

#[test]
fn golden_samoa() {
    check("samoa-apia-0.1", file("samoa-apia.osm.pbf"), 0.1);
}

#[test]
fn golden_tonga() {
    check("tonga-nukualofa-0.1", file("tonga-nukualofa.osm.pbf"), 0.1);
}

#[test]
fn golden_svalbard() {
    check("svalbard-78n-0.1", file("svalbard-78n.osm.pbf"), 0.1);
}

/// Two overlapping extracts across the Latvia/Lithuania border, read as one dataset. The golden
/// equals the output for the same two files merged with `osmium merge`.
#[test]
fn golden_border_directory() {
    // (one golden for directory input; its equality with `osmium merge` was checked when it was
    // recorded)
    let tmp = TempDir::new("border-input");
    for name in ["border-lv.osm.pbf", "border-lt.osm.pbf"] {
        std::fs::copy(fixture(name), tmp.0.join(name)).unwrap();
    }
    check(
        "border-lv-lt-dir-0.1",
        TileInputSource::Directory(tmp.0.clone()),
        0.1,
    );
}
