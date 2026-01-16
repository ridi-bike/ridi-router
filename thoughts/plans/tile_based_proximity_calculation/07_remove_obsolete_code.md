# Phase 7: Remove Obsolete Code

## Overview

This phase removes all obsolete code from the old implementation. The new tile-based system is complete and functional, so we can safely delete:
- Old partition() method
- node_coords_db database code
- compute_proximity_parallel() function
- partition_node/way/relation_redb() functions
- IntermediateTile structures
- intermediate_tiles.redb database code

This is a pure deletion phase - no new functionality, just cleanup.

## Changes Required

### 1. Remove Old partition() Method

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Delete**: Lines 58-165 (entire old partition method)

```rust
// DELETE THIS ENTIRE METHOD
pub fn partition(&self) -> Result<(TileBuffers, Database)> {
    // ... ~100 lines of old implementation ...
}
```

**Rationale**: Replaced by `partition_parallel()`.

### 2. Remove node_coords_db Code

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Delete**: All references to NODE_COORDS_TABLE

```rust
// DELETE THIS TABLE DEFINITION (line 38)
const NODE_COORDS_TABLE: TableDefinition<u64, (f32, f32, bool, bool)> = TableDefinition::new("node_coords");
```

**Delete**: All node_coords_db creation and cleanup code in old partition() method.

**Rationale**: No longer using intermediate database for node coordinates.

### 3. Remove compute_proximity_parallel()

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Delete**: Lines 313-372 (entire function)

```rust
// DELETE THIS ENTIRE METHOD
fn compute_proximity_parallel(&self, node_coords_db: &Database) -> Result<()> {
    // ... ~60 lines ...
}
```

**Rationale**: Proximity is now computed per-tile, not globally.

### 4. Remove extract_residential_areas() and extract_military_areas()

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Delete**: Lines 374-398 (both methods)

```rust
// DELETE THESE METHODS
fn extract_residential_areas(&self) -> Result<AreaGrid> { ... }
fn extract_military_areas(&self) -> Result<AreaGrid> { ... }
```

**Note**: These are replaced by `extract_residential_areas_for_bounds()` and `extract_military_areas_for_bounds()` which take bounds parameters.

### 5. Remove Old Proximity Computation Methods

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Delete**: Lines 400-453 (if duplicated)

Check if `compute_residential_proximity()` and `compute_nogo_area()` exist in both old and new code. If duplicated, keep only the new versions (from Phase 4).

**Rationale**: Avoid code duplication.

### 6. Remove partition_node()

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Delete**: Lines 167-197

```rust
// DELETE THIS ENTIRE METHOD
fn partition_node(
    &self,
    buffers: &mut TileBuffers,
    node: osmpbfreader::Node,
    node_coords_table: &impl ReadableTable<u64, (f32, f32, bool, bool)>
) -> Result<()> {
    // ... ~30 lines ...
}
```

**Rationale**: Node partitioning is now part of extract_tile_data().

### 7. Remove partition_way_redb()

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Delete**: Lines 199-244

```rust
// DELETE THIS ENTIRE METHOD
fn partition_way_redb<T: ReadableTable<u64, (f32, f32, bool, bool)>>(
    &self,
    buffers: &mut TileBuffers,
    way: osmpbfreader::Way,
    node_coords_table: &T
) -> Result<()> {
    // ... ~45 lines ...
}
```

**Rationale**: Way filtering is now part of extract_tile_data().

### 8. Remove partition_relation_redb()

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Delete**: Lines 246-310

```rust
// DELETE THIS ENTIRE METHOD
fn partition_relation_redb<T: ReadableTable<u64, (f32, f32, bool, bool)>>(
    &self,
    buffers: &mut TileBuffers,
    relation: osmpbfreader::Relation,
    node_coords_table: &T
) -> Result<()> {
    // ... ~65 lines ...
}
```

**Rationale**: Relation filtering is now part of extract_tile_data().

### 9. Remove get_tiles_for_point()

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Delete**: Lines 456-502 (if not used elsewhere)

```rust
// DELETE THIS METHOD IF NOT NEEDED
fn get_tiles_for_point(&self, lat: f32, lon: f32) -> Vec<TileId> {
    // ... border detection logic ...
}
```

**Rationale**: New implementation uses buffered bounds instead of multi-tile assignment.

**Note**: Check if this method is used elsewhere before deleting. If it's still needed for border handling, keep it.

### 10. Remove IntermediateTile Structure

**File**: `src/rmdf/generator/intermediate.rs`

**Delete**: Entire file if not used by other code

```rust
// DELETE ENTIRE FILE
// src/rmdf/generator/intermediate.rs
```

**Before deleting**, verify:
- Not used by manifest generation
- Not used by any other generator code
- Not exposed in public API

**Rationale**: TileData (in-memory) replaces IntermediateTile (database-backed).

### 11. Remove TileBuffers if Obsolete

**File**: `src/rmdf/generator/intermediate.rs`

**Check**: Is TileBuffers still used?

```rust
pub struct TileBuffers {
    pub tiles: HashMap<TileId, IntermediateTile>,
}
```

If TileBuffers is only used by old partition() method, delete it along with IntermediateTile.

**Rationale**: New implementation doesn't buffer tiles in memory - processes one at a time.

### 12. Update Module Exports

**File**: `src/rmdf/generator/mod.rs`

**Changes**: Remove exports of deleted items

```rust
// DELETE THESE LINES IF THEY EXIST
pub use intermediate::{IntermediateTile, TileBuffers};
```

**Rationale**: Clean up module interface.

### 13. Remove Old Tests

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Delete**: Tests for old implementation (lines 505-547)

```rust
// DELETE THESE TESTS IF THEY TEST OLD METHODS
#[cfg(test)]
mod tests {
    #[test]
    fn test_tile_assignment_no_border() { ... }

    #[test]
    fn test_tile_assignment_on_border() { ... }

    #[test]
    fn test_tile_assignment_corner() { ... }
}
```

**Note**: Keep tests that verify helper methods still used by new implementation.

### 14. Remove Unused Imports

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Clean up imports after deletions

```rust
// Remove imports only used by deleted code
// Keep only:
use anyhow::{Context, Result};
use osmpbfreader::{OsmPbfReader, OsmObj};
use std::fs::File;
use std::path::{Path, PathBuf};
use tracing::info;
use rayon::prelude::*;
use geo::{Point, Distance, Haversine, HaversineClosestPoint, CoordsIter, GeodesicArea};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::map_data::osm::{OsmNode, OsmWay, OsmRelation, OsmRelationMember, OsmRelationMemberRole, OsmRelationMemberType};
use crate::map_data::proximity::AreaGrid;
use crate::map_data::generation_graph::GenerationGraph;
use crate::osm_data::pbf_area_reader::PbfAreaReader;
use crate::rmdf::format::{TileId, TileBounds};
use crate::rmdf::generator::writer::RmdfWriter;

// DELETE THESE IF NOT USED
// use redb::{Database, TableDefinition, ReadableTable};
// use super::intermediate::TileBuffers;
```

**Rationale**: Keep codebase clean and reduce compilation dependencies.

## Success Criteria

### Automated Verification

- [ ] Code compiles without errors after deletions
- [ ] No unused import warnings
- [ ] No dead code warnings
- [ ] All remaining tests pass
- [ ] `cargo clippy` shows no warnings

### Manual Verification

- [ ] Search codebase for references to deleted items:
  - `node_coords_db`
  - `NODE_COORDS_TABLE`
  - `compute_proximity_parallel`
  - `partition_node`
  - `partition_way_redb`
  - `partition_relation_redb`
  - `IntermediateTile`
  - `TileBuffers`
- [ ] Verify no compilation errors
- [ ] Run full generation to verify new system works without old code

## Dependencies

- Depends on: Phase 6 (Remaining Bits)
- Blocks: Phase 8 (Testing and Cleanup)

## Risks & Mitigations

- **Risk**: Accidentally delete code still in use
  - **Mitigation**: Careful search for references before deleting
  - **Mitigation**: Compile after each deletion to catch errors early

- **Risk**: Remove helper methods needed by new code
  - **Mitigation**: Only delete methods confirmed to be used only by old partition()

## Notes

### Deletion Strategy

Recommended order:
1. Delete test cases first (safest)
2. Delete high-level methods (partition, compute_proximity_parallel)
3. Delete helper methods (partition_node/way/relation)
4. Delete data structures (IntermediateTile, TileBuffers)
5. Clean up imports and exports
6. Compile and fix any errors
7. Run full test suite

### Before/After Line Count

Expected reduction:
- `pbf_streamer.rs`: ~450 lines → ~300 lines (33% reduction)
- `intermediate.rs`: ~305 lines → 0 lines (deleted if obsolete)
- Total: ~755 lines removed

### Verification Commands

After deletions, verify:

```bash
# Check for compilation errors
cargo build

# Check for warnings
cargo clippy

# Check for unused dependencies
cargo machete  # If installed

# Search for deleted item references
rg "node_coords_db" src/
rg "IntermediateTile" src/
rg "partition_node" src/
rg "compute_proximity_parallel" src/
```

All searches should return no results (except possibly in comments or this plan).

### Keeping Old Code for Reference

If you want to keep old code for reference:
1. Create a git branch before deletion: `git branch old-partition-impl`
2. Or move old code to a separate file: `pbf_streamer_old.rs` (don't compile it)

Recommended: Just delete and rely on git history.

### Module Structure After Cleanup

Final `src/rmdf/generator/` structure:
```
generator/
├── mod.rs              - Main entry point (TileGenerator)
├── pbf_streamer.rs     - Tile-based parallel processing
├── writer.rs           - RMDF serialization
├── manifest.rs         - Manifest generation
└── [intermediate.rs]   - Deleted (or kept if used elsewhere)
```

Clean and focused on new architecture.
