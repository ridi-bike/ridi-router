---
date: 2026-02-21T17:51:33+02:00
git_commit: b3a30c31708ccc92dbd2fae8a5085dc00030bb96
branch: feat-ridi-map-format
repository: ridi-router
topic: "Multi-PBF Tile Generation Support"
tags: [research, codebase, rmdf, tiles, pbf, multi-file, proximity, grid, overlap]
last_updated: 2026-02-21T17:51:33+02:00
---

## Ticket Synopsis

Enable `ridi-router generate-tiles` to process multiple PBF files from an input directory, generating a unified set of RMDF tiles that correctly handle overlapping data between PBF files. The feature requires:

1. CLI support for `--input-dir` instead of single `--input` file
2. Individual PBF processing with proximity grid storage
3. Overlap zone detection between PBF geographic bounds
4. Flag re-evaluation in overlap zones using combined grids
5. Tile deduplication and merging across PBF sources
6. Unified manifest generation for all output tiles

## Summary

The multi-PBF tile generation infrastructure is **partially implemented**. Key components exist but have critical gaps:

| Component | Status | Key Gap |
|-----------|--------|---------|
| MultiPbfGenerator | Partial | Grid storage passes empty slice, nodes not stored |
| GridStorage | Complete | Works but not connected to actual grid data |
| Overlap Detection | Complete | `identify_overlap_zones()` fully functional |
| Grid Merging | Complete | `build_combined_grid()` and `GridCell.merge()` exist |
| Tile Merging | Complete | `IntermediateTile.merge()` and `deduplicate()` exist |
| Flag Re-evaluation | **Not Started** | `reevaluate_overlap_zones()` is placeholder |
| Final Tile Writing | **Not Started** | `write_final_tiles()` is placeholder |
| CLI | **Not Started** | Only supports single `--input` file |

**Critical blocker**: `InMemoryPbf` does not expose the computed `RasterizedProximityGrid`, preventing grid storage for overlap re-evaluation.

## Detailed Findings

### MultiPbfGenerator (src/rmdf/generator/mod.rs:109-306)

**Fully Implemented:**
- `generate()` method orchestrates 4-phase workflow correctly
- `discover_pbf_files()` finds and sorts `.pbf` files (lines 82-106)
- `identify_overlap_zones()` calculates pairwise overlaps with 500m buffer (lines 244-278)

**Partially Implemented:**
- `process_single_pbf()` (lines 205-241):
  - ✅ Loads PBF with `InMemoryPbf::from_pbf_file_with_flags()`
  - ✅ Creates `PbfStreamer` and partitions tiles in parallel
  - ✅ Stores grid bounds via `GridStorage::store_grid()`
  - ❌ **Gap**: Grid storage passes empty slice `&[]` instead of serialized grid
  - ❌ **Gap**: Comment notes `InMemoryPbf` doesn't expose grid (lines 212-214)
  - ❌ **Gap**: Flagged nodes not stored to redb for overlap re-evaluation

**Placeholder (Not Implemented):**
- `reevaluate_overlap_zones()` (lines 281-295): Returns `Ok(())` immediately
- `write_final_tiles()` (lines 298-305): Returns `Ok(())` - assumes tiles already written

### GridStorage (src/rmdf/generator/intermediate.rs:404-509)

**Complete implementation:**
- `store_grid()`: Stores grid bytes and bounds per PBF ID
- `load_grid()`: Retrieves grid bytes by PBF ID
- `load_all_bounds()`: Returns all stored PBF bounds for overlap detection
- `find_overlapping_pbfs()`: Finds PBFs overlapping a region
- `load_grids_for_region()`: Loads all grids covering a region

**redb Tables:**
```rust
const PROXIMITY_GRIDS: TableDefinition<u64, &[u8]> = TableDefinition::new("proximity_grids");
const PBF_BOUNDS: TableDefinition<u64, &[u8]> = TableDefinition::new("pbf_bounds");
const PBF_FILES: TableDefinition<u64, &str> = TableDefinition::new("pbf_files");
const TILE_NODES: TableDefinition<(u16, u16, u64), &[u8]> = TableDefinition::new("tile_nodes");
const TILE_WAYS: TableDefinition<(u16, u16, u64), &[u8]> = TableDefinition::new("tile_ways");
const TILE_RELATIONS: TableDefinition<(u16, u16, u64), &[u8]> = TableDefinition::new("tile_relations");
```

### Overlap Detection (src/rmdf/generator/mod.rs:244-278)

**Algorithm:**
1. Load all PBF bounds via `GridStorage::load_all_bounds()`
2. For each pair (i, j) where i < j:
   - Check if `bounds_a.overlaps(bounds_b)`
   - Calculate intersection bounds
   - Add 500m buffer: `buffer_deg = 500.0 / 111_000.0`
3. Return list of buffered overlap zones

**GridBounds (src/rmdf/generator/intermediate.rs:363-403):**
```rust
pub struct GridBounds {
    pub lon_min: f32, pub lat_min: f32,
    pub lon_max: f32, pub lat_max: f32,
}
// Methods: to_bytes(), from_bytes(), overlaps()
```

### Grid Merging (src/rmdf/generator/intermediate.rs:543-597)

**build_combined_grid() Algorithm:**
1. Collect all cells at each global coordinate
2. For each overlapping cell position, merge using:
   - **MAX** for `residential_sectors[i]` (avoids double-counting)
   - **OR** for `is_military_interior` flag

**GridCell.merge() (src/proximity/rasterized_grid.rs:101-107):**
```rust
pub fn merge(&mut self, other: &GridCell) {
    for i in 0..8 {
        self.residential_sectors[i] = self.residential_sectors[i].max(other.residential_sectors[i]);
    }
    self.is_military_interior |= other.is_military_interior;
}
```

### Tile Merging (src/rmdf/generator/intermediate.rs:97-150)

**IntermediateTile.merge():**
```rust
pub fn merge(&mut self, other: IntermediateTile) -> anyhow::Result<()> {
    // Nodes: HashMap insert (last write wins)
    for (id, node) in other.nodes {
        self.nodes.insert(id, node);
    }
    // Ways/Relations: append, deduplicate later
    self.ways.extend(other.ways);
    self.relations.extend(other.relations);
    Ok(())
}
```

**IntermediateTile.deduplicate():**
- Uses `HashSet` to track seen OSM IDs
- First-seen-wins for ways and relations
- Drains source Vec, rebuilds with unique entries

**Concern**: Node flag merging is order-dependent. If node A from PBF1 has `residential_in_proximity: true` and same node from PBF2 has `false`, the result depends on merge order. No OR-ing of flags occurs.

### PbfStreamer (src/rmdf/generator/pbf_streamer.rs:72-620)

**Design**: Single-PBF focused processor

**Key Methods:**
- `partition_parallel()`: Main entry, uses Rayon for parallel tile processing
- `process_tile()`: Single tile processing with 500m buffer
- `calculate_all_tiles()`: Determines intersecting tiles from PBF bounds
- `extract_tile_data()`: Queries `InMemoryPbf` for nodes/ways/relations in bounds
- `build_generation_graph()`: Converts tile data to graph

**Critical Note (line 169):**
> "Nodes already have residential_in_proximity and nogo_area flags pre-computed"

Flags are computed during `InMemoryPbf::from_pbf_file_with_flags()` BEFORE `PbfStreamer` receives the data. `PbfStreamer` is a pass-through for flags.

**Multi-PBF Integration**: `PbfStreamer` should remain single-PBF focused. `MultiPbfGenerator` coordinates multiple `PbfStreamer` instances and handles overlap re-evaluation separately.

### Proximity Grid Serialization (src/proximity/rasterized_grid.rs:333-410)

**RasterizedProximityGrid.to_bytes() Format:**
```
Header (16 bytes):
  - cols: u32 (4 bytes)
  - rows: u32 (4 bytes)
  - lon_min: f32 (4 bytes)
  - lat_min: f32 (4 bytes)
Cells (36 bytes each):
  - residential_sectors: [f32; 8] (32 bytes)
  - is_military_interior: bool (1 byte)
  - _padding: [u8; 3] (3 bytes)
```

**RasterizedProximityGrid.from_bytes()**: Parses bytes back into grid with validation.

### CLI Structure (src/router_runner.rs:181-470)

**Current State:**
```rust
GenerateTiles {
    #[arg(short, long, value_name = "FILE")]
    input: PathBuf,  // Single file only

    #[arg(short, long, value_name = "DIR")]
    output: PathBuf,

    #[arg(long, default_value = "1.0")]
    tile_size: f32,
}
```

**Required Changes:**
1. Add `--input-dir` option (mutually exclusive with `--input`)
2. Dispatch to `MultiPbfGenerator` when directory input used
3. Add `--db-path` option for intermediate storage location

## Code References

| File | Lines | Description |
|------|-------|-------------|
| `src/rmdf/generator/mod.rs` | 109-306 | MultiPbfGenerator implementation |
| `src/rmdf/generator/mod.rs` | 18-106 | TileGenerator (single PBF, complete) |
| `src/rmdf/generator/mod.rs` | 82-106 | discover_pbf_files() function |
| `src/rmdf/generator/mod.rs` | 244-278 | identify_overlap_zones() |
| `src/rmdf/generator/mod.rs` | 281-295 | reevaluate_overlap_zones() (placeholder) |
| `src/rmdf/generator/mod.rs` | 298-305 | write_final_tiles() (placeholder) |
| `src/rmdf/generator/intermediate.rs` | 12-30 | redb table definitions |
| `src/rmdf/generator/intermediate.rs` | 33-95 | IntermediateTile struct |
| `src/rmdf/generator/intermediate.rs` | 97-150 | merge() and deduplicate() methods |
| `src/rmdf/generator/intermediate.rs` | 156-250 | save_to_redb() and load_from_redb() |
| `src/rmdf/generator/intermediate.rs` | 363-403 | GridBounds struct |
| `src/rmdf/generator/intermediate.rs` | 404-509 | GridStorage helper |
| `src/rmdf/generator/intermediate.rs` | 512-538 | find_overlapping_cells() |
| `src/rmdf/generator/intermediate.rs` | 543-597 | build_combined_grid() |
| `src/proximity/rasterized_grid.rs` | 54-77 | GridCell struct |
| `src/proximity/rasterized_grid.rs` | 101-107 | GridCell.merge() |
| `src/proximity/rasterized_grid.rs` | 143-165 | RasterizedProximityGrid struct |
| `src/proximity/rasterized_grid.rs` | 333-410 | to_bytes() and from_bytes() |
| `src/osm_data/in_memory_pbf.rs` | 185-224 | InMemoryPbf struct |
| `src/osm_data/in_memory_pbf.rs` | 290-345 | from_pbf_file_with_flags() |
| `src/rmdf/generator/pbf_streamer.rs` | 72-76 | PbfStreamer struct |
| `src/rmdf/generator/pbf_streamer.rs` | 97-152 | partition_parallel() |
| `src/rmdf/generator/pbf_streamer.rs` | 161-203 | process_tile() |
| `src/router_runner.rs` | 181-214 | CliMode enum with GenerateTiles |

## Architecture Insights

### Multi-PBF Processing Flow

```
Phase 1: Process each PBF independently
┌─────────────────────────────────────────────────────────────┐
│  For each PBF in input-dir:                                  │
│    1. Load with InMemoryPbf::from_pbf_file_with_flags()      │
│    2. Extract proximity grid (BLOCKER: not exposed)          │
│    3. Store grid + bounds to redb                            │
│    4. Partition into intermediate tiles via PbfStreamer      │
│    5. Store tiles to redb with flagged nodes                 │
└─────────────────────────────────────────────────────────────┘

Phase 2: Identify overlap zones
┌─────────────────────────────────────────────────────────────┐
│  1. Load all PBF bounds from redb                            │
│  2. For each pair, check overlaps()                          │
│  3. Calculate intersection + 500m buffer                     │
│  4. Return list of overlap zones                             │
└─────────────────────────────────────────────────────────────┘

Phase 3: Re-evaluate overlap zones (NOT IMPLEMENTED)
┌─────────────────────────────────────────────────────────────┐
│  For each overlap zone:                                       │
│    1. Load all grids covering the zone                       │
│    2. Merge grids (MAX sectors, OR military)                 │
│    3. Query nodes in zone from redb                          │
│    4. Re-compute flags with combined grid                    │
│    5. Update node flags in redb                              │
└─────────────────────────────────────────────────────────────┘

Phase 4: Write final tiles (NOT IMPLEMENTED)
┌─────────────────────────────────────────────────────────────┐
│  1. Group intermediate tiles by (col, row)                   │
│  2. Merge tiles at same position from different PBFs        │
│  3. Deduplicate nodes/ways/relations by OSM ID              │
│  4. Write unified RMDF tiles                                 │
│  5. Generate manifest with all source files                  │
└─────────────────────────────────────────────────────────────┘
```

### Key Design Decisions (from historical research)

1. **Sequential PBF processing**: Memory efficient, enables incremental updates
2. **redb for intermediate storage**: Already implemented, suitable for multi-PBF
3. **Grid merging strategy**: MAX for residential (avoid double-counting), OR for military
4. **Overlap buffer**: 500m (same as `RESIDENTIAL_PROXIMITY_THRESHOLD_METERS`)
5. **Tile naming**: Deterministic from coordinates - same location from different PBFs would conflict
6. **Border point handling**: Duplicated in adjacent tiles, deduplication by OSM ID

### Buffer Zone Calculations

```rust
// 500m buffer for residential proximity
let buffer_deg = 500.0 / 111_000.0; // ~0.0045 degrees

// Border tolerance for tile assignment
const BORDER_TOLERANCE: f64 = 0.0001; // ~11 meters
```

## Historical Context (from thoughts/)

- `thoughts/plans/rmdf_memory_mapped_tiles/00_overview.md` - Master architecture overview, border point duplication strategy
- `thoughts/research/2026-01-16_tile_based_proximity_calculation.md` - Documents **CRITICAL BUG**: `GenerationGraph.insert_node()` hardcodes flags to false
- `thoughts/plans/rmdf_memory_mapped_tiles/02_streaming_partitioner.md` - Streaming PBF partitioning, intermediate storage design

**Critical Historical Finding**: The research notes a bug where `GenerationGraph.insert_node()` hardcodes proximity flags to `false`. This must be verified/fixed as part of multi-PBF implementation.

## Open Questions

1. **InMemoryPbf grid exposure**: How should `InMemoryPbf` expose the `RasterizedProximityGrid`?
   - Option A: Add public getter method
   - Option B: Return grid separately from loading
   - Option C: Compute grid outside InMemoryPbf

2. **Node flag merge strategy**: When merging nodes from multiple PBFs, should flags be OR-ed or should one source take precedence?
   - Current: Last write wins (order-dependent)
   - Alternative: OR for boolean flags

3. **Tile conflict resolution**: When same tile (col, row) exists in multiple PBFs:
   - Current design: Merge and deduplicate
   - Alternative: Detect and error?

4. **Manifest source tracking**: How detailed should source file metadata be in manifest?
   - Just filenames?
   - Include checksums for incremental update detection?

## Recommended Implementation Order

1. **Expose grid from InMemoryPbf** - Add method to access computed `RasterizedProximityGrid`
2. **Complete process_single_pbf()** - Store actual grid bytes, store flagged nodes
3. **Implement reevaluate_overlap_zones()** - Core logic for flag re-computation
4. **Implement write_final_tiles()** - Tile merging and deduplication
5. **Update CLI** - Add `--input-dir` option and dispatch logic
6. **Add tests** - Unit tests for each new component, integration test with mock overlapping PBFs
