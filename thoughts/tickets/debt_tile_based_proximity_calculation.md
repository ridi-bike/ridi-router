---
type: debt
priority: high
created: 2026-01-16T00:00:00Z
status: planned
tags: [proximity, nogo, areagrid, performance, memory, refactoring, tiles, pbf]
plan_document: thoughts/plans/tile_based_proximity_calculation/00_overview.md
keywords: [AreaGrid, proximity, nogo, PbfStreamer, compute_proximity_parallel, partition_node, partition_way, partition_relation, node_coords_db, IntermediateTile, GenerationGraph, get_objs_and_deps, RESIDENTIAL_PROXIMITY_THRESHOLD_METERS, TileId, TileBounds, rayon, parallel]
patterns: [tile-based processing, parallel iteration, geographic filtering, buffer zones, direct conversion, flag propagation]
research_document: thoughts/research/2026-01-16_tile_based_proximity_calculation.md
---

# DEBT-001: Refactor Proximity/NoGo Calculation to Tile-Based Architecture

## Description

The current proximity and nogo area calculation has critical scalability and performance issues that prevent processing global PBF files:

1. **Global AreaGrid Construction**: Creates AreaGrid for the entire PBF file, causing memory exhaustion when processing planet-scale data
2. **Excessive Disk I/O**: Writes proximity data to temporary redb database, adding unnecessary disk operations
3. **Missing Border Buffer**: No buffer zone around tiles, causing incorrect classification for nodes near tile boundaries
4. **Critical Bug**: Proximity flags are computed correctly but then hardcoded to `false` in `GenerationGraph.insert_node()` (lines 60-61 in `src/map_data/generation_graph.rs`), rendering all proximity calculations ineffective

This refactoring fundamentally restructures the processing pipeline from a global two-phase approach to a fully parallel tile-based architecture.

## Context

### Current Architecture (Inefficient)
```
1. First PBF scan: Collect all node coordinates → redb
2. Second PBF scan: Extract all residential/military areas (global AreaGrid)
3. Load all nodes into memory
4. Compute proximity in parallel (global processing)
5. Write flags back to redb
6. Partition into tiles reading from redb
7. Delete temporary database
```

**Problems**:
- Global AreaGrid requires memory proportional to entire PBF (fails on planet.osm.pbf)
- Temporary database adds disk I/O overhead
- Multiple full PBF scans
- No tile boundary buffering causes edge classification errors

### Target Architecture (Efficient)
```
1. Calculate tile boundaries (based on tile_size_degrees)
2. Rayon parallel iteration over all tiles:
   For each tile independently:
   a. Calculate tile + RESIDENTIAL_PROXIMITY_THRESHOLD_METERS buffer zone
   b. Read PBF with get_objs_and_deps filtering by buffered bounds
   c. Build tile-specific AreaGrid (residential + military)
   d. Compute proximity/nogo flags for all nodes
   e. Create GenerationGraph directly
   f. Write RMDF tile file
```

**Benefits**:
- Memory footprint controlled by tile size, not global PBF size
- No intermediate disk storage
- Rayon automatically manages concurrency
- Buffer zone ensures correct border node classification
- Scales to planet-level processing

## Requirements

### Functional Requirements

#### 1. Tile-Based AreaGrid Construction
- **Scope**: Build separate AreaGrid for each tile + buffer zone, not global
- **Buffer Zone**: Use `RESIDENTIAL_PROXIMITY_THRESHOLD_METERS` constant (500m) as buffer on all tile sides
- **Buffer Calculation**: Use fixed degree offset that guarantees >= 500m at all latitudes (simpler, slightly over-buffered at equator)
- **Polygon Filtering**: Filter residential/military areas by bounding box intersection with buffered tile bounds
- **Polygon Inclusion**: Keep full polygons even if they extend beyond buffer zone (don't clip)
- **Grid Precision**: Maintain current 2 decimal place precision (~1.1km cells)
- **Edge Handling**: Skip buffer zone for world edges (poles/date line) - irrelevant ocean/ice areas

#### 2. Eliminate Intermediate Database Storage
- **Remove**: All `node_coords_db` redb database code
- **Remove**: Temporary storage of proximity flags
- **Direct Flow**: PBF → GenerationGraph → RMDF (no intermediate persistence)
- **Keep**: Final RMDF tile files still contain proximity flags in PointRecord

#### 3. Parallel Tile Processing
- **Parallelization**: Process all tiles in one rayon `par_iter` iteration
- **Independence**: Each tile independently reads PBF, builds AreaGrid, computes flags
- **PBF Reader**: Clone reader for each tile, or construct new reader against same file
- **Concurrency Control**: Rely on rayon for automatic work distribution (no manual memory limits)
- **Deduplication**: Accept that ways/relations spanning tiles are processed multiple times

#### 4. PBF Filtering Per Tile
- **Method**: Use `get_objs_and_deps` to filter PBF data by geographic bounds
- **Filter Logic**: Filter nodes by tile + buffer bounds, retrieve dependent ways/relations
- **Trust Dependencies**: Trust `get_objs_and_deps` to correctly resolve way/relation dependencies
- **Node Selection**: Only load nodes/coordinates for current tile + buffer

#### 5. Fix GenerationGraph Bug
- **Bug Location**: `src/map_data/generation_graph.rs:60-61`
- **Current Code**: Hardcodes `residential_in_proximity: false, nogo_area: false`
- **Fix**: Use actual computed values from OsmNode: `residential_in_proximity: node.residential_in_proximity, nogo_area: node.nogo_area`
- **Verification**: Ensure flags propagate correctly to `RmdfWriter::serialize_points()`
- **Testing**: Add unit test to prevent regression

#### 6. NoGo Area Calculation
- **Same Architecture**: Apply tile-based AreaGrid approach to nogo (military) areas
- **Same Buffer**: Use same tile + buffer zone approach as proximity
- **Keep Logic**: Maintain existing distance-from-boundary logic (>100m inside military area)
- **Independent Grids**: Build separate AreaGrid for residential and military per tile

### Non-Functional Requirements

#### Performance
- **Primary Goal**: Eliminate disk I/O (read/write to temporary database)
- **Memory Control**: Use tiles to control memory footprint via rayon concurrency
- **Scalability**: Must handle planet.osm.pbf without memory exhaustion
- **Speed**: Reduce overall processing time by eliminating database operations

#### Code Quality
- **Complete Replacement**: Remove old implementation entirely (no feature flags)
- **Clean Removal**: Delete all obsolete/deprecated code
- **Constants**: Reuse `RESIDENTIAL_PROXIMITY_THRESHOLD_METERS` directly for buffer calculations
- **Documentation**: Link buffer zone constant to proximity threshold in code comments

#### Error Handling
- **Tile Failure**: Fail entire generation if any tile fails
- **Fallback**: If AreaGrid construction fails for a tile, mark all proximity flags as `false`
- **Progress Reporting**: Keep current progress reporting mechanism

#### Configuration
- **Tile Size**: Keep existing `tile_size_degrees` configuration approach
- **Buffer Zone**: Keep `RESIDENTIAL_PROXIMITY_THRESHOLD_METERS` as compile-time constant
- **Tile Boundaries**: Calculate same way as currently done (based on `tile_size_degrees`)

## Current State

### Code Locations
- **Proximity Calculation**: `src/rmdf/generator/pbf_streamer.rs:313-453`
- **AreaGrid**: `src/map_data/proximity.rs:62-165`
- **PBF Processing**: `src/rmdf/generator/pbf_streamer.rs:55-159`
- **Bug Location**: `src/map_data/generation_graph.rs:60-61`
- **Intermediate Storage**: `src/rmdf/generator/intermediate.rs`

### Current Flow
1. `PbfStreamer::partition()` - Main entry point
2. First pass (lines 73-102): Collect node coordinates → `NODE_COORDS_TABLE`
3. `compute_proximity_parallel()` (lines 313-372):
   - Extract global residential/military AreaGrids
   - Load all nodes into memory
   - Parallel compute proximity/nogo flags
   - Write back to database
4. Second pass (lines 109-155): Partition by tile
   - `partition_node()` - reads flags from database
   - `partition_way_redb()` / `partition_relation_redb()`
5. Write IntermediateTile to redb
6. Delete NODE_COORDS_TABLE
7. Later: RmdfWriter reads IntermediateTile → writes RMDF

### Critical Bug
```rust
// src/map_data/generation_graph.rs:53-67
pub fn insert_node(&mut self, node: OsmNode) {
    let point = MapDataPoint {
        id: node.id,
        lat: node.lat as f32,
        lon: node.lon as f32,
        lines: Vec::new(),
        rules: Vec::new(),
        residential_in_proximity: false,  // ← BUG: Should be node.residential_in_proximity
        nogo_area: false,                 // ← BUG: Should be node.nogo_area
    };
    // ...
}
```

**Impact**: All proximity flags in final RMDF files are `false`, making proximity calculation completely non-functional.

## Desired State

### New Flow
```
1. Calculate all tile boundaries (based on tile_size_degrees)
2. tiles.par_iter().map(|tile_id| {
     a. Calculate buffered_bounds = tile_bounds + RESIDENTIAL_PROXIMITY_THRESHOLD_METERS
     b. Create PBF reader (clone or new instance)
     c. Filter PBF: get_objs_and_deps(buffered_bounds)
     d. Extract residential areas → AreaGrid (tile-specific)
     e. Extract military areas → AreaGrid (tile-specific)
     f. For each node in tile:
        - compute_residential_proximity(node, residential_grid)
        - compute_nogo_area(node, military_grid)
     g. Build GenerationGraph directly (with correct flags)
     h. Write RMDF tile file
     i. Drop AreaGrids (automatic)
   }).collect()
3. Generate manifest
```

### Code Changes
- **Remove**: `node_coords_db` database code
- **Remove**: `compute_proximity_parallel()` function
- **Remove**: `partition_node/way/relation_redb()` functions
- **Remove**: `IntermediateTile` struct and intermediate database
- **Remove**: All obsolete/deprecated code
- **Modify**: `PbfStreamer::partition()` to implement new tile-based flow
- **Fix**: `GenerationGraph::insert_node()` to use actual flag values
- **Add**: `extract_areas_for_bounds()` to filter areas by tile + buffer
- **Add**: Buffer zone calculation utilities

## Research Context

### Keywords to Search
- **AreaGrid** - Core spatial data structure for proximity, uses PointGrid internally
- **proximity** - Proximity calculation logic, uses 500m threshold and area calculation
- **nogo** - NoGo area calculation, uses 100m interior distance threshold
- **PbfStreamer** - Main PBF processing orchestrator class
- **compute_proximity_parallel** - Current global proximity computation (to be replaced)
- **partition_node, partition_way, partition_relation** - Current partitioning functions (to be replaced)
- **node_coords_db** - Temporary database for node coordinates (to be removed)
- **IntermediateTile** - Intermediate storage structure (to be removed)
- **GenerationGraph** - Target graph structure for direct conversion
- **get_objs_and_deps** - PBF filtering method for geographic bounds
- **RESIDENTIAL_PROXIMITY_THRESHOLD_METERS** - 500m constant for both proximity and buffer
- **MILITARY_ENTRY_MAX_M** - 100m constant for nogo interior threshold
- **TileId, TileBounds** - Tile identification and boundary structures
- **rayon** - Parallelization library for tile-level concurrency

### Patterns to Investigate
- **PBF reading and filtering** - How to use `get_objs_and_deps` with geographic bounds
- **Parallel tile processing** - Using rayon `par_iter` for independent tile work
- **AreaGrid construction** - Building PointGrid from filtered polygon sets
- **Proximity calculation** - Distance-to-polygon and area threshold logic
- **Direct PBF to GenerationGraph** - Skipping intermediate storage layers
- **Error handling in parallel** - Propagating errors from rayon workers
- **Flag propagation** - Ensuring computed flags reach final RMDF PointRecord
- **Buffer zone math** - Converting meters to degrees with latitude compensation
- **PBF reader cloning** - Thread-safe PBF access patterns

### Key Decisions Made
- **Buffer Zone**: Fixed degree offset ensuring >= 500m globally (not latitude-adjusted)
- **Parallelization**: Process all tiles in single rayon iteration, no batching
- **PBF Access**: Clone reader or create new instance per tile
- **Code Removal**: Complete replacement, remove all intermediate database code
- **Error Strategy**: Fail entire generation on any tile error
- **Deduplication**: Accept duplicate processing of cross-tile ways/relations
- **Tile Size**: Keep existing `tile_size_degrees` configuration
- **AreaGrid Scope**: Per-tile construction with full polygons (no clipping)
- **Constants**: Reuse `RESIDENTIAL_PROXIMITY_THRESHOLD_METERS` for buffer
- **Testing**: Unit tests required, integration tests out of scope
- **Progress**: Keep current reporting mechanism

## Success Criteria

### Automated Verification

#### Unit Tests
- [ ] Test buffer zone calculation converts 500m to correct degree offset
- [ ] Test AreaGrid construction from filtered polygons (tile + buffer scope)
- [ ] Test proximity calculation with tile boundary nodes (edge cases)
- [ ] Test nogo calculation with tile boundary nodes
- [ ] Test GenerationGraph.insert_node() correctly propagates flags (regression test)
- [ ] Test flag serialization to RMDF PointRecord
- [ ] Test fallback behavior when AreaGrid construction fails (all flags = false)

#### Functional Tests
- [ ] Compare proximity flags between old and new implementation (sample tiles)
- [ ] Verify nodes near tile boundaries have correct flags with buffer zone
- [ ] Verify flags correctly stored in final RMDF files
- [ ] Verify no redb intermediate databases created during processing

#### Performance Tests
- [ ] Measure disk I/O reduction (should eliminate database writes/reads)
- [ ] Measure peak memory usage with large tiles
- [ ] Verify processing completes successfully on large PBF files (multi-GB)

### Manual Verification
- [ ] No temporary `node_coords_db` files created during generation
- [ ] No intermediate tile database files persist after generation
- [ ] Proximity flags visible in RMDF output (not all false)
- [ ] Nodes within 500m of residential areas correctly flagged
- [ ] Nodes inside military areas (>100m from boundary) correctly flagged as nogo
- [ ] Tiles can be generated in parallel without errors
- [ ] Memory usage remains bounded during planet-scale processing

### Code Quality Checks
- [ ] All `node_coords_db` references removed
- [ ] All `IntermediateTile` references removed
- [ ] All `compute_proximity_parallel` code removed
- [ ] All `partition_*_redb` functions removed
- [ ] No obsolete/deprecated code remains
- [ ] Buffer zone uses `RESIDENTIAL_PROXIMITY_THRESHOLD_METERS` constant
- [ ] Code comments explain buffer zone rationale

## Related Information

### Related Files
- `src/rmdf/generator/pbf_streamer.rs` - Main refactoring target
- `src/map_data/proximity.rs` - AreaGrid implementation (keep as-is)
- `src/map_data/generation_graph.rs` - Bug fix location
- `src/rmdf/generator/intermediate.rs` - To be removed
- `src/rmdf/generator/writer.rs` - Flag serialization verification
- `src/rmdf/format.rs` - PointRecord flag definitions

### Constants
- `RESIDENTIAL_PROXIMITY_THRESHOLD_METERS = 500.0` - Primary buffer/proximity constant
- `THRESHOLD_AREA ≈ 7852.5 m²` - 10% of 500m radius circle
- `MILITARY_ENTRY_MAX_M = 100.0` - NoGo interior distance threshold
- `GRID_CALC_PRECISION = 100` - 2 decimal places for grid cells

### Technical Constraints
- PBF file may be planet-scale (50+ GB)
- Tile size configurable (e.g., 1.0 degree ≈ 111km)
- AreaGrid uses Haversine distance calculations
- RMDF format stores flags as 2 bits in PointRecord.flags (u16)

## Notes

### Architectural Impact
This is a fundamental restructuring of the RMDF generation pipeline:
- **Old**: Sequential phases with global state and intermediate persistence
- **New**: Parallel tile-independent processing with no shared state

The change eliminates the bottleneck preventing planet-scale processing and significantly improves performance by removing disk I/O.

### Buffer Zone Rationale
The 500m buffer zone around each tile ensures nodes near tile boundaries are classified correctly. Without the buffer:
- A node 100m inside a tile boundary might be within 500m of a residential area just outside the tile
- The tile-specific AreaGrid wouldn't include that external area
- The node would be incorrectly classified as not near residential

With buffer:
- AreaGrid includes all areas within 500m of tile boundary
- Nodes near edges get correct proximity classification
- Slight overlap between tiles is acceptable (independent processing)

### Implementation Priority
1. **Critical**: Fix GenerationGraph bug first (lines 60-61) - current system is broken
2. **High**: Implement tile-based AreaGrid construction
3. **High**: Remove intermediate database code
4. **Medium**: Add buffer zone handling
5. **Medium**: Add unit tests

### Research Phase Next Steps
Research should focus on:
1. Understanding `get_objs_and_deps` PBF filtering API
2. Finding PBF reader cloning patterns in existing code
3. Identifying all code paths that reference `node_coords_db`
4. Mapping complete data flow from PBF to RMDF
5. Understanding rayon parallel error propagation
