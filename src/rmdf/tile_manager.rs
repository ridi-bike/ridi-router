use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

use super::format::*;
use super::io::MappedTile;
use super::generator::manifest::TileManifest;

pub struct TileManager {
    tile_dir: PathBuf,
    manifest: TileManifest,
    loaded_tiles: HashMap<TileId, MappedTile>,
    tile_size_degrees: f32,
}

impl TileManager {
    /// Initialize TileManager from directory containing manifest.json
    pub fn new(tile_dir: PathBuf) -> Result<Self> {
        let manifest_path = tile_dir.join("manifest.json");
        let manifest_file = std::fs::File::open(&manifest_path)
            .context("Failed to open manifest.json")?;
        let manifest: TileManifest = serde_json::from_reader(manifest_file)
            .context("Failed to parse manifest.json")?;

        let tile_size_degrees = manifest.tile_size_degrees;

        Ok(Self {
            tile_dir,
            manifest,
            loaded_tiles: HashMap::new(),
            tile_size_degrees,
        })
    }

    /// Ensure tile is loaded (load if not already)
    fn ensure_tile_loaded(&mut self, tile_id: TileId) -> Result<()> {
        if self.loaded_tiles.contains_key(&tile_id) {
            return Ok(());
        }

        let filename = tile_id.to_filename();
        let filepath = self.tile_dir.join(&filename);

        let mapped_tile = MappedTile::load(&filepath)
            .with_context(|| format!("Failed to load tile: {:?}", filename))?;

        self.loaded_tiles.insert(tile_id, mapped_tile);

        Ok(())
    }

    /// Get point by OSM ID within a specific tile
    pub fn get_point(&mut self, tile_id: TileId, osm_id: u64) -> Result<Option<PointRef>> {
        self.ensure_tile_loaded(tile_id)?;

        let tile = self.loaded_tiles.get(&tile_id).unwrap();
        let points = tile.get_points()?;

        // Linear search for now (could optimize with binary search if sorted)
        for point in points {
            if point.osm_id == osm_id {
                return Ok(Some(PointRef {
                    tile_id,
                    osm_id,
                    lat: point.lat,
                    lon: point.lon,
                }));
            }
        }

        Ok(None)
    }

    /// Get closest point to coordinates (single-tile version)
    pub fn get_closest_to_coords(&mut self, lat: f32, lon: f32) -> Result<Option<PointRef>> {
        // Determine which tile contains these coordinates
        let tile_id = TileId::from_coords(lat, lon);

        self.ensure_tile_loaded(tile_id)?;

        let tile = self.loaded_tiles.get(&tile_id).unwrap();
        let _spatial_index = tile.get_spatial_index()?;

        // Binary search for closest grid cell
        let _cell_id = GridCellEntry::encode_cell_id(lat, lon, 100);

        // For now: linear scan through spatial index
        // TODO: Implement expanding ring search like PointGrid
        let points = tile.get_points()?;

        let mut closest: Option<(PointRef, f32)> = None;

        for point in points {
            if point.lines_count == 0 {
                continue; // Skip disconnected points
            }

            let dist = ((point.lat - lat).powi(2) + (point.lon - lon).powi(2)).sqrt();

            if let Some((_, min_dist)) = closest {
                if dist < min_dist {
                    closest = Some((
                        PointRef {
                            tile_id,
                            osm_id: point.osm_id,
                            lat: point.lat,
                            lon: point.lon,
                        },
                        dist,
                    ));
                }
            } else {
                closest = Some((
                    PointRef {
                        tile_id,
                        osm_id: point.osm_id,
                        lat: point.lat,
                        lon: point.lon,
                    },
                    dist,
                ));
            }
        }

        Ok(closest.map(|(point_ref, _)| point_ref))
    }
}

/// Reference to a point (includes tile context)
#[derive(Debug, Clone, PartialEq)]
pub struct PointRef {
    pub tile_id: TileId,
    pub osm_id: u64,
    pub lat: f32,
    pub lon: f32,
}

/// Reference to a line (includes tile context)
#[derive(Debug, Clone, PartialEq)]
pub struct LineRef {
    pub tile_id: TileId,
    pub line_index: usize,  // Index within tile's lines array
}
