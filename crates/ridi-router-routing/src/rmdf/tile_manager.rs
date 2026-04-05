const MAX_LOADED_TILES: usize = 100; // Conservative FD limit

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

use super::format::*;
use super::generator::manifest::TileManifest;
use super::io::MappedTile;

pub struct TileManager {
    tile_dir: PathBuf,
    loaded_tiles: HashMap<TileId, MappedTile>,
    loaded_tile_order: [TileId; MAX_LOADED_TILES],
    loaded_tile_order_len: usize,
    #[cfg(test)]
    loaded_tiles_limit: usize,
    tile_size_degrees: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TilePointRule {
    pub from_line_indices: Vec<u64>,
    pub to_line_indices: Vec<u64>,
    pub rule_type: u8,
}

impl TileManager {
    /// Initialize TileManager from directory containing manifest.json
    pub fn new(tile_dir: PathBuf) -> Result<Self> {
        let manifest_path = tile_dir.join("manifest.json");
        let manifest_file =
            std::fs::File::open(&manifest_path).context("Failed to open manifest.json")?;
        let manifest: TileManifest =
            serde_json::from_reader(manifest_file).context("Failed to parse manifest.json")?;

        Ok(Self::from_manifest(manifest, tile_dir))
    }

    /// Create TileManager from manifest directly
    pub fn from_manifest(manifest: TileManifest, tile_dir: PathBuf) -> Self {
        let tile_size_degrees = manifest.tile_size_degrees;
        let manager = Self {
            tile_dir,
            loaded_tiles: HashMap::new(),
            loaded_tile_order: [TileId::default(); MAX_LOADED_TILES],
            loaded_tile_order_len: 0,
            #[cfg(test)]
            loaded_tiles_limit: MAX_LOADED_TILES,
            tile_size_degrees,
        };
        manager.debug_assert_registry_invariants();
        manager
    }

    #[cfg(test)]
    pub(crate) fn from_manifest_with_loaded_tiles_limit(
        manifest: TileManifest,
        tile_dir: PathBuf,
        loaded_tiles_limit: usize,
    ) -> Self {
        assert!(
            loaded_tiles_limit > 0,
            "test loaded tiles limit must be positive"
        );
        assert!(
            loaded_tiles_limit <= MAX_LOADED_TILES,
            "test loaded tiles limit {} exceeds order capacity {}",
            loaded_tiles_limit,
            MAX_LOADED_TILES
        );

        let mut manager = Self::from_manifest(manifest, tile_dir);
        manager.loaded_tiles_limit = loaded_tiles_limit;
        manager
    }

    #[cfg(test)]
    fn loaded_tiles_limit(&self) -> usize {
        self.loaded_tiles_limit
    }

    #[cfg(not(test))]
    fn loaded_tiles_limit(&self) -> usize {
        MAX_LOADED_TILES
    }

    /// Compute TileId for given coordinates
    pub fn tile_id_for_coords(&self, lat: f32, lon: f32) -> TileId {
        TileId::from_coords(lat, lon, self.tile_size_degrees)
    }

    fn debug_assert_registry_invariants(&self) {
        debug_assert_eq!(
            self.loaded_tiles.len(),
            self.loaded_tile_order_len,
            "loaded tile registry order length mismatch"
        );

        let active_order = &self.loaded_tile_order[..self.loaded_tile_order_len];
        let mut seen = std::collections::HashSet::with_capacity(active_order.len());

        for &loaded_tile_id in active_order {
            debug_assert!(
                seen.insert(loaded_tile_id),
                "loaded tile registry order contains duplicates: {:?}",
                loaded_tile_id
            );
            debug_assert!(
                self.loaded_tiles.contains_key(&loaded_tile_id),
                "loaded tile registry order contains unloaded tile: {:?}",
                loaded_tile_id
            );
        }

        debug_assert_eq!(
            seen.len(),
            self.loaded_tiles.len(),
            "loaded tile registry order does not cover all loaded tiles"
        );
    }

    fn append_loaded_tile(&mut self, tile_id: TileId) {
        debug_assert_eq!(
            self.loaded_tiles.len(),
            self.loaded_tile_order_len + 1,
            "loaded tile registry map/order mismatch before append"
        );
        debug_assert!(
            self.loaded_tiles.contains_key(&tile_id),
            "appending tile {:?} that is not loaded",
            tile_id
        );
        debug_assert!(
            self.loaded_tile_order_len < self.loaded_tile_order.len(),
            "loaded tile registry order overflow before unload"
        );
        debug_assert!(
            !self.loaded_tile_order[..self.loaded_tile_order_len].contains(&tile_id),
            "tile {:?} already present in loaded tile registry order",
            tile_id
        );

        self.loaded_tile_order[self.loaded_tile_order_len] = tile_id;
        self.loaded_tile_order_len += 1;
        self.debug_assert_registry_invariants();
    }

    fn mark_tile_recent(&mut self, tile_id: TileId) {
        self.debug_assert_registry_invariants();
        let active_len = self.loaded_tile_order_len;
        let Some(index) = self.loaded_tile_order[..active_len]
            .iter()
            .rposition(|loaded_tile_id| *loaded_tile_id == tile_id)
        else {
            panic!(
                "loaded tile registry order missing tile {:?} during recency update",
                tile_id
            );
        };

        if index + 1 < active_len {
            self.loaded_tile_order[index..active_len].rotate_left(1);
        }
        self.debug_assert_registry_invariants();
    }

    fn unload_if_needed(&mut self) -> Result<()> {
        self.debug_assert_registry_invariants();

        if self.loaded_tiles.len() <= self.loaded_tiles_limit() {
            return Ok(());
        }

        let tile_id = self.loaded_tile_order[0];
        self.loaded_tiles.remove(&tile_id);
        self.loaded_tile_order[..self.loaded_tile_order_len].rotate_left(1);
        self.loaded_tile_order[self.loaded_tile_order_len - 1] = TileId::default();
        self.loaded_tile_order_len -= 1;
        tracing::debug!("Unloaded tile {:?}", tile_id);

        self.debug_assert_registry_invariants();
        Ok(())
    }

    /// Ensure tile is loaded (load if not already)
    fn ensure_tile_loaded(&mut self, tile_id: TileId) -> Result<()> {
        self.debug_assert_registry_invariants();

        if self.loaded_tiles.contains_key(&tile_id) {
            self.mark_tile_recent(tile_id);
            self.debug_assert_registry_invariants();
            return Ok(());
        }

        let filename = tile_id.to_filename();
        let filepath = self.tile_dir.join(&filename);

        let mapped_tile = MappedTile::load(&filepath)
            .with_context(|| format!("Failed to load tile: {:?}", filename))?;

        self.loaded_tiles.insert(tile_id, mapped_tile);
        self.append_loaded_tile(tile_id);
        self.unload_if_needed()?;
        self.debug_assert_registry_invariants();

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

    pub(crate) fn get_rules_for_point(
        &mut self,
        tile_id: TileId,
        point: &PointRecord,
    ) -> Result<Vec<TilePointRule>> {
        self.ensure_tile_loaded(tile_id)?;

        let tile = self.loaded_tiles.get(&tile_id).unwrap();
        let rule_records = tile.get_rules()?;
        let payload = tile.get_rule_line_refs_payload()?;
        let (rules_start, rules_end) =
            Self::checked_range(point.rules_offset, point.rules_count, rule_records.len())
                .with_context(|| {
                    format!(
                        "Rule slice for point {} is out of bounds in tile {:?}",
                        point.osm_id, tile_id
                    )
                })?;

        rule_records[rules_start..rules_end]
            .iter()
            .map(|rule_record| {
                let from_line_indices = Self::slice_rule_line_refs(
                    &payload,
                    rule_record.from_lines_offset,
                    rule_record.from_lines_count,
                )
                .with_context(|| {
                    format!(
                        "from-lines slice for point {} rule is out of bounds in tile {:?}",
                        point.osm_id, tile_id
                    )
                })?;
                let to_line_indices = Self::slice_rule_line_refs(
                    &payload,
                    rule_record.to_lines_offset,
                    rule_record.to_lines_count,
                )
                .with_context(|| {
                    format!(
                        "to-lines slice for point {} rule is out of bounds in tile {:?}",
                        point.osm_id, tile_id
                    )
                })?;

                Ok(TilePointRule {
                    from_line_indices,
                    to_line_indices,
                    rule_type: rule_record.rule_type,
                })
            })
            .collect()
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
        let tile_id = TileId::from_coords(lat, lon, self.tile_size_degrees);

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
            let other_tile_id = TileId::from_coords(other_lat, other_lon, self.tile_size_degrees);

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

        Ok(result)
    }

    fn checked_range(offset: u64, count: u32, len: usize) -> Result<(usize, usize)> {
        let start = usize::try_from(offset).context("offset must fit in usize")?;
        let count = usize::try_from(count).context("count must fit in usize")?;
        let end = start.checked_add(count).context("slice end overflow")?;

        if end > len {
            anyhow::bail!("slice {}..{} exceeds length {}", start, end, len);
        }

        Ok((start, end))
    }

    fn slice_rule_line_refs(payload: &[u64], offset: u64, count: u32) -> Result<Vec<u64>> {
        let (start, end) = Self::checked_range(offset, count, payload.len())?;
        Ok(payload[start..end].to_vec())
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

    #[cfg(test)]
    pub(crate) fn loaded_tile_count(&self) -> usize {
        self.loaded_tiles.len()
    }

    #[cfg(test)]
    pub(crate) fn is_tile_loaded(&self, tile_id: TileId) -> bool {
        self.loaded_tiles.contains_key(&tile_id)
    }

    #[cfg(test)]
    pub(crate) fn loaded_tile_order_for_test(&self) -> Vec<TileId> {
        self.loaded_tile_order[..self.loaded_tile_order_len].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::rules::RouterRules;
    use ridi_router_test_support::rmdf::create_missing_neighbor_fixture;
    fn test_manifest() -> TileManifest {
        TileManifest {
            version: "test".to_string(),
            tile_size_degrees: 1.0,
            format_version: 1,
            generated_at: "2026-04-05T00:00:00Z".to_string(),
            source_files: Vec::new(),
            tiles: Vec::new(),
        }
    }

    #[test]
    fn test_load_manifest() {
        // Assumes Montenegro tiles generated in test setup
        let tile_dir = PathBuf::from("test_data/montenegro_tiles");

        // Skip if test data doesn't exist
        if !tile_dir.exists() {
            eprintln!("Skipping test: test_data/montenegro_tiles doesn't exist");
            return;
        }

        let _manager = TileManager::new(tile_dir).expect("Failed to load TileManager");
    }

    #[test]
    fn test_registry_foundation_initializes_test_hooks() {
        let manager = TileManager::from_manifest_with_loaded_tiles_limit(
            test_manifest(),
            PathBuf::from("test"),
            3,
        );

        assert_eq!(manager.loaded_tile_count(), 0);
        assert!(!manager.is_tile_loaded(TileId { col: 1, row: 2 }));
        assert_eq!(manager.loaded_tile_order_for_test(), Vec::<TileId>::new());
        assert_eq!(manager.loaded_tiles_limit(), 3);
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
        let point = manager
            .get_closest_to_coords(42.5, 18.5, &RouterRules::default(), false, None)
            .unwrap();
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
        let point = manager
            .get_closest_to_coords(42.1, 18.9, &RouterRules::default(), false, None)
            .unwrap()
            .unwrap();

        // Get adjacent points (may cross tile boundary)
        let adjacent = manager.get_adjacent_by_id(point.0, point.1).unwrap();

        // Should have at least one adjacent point
        assert!(adjacent.len() > 0);

        // Check if any cross tile boundary
        let crosses_boundary = adjacent
            .iter()
            .any(|(line_tile_id, _, other_tile_id, _)| *line_tile_id != *other_tile_id);
        // May or may not cross depending on location
        let _ = crosses_boundary;
    }

    #[test]
    fn test_missing_tile_handling() {
        let fixture = create_missing_neighbor_fixture("tile-manager-missing-neighbor");
        let fixture_dir = fixture.dir.clone();

        let mut manager = TileManager::new(fixture.dir.clone()).unwrap();
        let adjacent = manager.get_adjacent_by_id(fixture.tile_a, fixture.center_osm_id);

        assert!(
            adjacent.is_ok(),
            "missing-neighbor adjacency lookup should succeed: {adjacent:?}",
        );

        let adjacent = adjacent.unwrap();
        assert_eq!(
            adjacent.len(),
            1,
            "missing tile edge should be filtered out"
        );
        assert!(adjacent.contains(&(
            fixture.tile_a,
            0,
            fixture.tile_a,
            fixture.in_tile_neighbor_osm_id,
        )));
        assert!(!adjacent.iter().any(|(_, _, other_tile_id, other_osm_id)| {
            *other_tile_id == fixture.missing_tile
                || *other_osm_id == fixture.missing_neighbor_osm_id
        }));

        std::fs::remove_dir_all(fixture_dir).unwrap();
    }
}
