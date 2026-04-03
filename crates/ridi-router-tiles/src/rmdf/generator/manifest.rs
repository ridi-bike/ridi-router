use anyhow::Result;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use crate::rmdf::format::{RmdfHeader, TileId};
pub use ridi_router_common::manifest::{
    military_geojson_filename, TileBounds, TileManifest, TileMetadata, TileNeighbors,
};

#[cfg(feature = "debug-polygons")]
fn discover_military_geojson_filename(output_dir: &Path, tile_id: TileId) -> Option<String> {
    let candidate = military_geojson_filename(tile_id);
    output_dir.join(&candidate).exists().then_some(candidate)
}

#[cfg(not(feature = "debug-polygons"))]
fn discover_military_geojson_filename(_output_dir: &Path, _tile_id: TileId) -> Option<String> {
    None
}

pub struct ManifestGenerator {
    tile_size_degrees: f32,
}

impl ManifestGenerator {
    pub fn new(tile_size_degrees: f32) -> Self {
        Self { tile_size_degrees }
    }

    fn parse_tile_id_from_filename(path: &Path) -> Option<TileId> {
        let filename = path.file_stem()?.to_str()?;
        let parts: Vec<&str> = filename.split('_').collect();
        if parts.len() == 3 && parts[0] == "tile" {
            let col = parts[1].parse().ok()?;
            let row = parts[2].parse().ok()?;
            return Some(TileId { col, row });
        }

        None
    }

    pub fn discover_tiles(&self, output_dir: &Path) -> Result<Vec<TileId>> {
        let mut tile_ids = Vec::new();

        for entry in std::fs::read_dir(output_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("rmdf") {
                if let Some(tile_id) = Self::parse_tile_id_from_filename(&path) {
                    tile_ids.push(tile_id);
                }
            }
        }

        tile_ids.sort_by_key(|id| (id.col, id.row));
        tracing::info!("Discovered {} tiles for manifest", tile_ids.len());

        Ok(tile_ids)
    }

    pub fn generate(
        &self,
        output_dir: &Path,
        tile_ids: &[TileId],
        source_file: &str,
    ) -> Result<TileManifest> {
        let mut tiles = Vec::new();

        for tile_id in tile_ids {
            let filename = tile_id.to_filename();
            let filepath = output_dir.join(&filename);
            let metadata = std::fs::metadata(&filepath)?;
            let size_bytes = metadata.len();

            let file = File::open(&filepath)?;
            let mmap = unsafe { memmap2::Mmap::map(&file)? };
            let header: &RmdfHeader = bytemuck::from_bytes(&mmap[0..RmdfHeader::SIZE]);

            let checksum_bytes = &mmap[mmap.len() - 32..];
            let checksum = format!("sha256:{}", hex::encode(checksum_bytes));

            let neighbors = self.compute_neighbors(*tile_id, tile_ids);
            let military_geojson_filename =
                discover_military_geojson_filename(output_dir, *tile_id);

            tiles.push(TileMetadata {
                filename,
                col: tile_id.col,
                row: tile_id.row,
                bounds: TileBounds {
                    lat_min: header.tile_bounds.lat_min,
                    lat_max: header.tile_bounds.lat_max,
                    lon_min: header.tile_bounds.lon_min,
                    lon_max: header.tile_bounds.lon_max,
                },
                neighbors,
                size_bytes,
                point_count: header.point_count,
                line_count: header.line_count,
                checksum,
                military_geojson_filename,
            });
        }

        let manifest = TileManifest {
            version: env!("CARGO_PKG_VERSION").to_string(),
            tile_size_degrees: self.tile_size_degrees,
            format_version: 1,
            generated_at: chrono::Utc::now().to_rfc3339(),
            source_files: vec![source_file.to_string()],
            tiles,
        };

        let manifest_path = output_dir.join("manifest.json");
        let file = File::create(manifest_path)?;
        serde_json::to_writer_pretty(file, &manifest)?;

        Ok(manifest)
    }

    fn compute_neighbors(&self, tile_id: TileId, all_tiles: &[TileId]) -> TileNeighbors {
        let tile_set: HashMap<(u16, u16), TileId> =
            all_tiles.iter().map(|t| ((t.col, t.row), *t)).collect();

        let get_neighbor = |col: i32, row: i32| -> Option<String> {
            if !(0..=359).contains(&col) || !(0..=179).contains(&row) {
                return None;
            }

            tile_set
                .get(&(col as u16, row as u16))
                .map(|t| t.to_filename())
        };

        TileNeighbors {
            north: get_neighbor(tile_id.col as i32, tile_id.row as i32 + 1),
            south: get_neighbor(tile_id.col as i32, tile_id.row as i32 - 1),
            east: get_neighbor(tile_id.col as i32 + 1, tile_id.row as i32),
            west: get_neighbor(tile_id.col as i32 - 1, tile_id.row as i32),
            northeast: get_neighbor(tile_id.col as i32 + 1, tile_id.row as i32 + 1),
            northwest: get_neighbor(tile_id.col as i32 - 1, tile_id.row as i32 + 1),
            southeast: get_neighbor(tile_id.col as i32 + 1, tile_id.row as i32 - 1),
            southwest: get_neighbor(tile_id.col as i32 - 1, tile_id.row as i32 - 1),
        }
    }
}
