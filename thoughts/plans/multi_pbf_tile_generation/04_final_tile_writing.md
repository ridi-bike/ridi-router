# Phase 4: Final Tile Writing

## Overview

Implement the `write_final_tiles()` method to merge intermediate tiles from all PBFs, deduplicate entities, and write the final unified RMDF tiles with a combined manifest.

This phase comes after overlap re-evaluation because:
- Node flags in overlap zones must be correct before final write
- All intermediate data must be complete in redb

## Changes Required

### 1. Implement write_final_tiles()

**File**: `src/rmdf/generator/mod.rs`

**Changes**: Replace placeholder implementation (lines 298-306) with actual logic.

**Algorithm**:
```
1. Discover all unique tile positions (col, row) from redb
2. For each tile position:
   a. Load tiles from all PBFs at this position
   b. Merge tiles using IntermediateTile::merge()
   c. Deduplicate using IntermediateTile::deduplicate()
   d. Build GenerationGraph from merged tile
   e. Write RMDF tile file
3. Generate manifest.json with all source filenames
```

### 2. Discover Unique Tile Positions

**File**: `src/rmdf/generator/intermediate.rs`

**Changes**: Add method to list all tile positions:

```rust
pub fn list_all_tile_positions(db: &redb::Database) -> Result<HashSet<(u16, u16)>> {
    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(TILE_NODES)?;
    
    let mut positions = HashSet::new();
    for result in table.iter()? {
        let (key, _) = result?;
        let (col, row, _) = key.value();
        positions.insert((col, row));
    }
    
    Ok(positions)
}
```

### 3. Load and Merge Tiles at Position

**File**: `src/rmdf/generator/mod.rs`

**Changes**: Add helper method:

```rust
fn load_and_merge_tile(
    &self,
    db: &redb::Database,
    col: u16,
    row: u16,
) -> Result<IntermediateTile> {
    // Load intermediate tile (may have data from multiple PBFs already merged by key)
    let merged = IntermediateTile::load_from_redb(db, col, row)?;
    
    // Deduplicate (ways/relations may have duplicates from multiple PBFs)
    merged.deduplicate();
    
    Ok(merged)
}
```

Note: Since `IntermediateTile::save_to_redb()` uses key `(col, row, osm_id)`, loading by (col, row) range already includes data from all PBFs. The `deduplicate()` handles any remaining duplicates.

### 4. Build GenerationGraph from Merged Tile

Reuse existing `PbfStreamer` logic or extract the graph building:

**Option A**: Extract `build_generation_graph()` from PbfStreamer into reusable function
**Option B**: Create `IntermediateTile::to_generation_graph()` method

The graph building logic is at `src/rmdf/generator/pbf_streamer.rs` around lines 350-450.

```rust
fn build_generation_graph(&self, tile: &IntermediateTile) -> Result<GenerationGraph> {
    let mut graph = GenerationGraph::new();
    
    // Add nodes
    for (osm_id, node) in &tile.nodes {
        graph.insert_node(*osm_id, node.clone());
    }
    
    // Add ways and edges
    for way in &tile.ways {
        graph.insert_way(way.clone());
    }
    
    // Add relations
    for relation in &tile.relations {
        graph.insert_relation(relation.clone());
    }
    
    Ok(graph)
}
```

### 5. Write RMDF Tile

Reuse existing RMDF writer logic from `PbfStreamer::write_rmdf_tile()`:

**File**: `src/rmdf/generator/pbf_streamer.rs` - extract or expose `write_rmdf_tile()`

Or use the `RmdfWriter` directly (check existing code for exact API).

### 6. Generate Manifest

**File**: `src/rmdf/generator/mod.rs`

**Changes**: Add manifest generation:

```rust
fn generate_manifest(
    &self,
    db: &redb::Database,
    tile_count: usize,
) -> Result<()> {
    // Load all source PBF filenames
    let source_files = GridStorage::load_all_filenames(db)?;
    
    // Calculate combined bounds from all PBFs
    let all_bounds = GridStorage::load_all_bounds(db)?;
    let combined_bounds = calculate_combined_bounds(&all_bounds);
    
    let manifest = Manifest {
        tile_size_degrees: self.tile_size_degrees,
        source_files,
        bounds: combined_bounds,
        tile_count,
    };
    
    let manifest_path = self.output_dir.join("manifest.json");
    let json = serde_json::to_string_pretty(&manifest)?;
    std::fs::write(&manifest_path, json)?;
    
    info!("Wrote manifest to {:?}", manifest_path);
    Ok(())
}
```

### 7. Add GridStorage Helper for Filenames

**File**: `src/rmdf/generator/intermediate.rs`

**Changes**: Add method to `GridStorage`:

```rust
pub fn load_all_filenames(db: &redb::Database) -> Result<Vec<String>> {
    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(PBF_FILES)?;
    
    let mut filenames = Vec::new();
    for result in table.iter()? {
        let (_, value) = result?;
        filenames.push(value.value().to_string());
    }
    
    // Sort for deterministic output
    filenames.sort();
    Ok(filenames)
}
```

### 8. Clean Up Intermediate Storage

After successful final write, clean up the redb database:

```rust
fn cleanup_intermediate_storage(&self) -> Result<()> {
    if self.db_path.exists() {
        std::fs::remove_file(&self.db_path)?;
        info!("Cleaned up intermediate storage: {:?}", self.db_path);
    }
    Ok(())
}
```

## Success Criteria

### Automated Verification:
 [x] Unit test: `list_all_tile_positions()` returns correct positions
 [x] Unit test: Merged tile contains nodes from multiple sources
 [x] Unit test: Deduplication removes duplicate ways/relations
 [x] Integration test: Two PBFs generate single set of RMDF tiles
 [x] Manifest validation: All source files listed, bounds are correct

### Manual Verification:
 [x] Process two overlapping PBFs
 [x] Verify no duplicate OSM IDs in any tile
 [x] Verify tile count matches expected (not double)
 [x] Verify manifest.json lists both source files
 [x] Verify combined bounds cover both PBFs

## Dependencies

- Depends on: Phase 3 (Overlap Re-evaluation)
- Blocks: Phase 5 (CLI Update)

## Risks & Mitigations

- **Risk**: Large merge operations may be slow
  - **Mitigation**: Process tiles in parallel if possible, or accept sequential for simplicity

- **Risk**: Out of memory for very large tiles with many entities
  - **Mitigation**: Process one tile at a time, release memory between tiles

- **Risk**: redb cleanup might fail on Windows (file locks)
  - **Mitigation**: Mark cleanup as optional, log warning if fails

## Notes

The `IntermediateTile::load_from_redb()` already loads all entities for a tile position, which includes data from all PBFs (since key includes osm_id). The `deduplicate()` method handles any duplicate ways/relations that came from multiple PBFs.

For nodes, HashMap already deduplicates by OSM ID (last-write-wins per the design decision).
