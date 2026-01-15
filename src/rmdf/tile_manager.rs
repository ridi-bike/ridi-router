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
    const MAX_LOADED_TILES: usize = 100;  // Conservative FD limit

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

    /// Get adjacent lines and points from a point (handles border crossing)
    pub fn get_adjacent(&mut self, point_ref: &PointRef) -> Result<Vec<(LineRef, PointRef)>> {
        self.ensure_tile_loaded(point_ref.tile_id)?;

        // First, collect information from the current tile
        // We need to clone/copy the data to avoid holding references while loading other tiles
        let line_indices_and_data: Vec<(usize, u64, f32, f32, u64, f32, f32)> = {
            let tile = self.loaded_tiles.get(&point_ref.tile_id).unwrap();
            let points = tile.get_points()?;

            // Find the point in the tile
            let point = points.iter()
                .find(|p| p.osm_id == point_ref.osm_id)
                .context("Point not found in tile")?;

            let lines = tile.get_lines()?;
            let line_refs_array = tile.get_line_refs()?;

            // Get line references for this point
            let point_line_refs = &line_refs_array[point.lines_offset as usize..]
                [..point.lines_count as usize];

            // Collect the data we need from each line
            point_line_refs.iter()
                .map(|&line_idx| {
                    let line = &lines[line_idx as usize];
                    (
                        line_idx as usize,
                        line.point_a_osm_id,
                        line.point_a_lat,
                        line.point_a_lon,
                        line.point_b_osm_id,
                        line.point_b_lat,
                        line.point_b_lon,
                    )
                })
                .collect()
        }; // Drop all tile references here

        let mut result = Vec::new();

        for (line_idx, point_a_osm_id, point_a_lat, point_a_lon, point_b_osm_id, point_b_lat, point_b_lon) in line_indices_and_data {
            // Determine which endpoint is the "other" point
            let (other_osm_id, other_lat, other_lon) = if point_a_osm_id == point_ref.osm_id {
                (point_b_osm_id, point_b_lat, point_b_lon)
            } else {
                (point_a_osm_id, point_a_lat, point_a_lon)
            };

            // Determine which tile contains the other point
            let other_tile_id = TileId::from_coords(other_lat, other_lon);

            // Check if we need to load a different tile
            if other_tile_id != point_ref.tile_id {
                // Border crossing detected
                match self.ensure_tile_loaded(other_tile_id) {
                    Ok(_) => {
                        // Tile loaded successfully
                    }
                    Err(_) => {
                        // Tile missing - skip this line (dead-end)
                        tracing::warn!(
                            "Tile {:?} not available, treating line as dead-end",
                            other_tile_id
                        );
                        continue;
                    }
                }
            }

            // Create references
            let line_ref = LineRef {
                tile_id: point_ref.tile_id,
                line_index: line_idx,
            };

            let other_point_ref = PointRef {
                tile_id: other_tile_id,
                osm_id: other_osm_id,
                lat: other_lat,
                lon: other_lon,
            };

            result.push((line_ref, other_point_ref));
        }

        // Evict old tiles if needed
        self.evict_if_needed()?;

        Ok(result)
    }

    /// Evict least recently used tiles if over limit
    fn evict_if_needed(&mut self) -> Result<()> {
        if self.loaded_tiles.len() <= Self::MAX_LOADED_TILES {
            return Ok(());
        }

        // Simple strategy: remove arbitrary tile
        // TODO: Implement proper LRU tracking
        if let Some(tile_id) = self.loaded_tiles.keys().next().cloned() {
            self.loaded_tiles.remove(&tile_id);
            tracing::debug!("Evicted tile {:?}", tile_id);
        }

        Ok(())
    }

    /// Get line data (for routing algorithms)
    pub fn get_line(&mut self, line_ref: &LineRef) -> Result<LineRecord> {
        self.ensure_tile_loaded(line_ref.tile_id)?;

        let tile = self.loaded_tiles.get(&line_ref.tile_id).unwrap();
        let lines = tile.get_lines()?;

        Ok(lines[line_ref.line_index])
    }

    /// Get point data (for routing algorithms)
    pub fn get_point_data(&mut self, point_ref: &PointRef) -> Result<PointRecord> {
        self.ensure_tile_loaded(point_ref.tile_id)?;

        let tile = self.loaded_tiles.get(&point_ref.tile_id).unwrap();
        let points = tile.get_points()?;

        let point = points.iter()
            .find(|p| p.osm_id == point_ref.osm_id)
            .context("Point not found")?;

        Ok(*point)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_manifest() {
        // Assumes Montenegro tiles generated in test setup
        let tile_dir = PathBuf::from("test_data/montenegro_tiles");

        // Skip if test data doesn't exist
        if !tile_dir.exists() {
            eprintln!("Skipping test: test_data/montenegro_tiles doesn't exist");
            return;
        }

        let manager = TileManager::new(tile_dir).expect("Failed to load TileManager");

        assert!(manager.manifest.tiles.len() > 0);
    }

    #[test]
    fn test_single_tile_query() {
        let tile_dir = PathBuf::from("test_data/montenegro_tiles");

        // Skip if test data doesn't exist
        if !tile_dir.exists() {
            eprintln!("Skipping test: test_data/montenegro_tiles doesn't exist");
            return;
        }

        let mut manager = TileManager::new(tile_dir).unwrap();

        // Query for a point known to exist in Montenegro
        let point = manager.get_closest_to_coords(42.5, 18.5).unwrap();
        assert!(point.is_some());
    }

    #[test]
    fn test_cross_tile_traversal() {
        let tile_dir = PathBuf::from("test_data/montenegro_tiles");

        // Skip if test data doesn't exist
        if !tile_dir.exists() {
            eprintln!("Skipping test: test_data/montenegro_tiles doesn't exist");
            return;
        }

        let mut manager = TileManager::new(tile_dir).unwrap();

        // Get a point near a tile boundary
        let point = manager.get_closest_to_coords(42.1, 18.9).unwrap().unwrap();

        // Get adjacent points (may cross tile boundary)
        let adjacent = manager.get_adjacent(&point).unwrap();

        // Should have at least one adjacent point
        assert!(adjacent.len() > 0);

        // Check if any cross tile boundary
        let crosses_boundary = adjacent.iter().any(|(_, p)| p.tile_id != point.tile_id);
        // May or may not cross depending on location
        let _ = crosses_boundary;
    }

    #[test]
    fn test_missing_tile_handling() {
        // TODO: Test scenario where tile is missing
        // Should gracefully filter out lines leading to missing tile
    }
}
