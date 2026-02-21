# Multi-PBF Tile Generation Implementation Plan

## Overview

Enable `ridi-router generate-tiles` to process multiple PBF files from an input directory, generating a unified set of RMDF tiles that correctly handle overlapping data between PBF files.

The current implementation only supports single PBF file processing. This feature extends the tile generator to:
1. Discover and load multiple PBF files from an input directory
2. Process each PBF independently with its own proximity grid
3. Detect geographic overlaps between PBF files
4. Re-evaluate nodes in overlap zones using combined grids
5. Deduplicate and merge tiles from multiple sources
6. Output unified RMDF tiles covering all input PBFs

## Current State Analysis

### Infrastructure Status

| Component | Status | Location | Notes |
|-----------|--------|----------|-------|
| Grid serialization | Complete | `src/proximity/rasterized_grid.rs:277-345` | `to_bytes()`, `from_bytes()` ready |
| GridStorage | Complete | `src/rmdf/generator/intermediate.rs:374-438` | All methods implemented |
| Overlap detection | Complete | `src/rmdf/generator/mod.rs:244-279` | `identify_overlap_zones()` functional |
| Grid merging | Complete | `src/proximity/rasterized_grid.rs:77-82`, `intermediate.rs:539-590` | `GridCell::merge()`, `build_combined_grid()` |
| Tile merge/dedupe | Complete | `src/rmdf/generator/intermediate.rs:102-138` | `merge()`, `deduplicate()` ready |
| `compute_proximity_flags_with_grid()` | Complete | `src/proximity/flag_computer.rs:51-79` | Returns grid for storage |
| `apply_grid_to_nodes()` | Complete | `src/proximity/flag_computer.rs:81-88` | For overlap re-evaluation |
| `process_single_pbf()` | Partial | `src/rmdf/generator/mod.rs:205-241` | Stores empty grid bytes, writes RMDF not redb |
| `reevaluate_overlap_zones()` | Placeholder | `src/rmdf/generator/mod.rs:281-295` | Empty implementation |
| `write_final_tiles()` | Placeholder | `src/rmdf/generator/mod.rs:298-306` | Empty implementation |
| CLI | Single-PBF only | `src/router_runner.rs:214-226, 449-466` | No `--input-dir` option |

### Key Discovery

The research noted "InMemoryPbf doesn't expose the grid" as a blocker. This is solvable: `compute_proximity_flags_with_grid()` already exists and returns the grid - it just needs to be used.

### Critical Architecture Decision

Current `PbfStreamer::process_tile()` writes **RMDF tiles directly** (`src/rmdf/generator/pbf_streamer.rs:195`). For multi-PBF, we need **intermediate storage** in redb to enable:
- Overlap detection between PBFs
- Flag re-evaluation in overlap zones
- Tile merging across PBF sources

## Desired End State

After this plan is complete:
- `ridi-router generate-tiles --input-dir ./map-data/input --output-dir ./map-data/output` processes all PBF files
- Unified RMDF tiles are generated covering the union of all input PBFs
- Nodes in overlap zones have correct proximity flags computed from combined grids
- Manifest includes all source PBF filenames

**Verification**: Process two overlapping PBF files (e.g., germany.osm.pbf and austria.osm.pbf) and verify:
1. No duplicate nodes/ways in any tile
2. Border region nodes have correct flags
3. Single manifest covers both sources

## What We're NOT Doing

- **Incremental updates**: No detection of changed PBF files or partial re-processing
- **Checksums in manifest**: Filenames only, no SHA256 hashes
- **Parallel PBF loading**: Sequential processing for memory efficiency
- **Single-PBF mode changes**: Existing `--input` file mode unchanged
- **Proximity algorithm changes**: Using existing grid/flag computation

## Implementation Approach

1. **Phase 1**: Add grid exposure to `InMemoryPbf` - new method returns both PBF and grid
2. **Phase 2**: Complete `process_single_pbf()` - store actual grid, save tiles to redb
3. **Phase 3**: Implement `reevaluate_overlap_zones()` - core re-evaluation logic
4. **Phase 4**: Implement `write_final_tiles()` - merge, dedupe, write final tiles
5. **Phase 5**: Update CLI - add `--input-dir` option and dispatch logic

## Implementation Phases

1. **Phase 1**: Grid Exposure from InMemoryPbf - Add method to return proximity grid
   - See: `01_grid_exposure.md`
2. **Phase 2**: Intermediate Storage - Store grid and tiles to redb
   - See: `02_intermediate_storage.md`
3. **Phase 3**: Overlap Re-evaluation - Re-compute flags in overlap zones
   - See: `03_overlap_reevaluation.md`
4. **Phase 4**: Final Tile Writing - Merge, dedupe, write RMDF tiles
   - See: `04_final_tile_writing.md`
5. **Phase 5**: CLI Update - Add directory input support
   - See: `05_cli_update.md`

## Testing Strategy

### Unit Tests:
- Grid exposure: Verify new method returns both PBF and grid
- Grid storage: Round-trip serialization via `to_bytes()`/`from_bytes()`
- Overlap detection: Mock bounds, verify intersection calculation
- Grid merging: Verify `MAX` for sectors, `OR` for military
- Tile merging: Verify deduplication by OSM ID

### Integration Tests:
- Process two small overlapping mock PBF files
- Verify unified output with no duplicates
- Verify manifest includes both sources

### Manual Testing Steps:
1. Create two small overlapping PBF extracts (e.g., 10km border region)
2. Run multi-PBF generation: `ridi-router generate-tiles --input-dir ./input --output-dir ./output`
3. Inspect output tiles for duplicates (no OSM ID should appear twice in same tile)
4. Verify border nodes have correct `residential_in_proximity` flags
5. Verify manifest.json includes both source filenames

## Performance Considerations

- **Memory**: Sequential PBF processing (already designed) - max memory = largest single PBF
- **redb overhead**: Intermediate storage adds disk I/O, but enables merge/dedupe
- **Overlap re-evaluation**: Only processes nodes in overlap zones, not entire PBFs

## Migration Notes

No migration needed - this is a new feature. Existing single-PBF mode unchanged.

## References

- Original ticket: `thoughts/tickets/feature_multi_pbf_tile_generation.md`
- Research document: `thoughts/research/2026-02-21_multi_pbf_tile_generation.md`
- Grid serialization: `src/proximity/rasterized_grid.rs:277-345`
- Grid storage: `src/rmdf/generator/intermediate.rs:374-438`
- Tile merging: `src/rmdf/generator/intermediate.rs:102-138`

## Implementation Status

All phases completed successfully:

 [x] **Phase 1**: Grid Exposure from InMemoryPbf - Added `from_pbf_file_with_grid()` method
 [x] **Phase 2**: Intermediate Storage - Stores grid bytes and tiles to redb
 [x] **Phase 3**: Overlap Re-evaluation - Implements `reevaluate_overlap_zones()`
 [x] **Phase 4**: Final Tile Writing - Implements `write_final_tiles()`
 [x] **Phase 5**: CLI Update - Added `--input-dir` and `--db-path` options
