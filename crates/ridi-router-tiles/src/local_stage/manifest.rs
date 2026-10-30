//! Manifest from in-memory tile results (same content as `ManifestGenerator::generate`), plus
//! manifest parts for regional runs and their merge.

use anyhow::{Context, Result};
use ridi_router_common::manifest::{TileManifest, TileMetadata, TileNeighbors};
use std::collections::HashSet;
use std::path::Path;

pub(crate) const PARTS_DIR: &str = "manifest.parts";

pub(crate) fn empty_neighbors() -> TileNeighbors {
    TileNeighbors {
        north: None,
        south: None,
        east: None,
        west: None,
        northeast: None,
        northwest: None,
        southeast: None,
        southwest: None,
    }
}

/// Same neighbour rule as `ManifestGenerator::compute_neighbors`.
fn fill_neighbors(tiles: &mut [TileMetadata]) {
    let set: HashSet<(u16, u16)> = tiles.iter().map(|t| (t.col, t.row)).collect();
    let get = |col: i32, row: i32| -> Option<String> {
        if !(0..=359).contains(&col) || !(0..=179).contains(&row) {
            return None;
        }
        set.contains(&(col as u16, row as u16))
            .then(|| format!("tile_{col}_{row}.rmdf"))
    };
    for t in tiles.iter_mut() {
        let (c, r) = (t.col as i32, t.row as i32);
        t.neighbors = TileNeighbors {
            north: get(c, r + 1),
            south: get(c, r - 1),
            east: get(c + 1, r),
            west: get(c - 1, r),
            northeast: get(c + 1, r + 1),
            northwest: get(c - 1, r + 1),
            southeast: get(c + 1, r - 1),
            southwest: get(c - 1, r - 1),
        };
    }
}

fn build(
    tile_size_degrees: f32,
    mut tiles: Vec<TileMetadata>,
    sources: Vec<String>,
) -> TileManifest {
    tiles.sort_by_key(|t| (t.col, t.row));
    fill_neighbors(&mut tiles);
    TileManifest {
        version: env!("CARGO_PKG_VERSION").to_string(),
        tile_size_degrees,
        format_version: 1,
        generated_at: chrono::Utc::now().to_rfc3339(),
        source_files: sources,
        tiles,
    }
}

fn write_json(path: &Path, manifest: &TileManifest) -> Result<()> {
    let file = std::io::BufWriter::new(
        std::fs::File::create(path).with_context(|| format!("Failed to create {path:?}"))?,
    );
    serde_json::to_writer_pretty(file, manifest)?;
    Ok(())
}

/// Write `manifest.json` for the given tiles.
pub fn write_manifest(
    output_dir: &Path,
    tile_size_degrees: f32,
    tiles: Vec<TileMetadata>,
    sources: Vec<String>,
) -> Result<()> {
    let manifest = build(tile_size_degrees, tiles, sources);
    write_json(&output_dir.join("manifest.json"), &manifest)
}

/// Write this run's tiles as `manifest.parts/<part>.json` (neighbours are recomputed on merge).
pub fn write_manifest_part(
    output_dir: &Path,
    part: &str,
    tile_size_degrees: f32,
    tiles: Vec<TileMetadata>,
    source: &str,
) -> Result<()> {
    let dir = output_dir.join(PARTS_DIR);
    std::fs::create_dir_all(&dir)?;
    let manifest = build(tile_size_degrees, tiles, vec![source.to_string()]);
    write_json(&dir.join(format!("{part}.json")), &manifest)
}

/// Merge all `manifest.parts/*.json` into `manifest.json` (tiles from later parts with the
/// same filename replace earlier ones; neighbours recomputed over the union).
pub fn merge_manifest_parts(output_dir: &Path) -> Result<usize> {
    let dir = output_dir.join(PARTS_DIR);
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .with_context(|| format!("Failed to read {dir:?}"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();
    let mut by_name = std::collections::BTreeMap::new();
    let mut sources = Vec::new();
    let mut tile_size = None;
    for path in &paths {
        let part: TileManifest = serde_json::from_slice(&std::fs::read(path)?)
            .with_context(|| format!("Failed to parse {path:?}"))?;
        if let Some(ts) = tile_size {
            anyhow::ensure!(
                ts == part.tile_size_degrees,
                "tile size mismatch in {path:?}"
            );
        }
        tile_size = Some(part.tile_size_degrees);
        sources.extend(part.source_files);
        for t in part.tiles {
            by_name.insert(t.filename.clone(), t);
        }
    }
    let tile_size = tile_size.context("no manifest parts found")?;
    let tiles: Vec<TileMetadata> = by_name.into_values().collect();
    let count = tiles.len();
    let manifest = build(tile_size, tiles, sources);
    write_json(&output_dir.join("manifest.json"), &manifest)?;
    Ok(count)
}
