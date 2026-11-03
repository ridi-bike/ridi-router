//! Published tile layout, relative to the bucket prefix (default `rmdf/`):
//!
//! - `index.json` — the only mutable object. Lists the current release per RMDF format version
//!   and that release's chunks. Apps read it to discover tiles; it is written last.
//! - `v<format>/releases/<release_id>/chunks/<cx>_<cy>.json` — tiles of one chunk
//!   (`CHUNK_SIZE_DEGREES` cells on the tile grid origin).
//! - `v<format>/tiles/<col>_<row>/<checksum hex>.rmdf` — tile contents, addressed by checksum.
//!   A tile's checksum is its version: generation is deterministic, so unchanged areas keep the
//!   same object and apps only re-download tiles whose checksum changed.
//!
//! The pre-index `manifest.json` + `tile_*.rmdf` objects that older app builds read are not
//! touched.

use std::collections::BTreeMap;

use ridi_router_common::manifest::{TileBounds, TileMetadata};
use serde::{Deserialize, Serialize};

pub const INDEX_VERSION: u32 = 1;
pub const INDEX_KEY: &str = "index.json";
pub const CHUNK_SIZE_DEGREES: f32 = 10.0;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TileIndex {
    pub index_version: u32,
    /// One current release per RMDF format version, highest first.
    pub releases: Vec<Release>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub format_version: u32,
    pub release_id: String,
    pub published_at: String,
    pub tile_size_degrees: f32,
    pub chunk_size_degrees: f32,
    pub tile_count: u64,
    pub total_bytes: u64,
    pub source_files: Vec<String>,
    pub chunks: Vec<ChunkSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSummary {
    pub id: String,
    pub bounds: TileBounds,
    pub tile_count: u64,
    pub total_bytes: u64,
    /// Key of the chunk manifest, relative to the prefix.
    pub manifest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkManifest {
    pub format_version: u32,
    pub release_id: String,
    pub tile_size_degrees: f32,
    pub tiles: Vec<PublishedTile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PublishedTile {
    pub filename: String,
    pub col: u16,
    pub row: u16,
    pub bounds: TileBounds,
    pub size_bytes: u64,
    pub point_count: u64,
    pub line_count: u64,
    pub checksum: String,
    /// Key of the tile contents, relative to the prefix.
    pub path: String,
}

impl PublishedTile {
    pub fn from_metadata(format_version: u32, tile: &TileMetadata) -> Self {
        Self {
            filename: tile.filename.clone(),
            col: tile.col,
            row: tile.row,
            bounds: tile.bounds.clone(),
            size_bytes: tile.size_bytes,
            point_count: tile.point_count,
            line_count: tile.line_count,
            checksum: tile.checksum.clone(),
            path: tile_key(format_version, tile.col, tile.row, &tile.checksum),
        }
    }
}

pub fn tile_key(format_version: u32, col: u16, row: u16, checksum: &str) -> String {
    let hash = checksum.split_once(':').map_or(checksum, |(_, hex)| hex);
    format!("v{format_version}/tiles/{col}_{row}/{hash}.rmdf")
}

fn chunk_coords(tile: &PublishedTile, tile_size_degrees: f32) -> (u16, u16) {
    let per_chunk = (CHUNK_SIZE_DEGREES / tile_size_degrees).round().max(1.0) as u16;
    (tile.col / per_chunk, tile.row / per_chunk)
}

/// Groups tiles into chunk manifests. Returns `(summary, manifest)` pairs sorted by chunk id.
pub fn build_chunks(
    format_version: u32,
    release_id: &str,
    tile_size_degrees: f32,
    tiles: impl IntoIterator<Item = PublishedTile>,
) -> Vec<(ChunkSummary, ChunkManifest)> {
    let mut grouped: BTreeMap<(u16, u16), Vec<PublishedTile>> = BTreeMap::new();
    for tile in tiles {
        grouped
            .entry(chunk_coords(&tile, tile_size_degrees))
            .or_default()
            .push(tile);
    }

    grouped
        .into_iter()
        .map(|((cx, cy), mut tiles)| {
            tiles.sort_by_key(|tile| (tile.row, tile.col));
            let id = format!("{cx}_{cy}");
            let lon_min = cx as f32 * CHUNK_SIZE_DEGREES - 180.0;
            let lat_min = cy as f32 * CHUNK_SIZE_DEGREES - 90.0;
            let summary = ChunkSummary {
                manifest: format!("v{format_version}/releases/{release_id}/chunks/{id}.json"),
                id,
                bounds: TileBounds {
                    lat_min,
                    lat_max: lat_min + CHUNK_SIZE_DEGREES,
                    lon_min,
                    lon_max: lon_min + CHUNK_SIZE_DEGREES,
                },
                tile_count: tiles.len() as u64,
                total_bytes: tiles.iter().map(|tile| tile.size_bytes).sum(),
            };
            let manifest = ChunkManifest {
                format_version,
                release_id: release_id.to_string(),
                tile_size_degrees,
                tiles,
            };
            (summary, manifest)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(col: u16, row: u16) -> PublishedTile {
        PublishedTile {
            filename: format!("tile_{col}_{row}.rmdf"),
            col,
            row,
            bounds: TileBounds {
                lat_min: 0.0,
                lat_max: 0.0,
                lon_min: 0.0,
                lon_max: 0.0,
            },
            size_bytes: 10,
            point_count: 1,
            line_count: 1,
            checksum: "xxh3-128:abcd".to_string(),
            path: tile_key(2, col, row, "xxh3-128:abcd"),
        }
    }

    #[test]
    fn tile_key_strips_algorithm_prefix() {
        assert_eq!(
            tile_key(2, 2009, 1461, "xxh3-128:3338c48b"),
            "v2/tiles/2009_1461/3338c48b.rmdf"
        );
    }

    #[test]
    fn groups_tenth_degree_tiles_into_ten_degree_chunks() {
        // Riga (24.1E, 56.9N) -> col 2041, row 1469 at 0.1 deg; chunk 20_14 spans 20..30E, 50..60N.
        let chunks = build_chunks(
            2,
            "r1",
            0.1,
            [tile(2041, 1469), tile(2099, 1499), tile(2100, 1469)],
        );
        let ids: Vec<_> = chunks
            .iter()
            .map(|(summary, _)| summary.id.as_str())
            .collect();
        assert_eq!(ids, ["20_14", "21_14"]);
        let (summary, manifest) = &chunks[0];
        assert_eq!(summary.tile_count, 2);
        assert_eq!(summary.total_bytes, 20);
        assert_eq!(summary.bounds.lon_min, 20.0);
        assert_eq!(summary.bounds.lat_min, 50.0);
        assert_eq!(summary.manifest, "v2/releases/r1/chunks/20_14.json");
        assert_eq!(manifest.tiles.len(), 2);
    }

    #[test]
    fn groups_one_degree_tiles_into_ten_degree_chunks() {
        let chunks = build_chunks(2, "r1", 1.0, [tile(204, 146), tile(209, 149)]);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0.id, "20_14");
    }
}
