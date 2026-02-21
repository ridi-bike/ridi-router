# Phase 2: Intermediate Storage

## Overview

Complete the `process_single_pbf()` method to:
1. Use the new grid exposure method to get actual grid bytes
2. Store the serialized grid to redb (not empty slice)
3. Extract tile data and save to intermediate redb storage (not RMDF files)

This phase enables the multi-PBF workflow by storing intermediate data for later overlap re-evaluation and merging.

## Changes Required

### 1. Update process_single_pbf() to Use Grid Exposure Method

**File**: `src/rmdf/generator/mod.rs`

**Changes**: Update `process_single_pbf()` method (lines 205-241).

Replace:
```rust
let pbf_data = InMemoryPbf::from_pbf_file_with_flags(pbf_path)
```

With:
```rust
let (pbf_data, proximity_grid) = InMemoryPbf::from_pbf_file_with_grid(pbf_path)
```

### 2. Store Actual Grid Bytes

**File**: `src/rmdf/generator/mod.rs`

**Changes**: Update `GridStorage::store_grid()` call (around line 231).

Replace:
```rust
GridStorage::store_grid(db, pbf_id, pbf_path.to_str().unwrap_or(""), &[], &grid_bounds)?;
```

With:
```rust
let grid_bytes = proximity_grid.to_bytes();
GridStorage::store_grid(db, pbf_id, pbf_path.to_str().unwrap_or(""), &grid_bytes, &grid_bounds)?;
```

### 3. Add Intermediate Tile Storage Method

**File**: `src/rmdf/generator/mod.rs` or `src/rmdf/generator/intermediate.rs`

**Changes**: Add method to extract tiles and save to redb instead of RMDF.

The current flow in `PbfStreamer::process_tile()`:
1. `extract_tile_data()` - get nodes/ways/relations
2. `build_generation_graph()` - convert to graph
3. `write_rmdf_tile()` - write RMDF file

For intermediate storage, we need:
1. `extract_tile_data()` - get nodes/ways/relations
2. Create `IntermediateTile` from extracted data
3. `IntermediateTile::save_to_redb()` - store to redb

**Option A**: Add method `save_tiles_to_redb()` to `MultiPbfGenerator` that:
- Calculates tile positions from PBF bounds
- For each tile: extracts data, creates `IntermediateTile`, saves to redb

**Option B**: Add mode flag to `PbfStreamer` that switches between RMDF output and redb output.

Recommendation: **Option A** - cleaner separation, doesn't modify `PbfStreamer`.

### 4. Implement save_tiles_to_redb() Method

**File**: `src/rmdf/generator/mod.rs`

**Changes**: Add private method to `MultiPbfGenerator`:

```rust
fn save_tiles_to_redb(&self, db: &redb::Database, pbf_data: &InMemoryPbf) -> Result<()> {
    // Calculate tiles that intersect with PBF bounds
    let tiles = self.calculate_all_tiles(pbf_data.bounds);
    
    // Process each tile
    for tile_id in tiles {
        // Extract data for this tile
        let (nodes, ways, relations) = self.extract_tile_data(pbf_data, tile_id);
        
        // Create intermediate tile
        let intermediate = IntermediateTile {
            tile_id,
            nodes,
            ways,
            relations,
        };
        
        // Save to redb
        intermediate.save_to_redb(db)?;
    }
    
    Ok(())
}
```

This method mirrors `PbfStreamer::process_tile()` but saves to redb instead of RMDF.

### 5. Update process_single_pbf() Flow

**File**: `src/rmdf/generator/mod.rs`

**Changes**: Replace `PbfStreamer` usage with intermediate storage:

Replace:
```rust
let streamer = PbfStreamer::new(&pbf_data, &self.output_dir, self.tile_size_degrees);
streamer.partition_parallel()?;
```

With:
```rust
self.save_tiles_to_redb(db, &pbf_data)?;
```

Note: For parallelism, we can use Rayon similar to `PbfStreamer::partition_parallel()`.

## Success Criteria

### Automated Verification:
 [x] Unit test: Store and retrieve grid via `GridStorage::store_grid()` and `load_grid()`
 [x] Unit test: Store and retrieve tile via `IntermediateTile::save_to_redb()` and `load_from_redb()`
 [x] Integration test: Process single PBF, verify redb contains expected data

### Manual Verification:
 [x] Process PBF with multi-PBF generator
 [x] Verify redb file contains grid data (not empty)
 [x] Verify redb file contains tile nodes/ways/relations
 [x] Verify tile count matches expected from PBF bounds

## Dependencies

- Depends on: Phase 1 (Grid Exposure)
- Blocks: Phase 3 (Overlap Re-evaluation)

## Risks & Mitigations

- **Risk**: Parallel tile saving may cause redb contention
  - **Mitigation**: Use write transactions per tile, or batch writes. Test with small PBF first.

- **Risk**: redb storage grows large for big PBFs
  - **Mitigation**: Expected - intermediate storage is temporary, deleted after final write.

## Notes

The redb database path should be configurable via CLI (`--db-path`) or use a temp directory by default. Consider adding cleanup logic to remove redb after successful final write.

Current `PbfStreamer` uses Rayon for parallel processing. For intermediate storage, we can:
1. Use sequential processing initially (simpler)
2. Add parallelism later if performance is an issue
