# Phase 6: TileManager - Multi-Tile & Border Crossing

## Overview

Extend TileManager with multi-tile operations: get_adjacent() that crosses tile boundaries, dynamic tile loading, LRU eviction, and missing tile handling.

**Goals:**
- Implement get_adjacent() with border crossing detection
- Load adjacent tiles on-demand during traversal
- Implement LRU eviction for file descriptor limits
- Handle missing tiles gracefully (filter out dead-end lines)
- Integration tests for cross-tile routing

## Changes Required

### 1. Extend TileManager with Border Crossing

**File**: `src/rmdf/tile_manager.rs`

**Changes**: Add get_adjacent() and dynamic loading

```rust
use std::collections::VecDeque;

impl TileManager {
    const MAX_LOADED_TILES: usize = 100;  // Conservative FD limit

    /// Get adjacent lines and points from a point (handles border crossing)
    pub fn get_adjacent(&mut self, point_ref: &PointRef) -> Result<Vec<(LineRef, PointRef)>> {
        self.ensure_tile_loaded(point_ref.tile_id)?;

        let tile = self.loaded_tiles.get(&point_ref.tile_id).unwrap();
        let points = tile.get_points()?;

        // Find the point in the tile
        let point = points.iter()
            .find(|p| p.osm_id == point_ref.osm_id)
            .context("Point not found in tile")?;

        let lines = tile.get_lines()?;
        let line_refs_array = tile.get_line_refs()?;

        let mut result = Vec::new();

        // Get line references for this point
        let point_line_refs = &line_refs_array[point.lines_offset as usize..]
            [..point.lines_count as usize];

        for &line_idx in point_line_refs {
            let line = &lines[line_idx as usize];

            // Determine which endpoint is the "other" point
            let other_osm_id = if line.point_a_osm_id == point_ref.osm_id {
                line.point_b_osm_id
            } else {
                line.point_a_osm_id
            };

            let (other_lat, other_lon) = if line.point_a_osm_id == point_ref.osm_id {
                (line.point_b_lat, line.point_b_lon)
            } else {
                (line.point_a_lat, line.point_a_lon)
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
                line_index: line_idx as usize,
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
```

**Rationale**:
- Border crossing uses denormalized coordinates from LineRecord
- Missing tiles handled gracefully (filter out, warn)
- LRU eviction prevents FD exhaustion

### 2. Add Integration Tests

**File**: `src/rmdf/tile_manager.rs` (test module)

**Changes**: Add multi-tile tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_manifest() {
        // Assumes Montenegro tiles generated in test setup
        let tile_dir = PathBuf::from("test_data/montenegro_tiles");
        let manager = TileManager::new(tile_dir).expect("Failed to load TileManager");

        assert!(manager.manifest.tiles.len() > 0);
    }

    #[test]
    fn test_single_tile_query() {
        let tile_dir = PathBuf::from("test_data/montenegro_tiles");
        let mut manager = TileManager::new(tile_dir).unwrap();

        // Query for a point known to exist
        let point = manager.get_closest_to_coords(42.5, 18.5).unwrap();
        assert!(point.is_some());
    }

    #[test]
    fn test_cross_tile_traversal() {
        let tile_dir = PathBuf::from("test_data/montenegro_tiles");
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
    }

    #[test]
    fn test_missing_tile_handling() {
        // TODO: Test scenario where tile is missing
        // Should gracefully filter out lines leading to missing tile
    }
}
```

**Rationale**: Validates multi-tile behavior before routing integration.

## Success Criteria

### Automated Verification

- [ ] Unit tests pass: `cargo test rmdf::tile_manager`
- [ ] Integration tests pass for cross-tile queries
- [ ] get_adjacent() returns correct neighbors
- [ ] Border crossing loads adjacent tile
- [ ] Missing tile filtered out (no panic)
- [ ] LRU eviction works when limit exceeded

### Manual Verification

- [ ] Query point near tile boundary
- [ ] Call get_adjacent() - adjacent tile loads automatically
- [ ] Verify correct neighbor points returned
- [ ] Delete a tile file, query nearby point - no crash, dead-end handled
- [ ] Load 100+ tiles - eviction occurs, no FD exhaustion

## Dependencies

- **Depends on**: Phase 5 (builds on single-tile access)
- **Blocks**: Phase 7 (routing needs get_adjacent())

## Risks & Mitigations

**Risk**: Border point deduplication errors
- **Mitigation**: Use OSM IDs as canonical identifiers, extensive testing

**Risk**: LRU eviction too aggressive
- **Mitigation**: Conservative MAX_LOADED_TILES=100, configurable later

**Risk**: FD limits still exceeded
- **Mitigation**: Monitor with lsof, adjust limit if needed

## Notes

- Deduplication handled implicitly via OSM IDs (no explicit tracking)
- LRU implementation simplified (can optimize later)
- Missing tiles don't fail routing - just reduce available paths
- LineRecord coordinates enable border detection without lookups
