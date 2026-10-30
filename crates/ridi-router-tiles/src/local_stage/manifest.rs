//! Manifest from in-memory tile results (same content as the legacy generator's manifest).

use anyhow::{Context, Result};
use ridi_router_common::manifest::{TileManifest, TileMetadata, TileNeighbors};
use std::collections::HashSet;
use std::path::Path;

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

/// Same neighbour rule as the legacy generator's manifest.
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
