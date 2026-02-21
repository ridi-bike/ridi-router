---
date: 2026-02-21T16:06:00+02:00
git_commit: debcadafdccece2cde068ac206e4697c483ab32f
branch: feat-ridi-map-format
repository: ridi-router
topic: "Multi-PBF Tile Generation for generate-tiles Command"
tags: [research, codebase, tile-generation, multi-pbf, rmdf, cli, grid-storage]
last_updated: 2026-02-21T16:06:00+02:00
---

## Ticket Synopsis

Feature request to enable the `generate-tiles` CLI command to process all PBF files in an input directory and generate a unified set of RMDF tiles. Currently, the command only accepts a single PBF file via `--input`. This feature requires:
- CLI changes: `--input` → `--input-dir`
- Multi-PBF processing pipeline with overlap zone handling
- Grid serialization and merge for proximity flags
- Tile content merging for duplicate TileIds
- Manifest generation with combined bounds

## Summary

The multi-PBF tile generation feature is **partially scaffolded**. The `MultiPbfGenerator` struct exists with the full pipeline design, but several critical pieces are incomplete:

1. **CLI Integration**: Still uses `TileGenerator` with `--input FILE` (line 449-466 in `router_runner.rs`)
2. **Grid Exposure**: `InMemoryPbf::from_pbf_file_with_flags()` computes the grid but doesn't expose it
3. **Grid Storage**: `process_single_pbf()` passes placeholder `&[]` for grid bytes (line 231 in `mod.rs`)
4. **Overlap Re-evaluation**: `reevaluate_overlap_zones()` is stubbed with TODO
5. **Tile Merge**: `write_final_tiles()` just logs "tiles written during phase 1"

The good news: **Grid serialization already exists** in `RasterizedProximityGrid::to_bytes()` and `from_bytes()` (lines 333-407 in `rasterized_grid.rs`). The `build_combined_grid()` function in `intermediate.rs` (lines 543-597) demonstrates the merge logic using MAX for sectors and OR for military flags.

## Detailed Findings

### CLI and Orchestration

**`src/router_runner.rs:449-466`**
- `CliMode::GenerateTiles { input, output, tile_size }` uses `TileGenerator`
- Needs to change to `input_dir` and use `MultiPbfGenerator`

**`src/rmdf/generator/mod.rs:18-78`** - `TileGenerator`
- Single-PBF implementation: loads PBF, creates `PbfStreamer`, generates manifest
- Uses `InMemoryPbf::from_pbf_file_with_flags()` for loading with pre-computed flags

**`src/rmdf/generator/mod.rs:109-306`** - `MultiPbfGenerator`
- Full pipeline scaffolded:
  1. Phase 1: Process each PBF independently (line 160-164)
  2. Phase 2: Identify overlap zones (line 167-169)
  3. Phase 3: Re-evaluate nodes in overlap zones (line 172-175)
  4. Phase 4: Write final RMDF tiles (line 178-179)
- `discover_pbf_files()` at line 82-106 is **fully implemented**

### Grid System

**`src/proximity/rasterized_grid.rs:143-408`** - `RasterizedProximityGrid`
- **Serialization exists**: `to_bytes()` at line 333, `from_bytes()` at line 357
- Format: 16-byte header (cols, rows, lon_min, lat_min) + 36-byte cells
- Each cell: 8 × f32 sectors + 1 byte bool + 3 bytes padding
- **Grid merge**: `GridCell::merge()` at line 101 uses MAX for sectors, OR for military

**`src/proximity/flag_computer.rs:66-95`** - `compute_proximity_flags_with_grid()`
- Returns the grid after computing flags
- This is the variant needed for multi-PBF to capture the grid

**`src/rmdf/generator/intermediate.rs:363-401`** - `GridBounds`
- 16-byte structure (lon_min, lat_min, lon_max, lat_max as f32)
- `to_bytes()` and `from_bytes()` implemented
- `overlaps()` method for detecting overlap zones

**`src/rmdf/generator/intermediate.rs:404-506`** - `GridStorage`
- `store_grid()`: Store grid bytes, path, and bounds in redb
- `load_grid()`: Load grid bytes by pbf_id
- `load_all_bounds()`: Get all bounds for overlap detection
- `find_overlapping_pbfs()`: Find PBFs overlapping a region
- `load_grids_for_region()`: Load all grids covering a region

### Grid Merge Implementation

**`src/rmdf/generator/intermediate.rs:543-597`** - `build_combined_grid()`
- Creates new grid for the overlap region
- Uses `find_overlapping_cells()` to identify cells in multiple grids
- Merges using `GridCell::merge()` which does MAX/OR
- Copies non-overlapping cells directly

**Critical insight**: The merge logic is already implemented! It correctly:
1. Takes MAX of residential sectors (avoids double-counting when same polygon appears in multiple PBFs)
2. Uses OR for military interior flag (if either says nogo, it's nogo)

### InMemoryPbf and Grid Access

**`src/osm_data/in_memory_pbf.rs:185-204`** - `InMemoryPbf` struct
- Does NOT currently store the `RasterizedProximityGrid`
- `from_pbf_file_with_flags()` at line 290 computes flags but discards grid

**Required change**: Add a field to store the grid:
```rust
pub struct InMemoryPbf {
    // ... existing fields ...
    proximity_grid: Option<RasterizedProximityGrid>,  // NEW
}
```

**`src/osm_data/in_memory_pbf.rs:356-377`** - Flag computation
- Calls `crate::proximity::compute_proximity_flags()` which discards grid
- Should use `compute_proximity_flags_with_grid()` and store result

### Tile Processing and Writing

**`src/rmdf/generator/pbf_streamer.rs:54-70`** - `TileData`
- Internal struct: `tile_id`, `nodes` (HashMap), `ways` (Vec), `relations` (Vec)
- Nodes use HashMap for deduplication by OSM ID

**`src/rmdf/generator/pbf_streamer.rs:97-152`** - `partition_parallel()`
- Progress reporting: every 100 tiles or at completion
- Shows rate (tiles/sec) and ETA

**`src/rmdf/generator/writer.rs:23-139`** - `RmdfWriter::write_tile_from_graph()`
- Takes `GenerationGraph` and writes RMDF binary
- No merge capability - writes single tile

### Intermediate Storage

**`src/rmdf/generator/intermediate.rs:32-154`** - `IntermediateTile`
- Has `merge()` method at line 102 for combining tiles
- Has `deduplicate()` method at line 121 for removing duplicate ways/relations
- `save_to_redb()` and `load_from_redb()` for persistent storage
- Uses bincode for serialization to redb

**Tables defined**:
- `TILE_NODES`: Key (col, row, osm_id) → serialized OsmNode
- `TILE_WAYS`: Key (col, row, osm_id) → serialized OsmWay
- `TILE_RELATIONS`: Key (col, row, osm_id) → serialized OsmRelation
- `PROXIMITY_GRIDS`: Key pbf_id → serialized grid bytes
- `PBF_FILES`: Key pbf_id → file path string
- `PBF_BOUNDS`: Key pbf_id → serialized GridBounds

### Manifest Generation

**`src/rmdf/generator/manifest.rs:52-162`** - `ManifestGenerator`
- `discover_tiles()`: Scan output dir for `.rmdf` files
- `generate()`: Create manifest with metadata, neighbors, checksums
- Currently accepts single `source_file` parameter - needs to accept Vec for multi-PBF

**`src/rmdf/generator/manifest.rs:147-155`** - TileManifest struct
- Has `source_files: Vec<String>` - already supports multiple sources!

### Stub Implementations

**`src/rmdf/generator/mod.rs:281-295`** - `reevaluate_overlap_zones()`
```rust
fn reevaluate_overlap_zones(&self, _db, _overlap_zones) -> Result<()> {
    // TODO: Implement grid combination and node re-evaluation
    // This requires:
    // 1. Load grids that cover each overlap zone
    // 2. Merge grids using MAX for sectors, OR for military
    // 3. Query nodes in overlap zones from redb
    // 4. Re-evaluate flags with combined grid
    // 5. Update nodes in redb
    info!("Overlap zone re-evaluation not yet fully implemented");
    Ok(())
}
```

**`src/rmdf/generator/mod.rs:298-305`** - `write_final_tiles()`
```rust
fn write_final_tiles(&self, _db, _pbf_files) -> Result<()> {
    // Tiles are already written by PbfStreamer in phase 1
    // In a full implementation, we would:
    // 1. Deduplicate nodes/ways/relations across tiles
    // 2. Write combined tiles
    info!("Final tile writing complete (tiles written during phase 1)");
    Ok(())
}
```

## Code References

- `src/router_runner.rs:449-466` - CLI GenerateTiles handling (needs change)
- `src/rmdf/generator/mod.rs:18-78` - TileGenerator (single-PBF, currently used)
- `src/rmdf/generator/mod.rs:109-306` - MultiPbfGenerator (scaffolded, not wired)
- `src/rmdf/generator/mod.rs:82-106` - discover_pbf_files() (implemented)
- `src/rmdf/generator/mod.rs:205-241` - process_single_pbf() (partial, needs grid storage)
- `src/rmdf/generator/mod.rs:244-278` - identify_overlap_zones() (implemented)
- `src/rmdf/generator/mod.rs:281-295` - reevaluate_overlap_zones() (stub)
- `src/rmdf/generator/mod.rs:298-305` - write_final_tiles() (stub)
- `src/rmdf/generator/intermediate.rs:363-401` - GridBounds (implemented)
- `src/rmdf/generator/intermediate.rs:404-506` - GridStorage (implemented)
- `src/rmdf/generator/intermediate.rs:543-597` - build_combined_grid() (implemented)
- `src/rmdf/generator/pbf_streamer.rs:54-70` - TileData struct
- `src/rmdf/generator/pbf_streamer.rs:97-152` - partition_parallel() with progress
- `src/rmdf/generator/manifest.rs:52-162` - ManifestGenerator
- `src/proximity/rasterized_grid.rs:143-408` - RasterizedProximityGrid with serialization
- `src/proximity/rasterized_grid.rs:333-354` - to_bytes() serialization
- `src/proximity/rasterized_grid.rs:357-407` - from_bytes() deserialization
- `src/proximity/flag_computer.rs:66-95` - compute_proximity_flags_with_grid()
- `src/osm_data/in_memory_pbf.rs:185-204` - InMemoryPbf (needs grid field)
- `src/osm_data/in_memory_pbf.rs:290-395` - from_pbf_file_with_flags()

## Architecture Insights

### Current Tile Generation Flow (Single-PBF)
```
PBF File → InMemoryPbf (with flags) → PbfStreamer.partition_parallel()
                                         ↓
                                    For each tile:
                                    1. Extract data
                                    2. Build GenerationGraph
                                    3. Write via RmdfWriter
                                         ↓
                                    ManifestGenerator.discover_tiles()
                                    ManifestGenerator.generate()
```

### Target Multi-PBF Flow
```
Input Dir → discover_pbf_files() → For each PBF:
                                    InMemoryPbf (with flags + grid)
                                         ↓
                                    Store grid to redb
                                    PbfStreamer.partition_parallel()
                                         ↓
identify_overlap_zones() → For each zone:
                           Load grids → build_combined_grid()
                           Query nodes → Re-evaluate flags → Update tiles
                                         ↓
                           write_final_tiles() (merge duplicate TileIds)
                                         ↓
                           ManifestGenerator (with combined bounds)
```

### Key Design Decision: Tiles Written Immediately
The current design writes tiles during Phase 1 (process_single_pbf). The `write_final_tiles()` stub acknowledges this. For overlapping tiles:
1. First PBF writes tile_123_456.rmdf
2. Second PBF overwrites tile_123_456.rmdf (or would need merge)

The ticket's Phase 5 ("Tile Merge") suggests tracking which PBFs wrote which tiles and merging content. This requires significant re-architecture - either:
- **Option A**: Buffer all tiles in redb, merge at end (memory-intensive)
- **Option B**: Track tile writers, re-read and merge on conflict (I/O intensive)
- **Option C**: Only merge proximity flags in overlap zones, accept last-PBF-wins for tile content (simpler)

### Grid Merge is Already Correct
The `GridCell::merge()` method correctly implements:
- MAX for residential_sectors (avoids double-counting same polygon)
- OR for is_military_interior (conservative: if either says nogo, it's nogo)

This means overlapping border zones will get correct proximity flags when grids are combined.

## Historical Context

- `thoughts/plans/rmdf_memory_mapped_tiles/` - Original design for tile-based architecture
- `thoughts/plans/rmdf_memory_mapped_tiles/02_streaming_partitioner.md` - PbfStreamer design
- `thoughts/plans/rmdf_memory_mapped_tiles/03_proximity_computation.md` - Grid-based flag computation
- `thoughts/plans/rmdf_memory_mapped_tiles/04_rmdf_writer.md` - RMDF binary format
- `thoughts/tickets/feature_rmdf_memory_mapped_tiles.md` - Original tile generation feature
- `thoughts/tickets/debt_tile_based_proximity_calculation.md` - Related cleanup ticket

## Related Research

- `thoughts/research/2026-01-16_tile_based_proximity_calculation.md` - Prior research on tile-based system
- `thoughts/research/2026-01-14_rmdf_memory_mapped_tiles.md` - Prior research on RMDF format

## Open Questions

1. **Tile Content Merge Strategy**: The ticket mentions merging nodes/ways/relations when multiple PBFs produce the same TileId. Current implementation is "last PBF wins". Should we:
   - Store intermediate tiles in redb and merge at end?
   - Accept the simpler approach of only fixing proximity flags in overlap zones?

2. **Grid Bounds Edge Case**: The `GridBounds::overlaps()` uses inclusive comparison. At exact boundaries (e.g., lat=50.0 where one PBF ends and another begins), should both grids contribute? Current code says yes (edge touching counts as overlap).

3. **Memory vs I/O Tradeoff**: Storing all intermediate tile data in redb vs. re-reading tiles for merge. For planet-scale (thousands of PBFs), redb approach may be required.

4. **Manifest Bounds**: When combining bounds from multiple PBFs, should we compute the union (simple) or track gaps (complex)? The ticket says "combined bounds" which suggests union.
