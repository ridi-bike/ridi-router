# Tile-Based Proximity Calculation Implementation Plan

## Overview

This plan refactors the proximity and nogo area calculation from a global two-phase architecture to a fully parallel tile-based architecture. The current implementation has critical scalability issues preventing planet-scale PBF processing, including global AreaGrid construction, excessive disk I/O, and missing border buffers.

The refactoring transforms the pipeline from sequential processing with global state to independent parallel tile processing with no shared state, eliminating memory bottlenecks and enabling planet.osm.pbf processing.

## Current State Analysis

### Current Architecture (Inefficient)

**File:** `src/rmdf/generator/pbf_streamer.rs`

```
1. First PBF scan (lines 73-103): Collect all node coordinates → node_coords.redb
2. Phase 2A (lines 313-372): Extract global AreaGrids, compute proximity in parallel
3. Second PBF scan (lines 109-155): Partition elements by tile reading from redb
4. Write to intermediate_tiles.redb
5. Delete node_coords.redb
6. Later: Load from intermediate_tiles.redb → write RMDF
```

### Critical Problems

1. **Global AreaGrid Construction** (lines 316-317)
   - Creates AreaGrid for entire PBF file
   - Memory scales with entire PBF, not tile size
   - Fails on planet.osm.pbf due to memory exhaustion

2. **Excessive Disk I/O** (lines 62-160)
   - Temporary `node_coords.redb` database (~1.1GB for 50M nodes)
   - Multiple read/write cycles before deletion
   - Temporary `intermediate_tiles.redb` database

3. **Missing Border Buffer**
   - No buffer zone around tiles during proximity computation
   - Nodes near tile boundaries miss areas just outside the tile
   - Incorrect classification for edge nodes

4. **Critical Bug** (`generation_graph.rs:60-61`)
   - Proximity flags hardcoded to `false` when converting OsmNode → MapDataPoint
   - All proximity calculations non-functional in current system
   - Will be fixed by new implementation

### Key Discoveries

- **PBF Filtering**: osmpbfreader doesn't support native geographic bounds filtering
- **Thread Safety**: OsmPbfReader requires `&mut`, cannot clone - must reopen file per tile
- **Rayon Pattern**: Current code successfully uses par_iter (pbf_streamer.rs:337-347)
- **AreaGrid Cloning**: Each grid cell stores cloned MultiPolygon (memory intensive but necessary)
- **Constants**: `RESIDENTIAL_PROXIMITY_THRESHOLD_METERS = 500.0` (also buffer size)

## Desired End State

### Target Architecture (Efficient)

```
1. Calculate all tile boundaries (based on tile_size_degrees)
2. tiles.par_iter().try_for_each(|tile_id| {
     a. Calculate buffered_bounds = tile_bounds + 500m buffer
     b. Open new PBF reader for this tile
     c. Filter PBF: Extract nodes/ways/relations in buffered bounds
     d. Build tile-specific residential AreaGrid
     e. Build tile-specific military AreaGrid
     f. For each node in tile core bounds:
        - compute_residential_proximity(node, residential_grid)
        - compute_nogo_area(node, military_grid)
     g. Build GenerationGraph directly with correct flags
     h. Write RMDF tile file
     i. Drop AreaGrids (memory reclaimed)
   })
3. Generate manifest
```

### Benefits

- **Controlled Memory**: Tile size controls peak memory, not PBF size (~10-50x reduction)
- **Eliminated Disk I/O**: Direct PBF → GenerationGraph → RMDF (no intermediate databases)
- **Buffer Zones**: 500m buffer ensures correct border node classification
- **Parallel Processing**: Rayon handles all tiles independently with no shared state
- **Scalability**: Handles planet.osm.pbf without memory exhaustion

## What We're NOT Doing

- Fixing the GenerationGraph bug separately (will be fixed by new implementation)
- Backward compatibility or feature flags (clean cutover)
- Comparing outputs with old implementation
- Keeping any intermediate database code
- Performance benchmarking beyond validation
- Documentation updates (separate task)

## Implementation Approach

The implementation follows an incremental strategy:

1. **Build new parallel structure** - Set up tile iteration and skeleton
2. **Implement tile processing** - Add each step incrementally (extract, compute, write)
3. **Complete the new flow** - Wire everything together end-to-end
4. **Remove old code** - Delete all obsolete implementation
5. **Test and validate** - Ensure correctness and performance

This approach allows testing at each step and minimizes risk.

## Implementation Phases

### Phase 1: Parallel Tile Loop Structure
Set up the parallel tile iteration framework with rayon and calculate tile boundaries.
- See: `01_parallel_tile_loop.md`

### Phase 2: Main Flow Outline
Structure the complete per-tile processing pipeline with stub implementations.
- See: `02_main_flow_outline.md`

### Phase 3: PBF Extraction and Filtering
Implement geographic bounds filtering to extract tile-specific PBF data.
- See: `03_pbf_extraction_filtering.md`

### Phase 4: Proximity and NoGo Computation
Build tile-specific AreaGrids and compute proximity flags.
- See: `04_proximity_nogo_computation.md`

### Phase 5: Tile Writing
Build GenerationGraph with correct flags and write RMDF files.
- See: `05_tile_writing.md`

### Phase 6: Remaining Bits
Add error handling, progress reporting, and edge case handling.
- See: `06_remaining_bits.md`

### Phase 7: Remove Obsolete Code
Delete all old implementation code and intermediate database structures.
- See: `07_remove_obsolete_code.md`

### Phase 8: Testing and Cleanup
Add comprehensive tests and validate the implementation.
- See: `08_testing_cleanup.md`

## Testing Strategy

### Automated Verification

#### Unit Tests
- Buffer zone calculation (500m → degrees)
- Tile boundary calculation and iteration
- Geographic bounds filtering for PBF data
- Flag propagation from computation to RMDF
- AreaGrid construction from filtered polygons

#### Integration Tests
- Process large PBF file (multi-GB) successfully
- Verify RMDF files contain correct proximity flags
- Verify memory usage stays bounded
- Verify all tiles processed without errors

### Manual Verification
- No temporary database files created during generation
- Proximity flags visible in RMDF output (not all false)
- Tiles can be generated in parallel without errors
- Memory usage remains stable during processing

## Performance Considerations

### Memory

- **Before**: Global AreaGrids (500MB - 1GB for country-scale)
- **After**: Per-tile AreaGrids (controlled by rayon concurrency)
- **Expected**: 10-50x reduction in peak memory usage

### Disk I/O

- **Before**: Write node_coords.redb + intermediate_tiles.redb + read both + delete
- **After**: Direct PBF → RMDF (only final tile writes)
- **Expected**: Significant I/O reduction

### Processing Time

- **Before**: Sequential phases with database overhead
- **After**: Parallel tile processing with Rayon
- **Expected**: Faster overall due to parallelism and no database overhead

## Migration Notes

### Breaking Changes

- Complete replacement of partition pipeline
- Intermediate database format no longer used
- Processing flow fundamentally different

### Compatibility

- Final RMDF format unchanged
- Manifest generation unchanged
- No migration of existing data needed (regenerate from PBF)

## References

- Original ticket: `thoughts/tickets/debt_tile_based_proximity_calculation.md`
- Research document: `thoughts/research/2026-01-16_tile_based_proximity_calculation.md`
- Current implementation: `src/rmdf/generator/pbf_streamer.rs`
- AreaGrid: `src/map_data/proximity.rs`
- GenerationGraph: `src/map_data/generation_graph.rs`
