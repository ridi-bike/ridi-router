use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

use super::format::*;
use super::generator::manifest::TileManifest;
use super::io::MappedTile;

pub struct TileManager {
    tile_dir: PathBuf,
    manifest: TileManifest,
    loaded_tiles: HashMap<TileId, MappedTile>,
    tile_size_degrees: f32,
}

impl TileManager {
    const MAX_LOADED_TILES: usize = 100; // Conservative FD limit

    /// Initialize TileManager from directory containing manifest.json
    pub fn new(tile_dir: PathBuf) -> Result<Self> {
        let manifest_path = tile_dir.join("manifest.json");
        let manifest_file =
            std::fs::File::open(&manifest_path).context("Failed to open manifest.json")?;
        let manifest: TileManifest =
            serde_json::from_reader(manifest_file).context("Failed to parse manifest.json")?;

        let tile_size_degrees = manifest.tile_size_degrees;

        Ok(Self {
            tile_dir,
            manifest,
            loaded_tiles: HashMap::new(),
            tile_size_degrees,
        })
    }

    /// Create TileManager from manifest directly (for tests)
    #[cfg(test)]
    pub fn from_manifest(manifest: TileManifest, tile_dir: PathBuf) -> Self {
        let tile_size_degrees = manifest.tile_size_degrees;
        Self {
            tile_dir,
            manifest,
            loaded_tiles: HashMap::new(),
            tile_size_degrees,
        }
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
    pub fn get_point_by_id(&mut self, tile_id: TileId, osm_id: u64) -> Result<PointRecord> {
        self.ensure_tile_loaded(tile_id)?;

        let tile = self.loaded_tiles.get(&tile_id).unwrap();
        let points = tile.get_points()?;

        // Linear search for now (could optimize with binary search if sorted)
        for point in points {
            if point.osm_id == osm_id {
                return Ok(*point);
            }
        }

        anyhow::bail!("Point {} not found in tile {:?}", osm_id, tile_id)
    }

    /// Get line by index within a specific tile
    pub fn get_line_by_index(&mut self, tile_id: TileId, line_index: usize) -> Result<LineRecord> {
        self.ensure_tile_loaded(tile_id)?;

        let tile = self.loaded_tiles.get(&tile_id).unwrap();
        let lines = tile.get_lines()?;

        if line_index < lines.len() {
            Ok(lines[line_index])
        } else {
            anyhow::bail!(
                "Line index {} out of bounds in tile {:?}",
                line_index,
                tile_id
            )
        }
    }

    /// Get closest point to coordinates with filtering
    /// Returns (TileId, osm_id) tuple
    pub fn get_closest_to_coords(
        &mut self,
        lat: f32,
        lon: f32,
        _rules: &crate::router::rules::RouterRules,
        avoid_proximity_to_residential: bool,
        _limit_to_hw_tags: Option<&[&'static str]>,
    ) -> Result<Option<(TileId, u64)>> {
        // Determine which tile contains these coordinates
        let tile_id = TileId::from_coords(lat, lon);

        self.ensure_tile_loaded(tile_id)?;

        let tile = self.loaded_tiles.get(&tile_id).unwrap();
        let _spatial_index = tile.get_spatial_index()?;

        // Binary search for closest grid cell
        let _cell_id = GridCellEntry::encode_cell_id(lat, lon, 100);

        // For now: linear scan through spatial index
        // TODO: Implement expanding ring search like PointGrid
        // TODO: Implement rules filtering
        // TODO: Implement highway tag filtering
        let points = tile.get_points()?;

        let mut closest: Option<((TileId, u64), f32)> = None;

        for point in points {
            if point.lines_count == 0 {
                continue; // Skip disconnected points
            }

            // Apply proximity filter
            if avoid_proximity_to_residential && point.residential_in_proximity() {
                continue;
            }

            let dist = ((point.lat - lat).powi(2) + (point.lon - lon).powi(2)).sqrt();

            if let Some((_, min_dist)) = closest {
                if dist < min_dist {
                    closest = Some(((tile_id, point.osm_id), dist));
                }
            } else {
                closest = Some(((tile_id, point.osm_id), dist));
            }
        }

        Ok(closest.map(|(id_tuple, _)| id_tuple))
    }

    /// Get adjacent lines and points from a point (handles border crossing)
    /// Returns Vec<(line_tile_id, line_index, other_point_tile_id, other_point_osm_id)>
    pub fn get_adjacent_by_id(
        &mut self,
        tile_id: TileId,
        osm_id: u64,
    ) -> Result<Vec<(TileId, usize, TileId, u64)>> {
        self.ensure_tile_loaded(tile_id)?;

        // First, collect information from the current tile
        // We need to clone/copy the data to avoid holding references while loading other tiles
        let line_indices_and_data: Vec<(usize, u64, f32, f32, u64, f32, f32)> = {
            let tile = self.loaded_tiles.get(&tile_id).unwrap();
            let points = tile.get_points()?;

            // Find the point in the tile
            let point = points
                .iter()
                .find(|p| p.osm_id == osm_id)
                .context("Point not found in tile")?;

            let lines = tile.get_lines()?;
            let line_refs_array = tile.get_line_refs()?;

            // Get line references for this point
            let point_line_refs =
                &line_refs_array[point.lines_offset as usize..][..point.lines_count as usize];

            // Collect the data we need from each line
            point_line_refs
                .iter()
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

        for (
            line_idx,
            point_a_osm_id,
            point_a_lat,
            point_a_lon,
            point_b_osm_id,
            point_b_lat,
            point_b_lon,
        ) in line_indices_and_data
        {
            // Determine which endpoint is the "other" point
            let (other_osm_id, other_lat, other_lon) = if point_a_osm_id == osm_id {
                (point_b_osm_id, point_b_lat, point_b_lon)
            } else {
                (point_a_osm_id, point_a_lat, point_a_lon)
            };

            // Determine which tile contains the other point
            let other_tile_id = TileId::from_coords(other_lat, other_lon);

            // Check if we need to load a different tile
            if other_tile_id != tile_id {
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

            result.push((tile_id, line_idx, other_tile_id, other_osm_id));
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

    /// Get tag value string from tile
    pub fn get_tag_value(&mut self, tile_id: TileId, tag_value_idx: u32) -> Result<String> {
        self.ensure_tile_loaded(tile_id)?;

        let tile = self.loaded_tiles.get(&tile_id).unwrap();
        let value_str = tile.get_tag_value(tag_value_idx)?;

        Ok(value_str.to_string())
    }

    /// Get tag set record from tile
    pub fn get_tag_set_record(
        &mut self,
        tile_id: TileId,
        tag_set_idx: u32,
    ) -> Result<TagSetRecord> {
        self.ensure_tile_loaded(tile_id)?;

        let tile = self.loaded_tiles.get(&tile_id).unwrap();
        let tag_set = tile.get_tag_set(tag_set_idx)?;

        Ok(*tag_set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::rules::RouterRules;

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
        let point = manager.get_closest_to_coords(42.5, 18.5, &RouterRules::default(), false, None).unwrap();
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
        let point = manager.get_closest_to_coords(42.1, 18.9, &RouterRules::default(), false, None).unwrap().unwrap();

        // Get adjacent points (may cross tile boundary)
        let adjacent = manager.get_adjacent_by_id(point.0, point.1).unwrap();

        // Should have at least one adjacent point
        assert!(adjacent.len() > 0);

        // Check if any cross tile boundary
        let crosses_boundary = adjacent.iter().any(|(line_tile_id, _, other_tile_id, _)| *line_tile_id != *other_tile_id);
        // May or may not cross depending on location
        let _ = crosses_boundary;
    }

    #[test]
    fn test_missing_tile_handling() {
        // TODO: Test scenario where tile is missing
        // Should gracefully filter out lines leading to missing tile
    }
}
