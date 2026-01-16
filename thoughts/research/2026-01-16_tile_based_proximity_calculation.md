---
date: 2026-01-16T23:46:04+02:00
git_commit: 36f9d49c2445e0467b7422068b02c852e4227778
branch: feat-ridi-map-format
repository: ridi-router
topic: "Tile-Based Proximity Calculation Refactoring"
tags: [research, codebase, proximity, nogo, areagrid, performance, memory, refactoring, tiles, pbf]
last_updated: 2026-01-16
---

## Ticket Synopsis

This research addresses technical debt ticket DEBT-001 which proposes refactoring the proximity and nogo area calculation from a global two-phase architecture to a fully parallel tile-based architecture. The current implementation has critical scalability issues that prevent processing planet-scale PBF files:

1. **Global AreaGrid Construction** - Creates AreaGrid for entire PBF file, causing memory exhaustion
2. **Excessive Disk I/O** - Writes proximity data to temporary redb database
3. **Missing Border Buffer** - No buffer zone around tiles causes incorrect boundary classification
4. **Critical Bug** - Proximity flags computed correctly but hardcoded to `false` in `GenerationGraph.insert_node()`

The target architecture processes tiles independently in parallel using Rayon, with tile-specific AreaGrids built from PBF data filtered by geographic bounds plus a 500m buffer zone.

## Summary

The current RMDF generation pipeline uses a sequential three-phase approach:
1. First PBF scan: Collect all node coordinates → redb
2. Compute proximity flags for ALL nodes in parallel (global AreaGrids)
3. Second PBF scan: Partition elements into tiles reading flags from redb

This architecture **successfully parallelizes proximity computation** but has critical flaws:
- Global AreaGrids require memory proportional to entire PBF (fails on planet.osm.pbf)
- Temporary database adds disk I/O overhead
- **CRITICAL BUG**: Computed flags are lost in `GenerationGraph.insert_node()` (lines 60-61)

The proposed tile-based architecture would:
- Process each tile independently with local AreaGrids (memory controlled by tile size)
- Eliminate intermediate database storage
- Add 500m buffer zones for correct border classification
- Scale to planet-level processing

## Detailed Findings

### Current Implementation Architecture

#### src/rmdf/generator/pbf_streamer.rs:58-165

**Phase 1: Node Coordinate Collection**
```
Lines 74-102: First PBF Pass
├─ Open PBF file with osmpbfreader
├─ Create node_coords.redb database
├─ For each node: INSERT (id, lat, lon, false, false)
└─ Result: All node coordinates with zero-initialized proximity flags
```

**Phase 2A: Parallel Proximity Computation** (Lines 313-372)
```
compute_proximity_parallel():
├─ extract_residential_areas() [375-385] - Build global AreaGrid
├─ extract_military_areas() [388-398] - Build global AreaGrid
├─ Load ALL nodes into memory [320-332] - ~800MB for 50M nodes
├─ Parallel computation with Rayon [337-347]
│  ├─ compute_residential_proximity() [401-432]
│  │  └─ 500m threshold, 10% coverage requirement
│  └─ compute_nogo_area() [435-453]
│     └─ 100m interior distance from military boundary
└─ Batch write results back to database [350-368]
```

**Phase 2B: Element Partitioning** (Lines 109-155)
```
Second PBF Pass:
├─ Read proximity flags from node_coords_db [174-179]
├─ Create OsmNode with flags [181-187]
├─ partition_node() - Assign to tile(s) [167-197]
│  └─ get_tiles_for_point() [458-502] - Handles borders
├─ partition_way_redb() [199-244]
├─ partition_relation_redb() [246-310]
├─ Incremental flush every 10K ways [137-140]
└─ Write to intermediate_tiles.redb
```

**Memory Characteristics:**
- Peak: ~1.2-1.6GB during Phase 2A (AreaGrids + all nodes)
- Disk: node_coords.redb (~1.1GB for 50M nodes, deleted after use)
- Disk: intermediate_tiles.redb (~0.5-2GB, deleted after RMDF writing)

### AreaGrid Implementation

#### src/map_data/proximity.rs:62-164

**Core Structure:**
```rust
pub struct AreaGrid {
    point_grid: PointGrid<MultiPolygon>,  // Lines 62-64
}

pub struct PointGrid<T: Clone> {
    grid: HashMap<GpsCellId, Vec<T>>,  // Lines 167-169
}

type GpsCellId = (i16, i16);  // Line 11
const GRID_CALC_PRECISION: i16 = 100;  // Line 15 (2 decimal places = ~1.1km cells)
```

**Grid Expansion Algorithm** (Lines 72-152)
```
insert_multi_polygon():
1. Round each polygon coordinate to 4 corners [106-120]
   └─ Ceil, Floor combinations for x,y

2. Expand in 4 directions until boundary [122-141]
   ├─ Direction::Up - Increment Y by 1/100
   ├─ Direction::Down - Decrement Y by 1/100
   ├─ Direction::Left - Decrement X by 1/100
   └─ Direction::Right - Increment X by 1/100
   └─ while multi_polygon.contains(&next_coord)

3. Insert MultiPolygon clone into each grid cell [145-151]
   └─ self.point_grid.insert(y, x, multi_polygon)
```

**Memory Intensive Factors:**
1. **Redundant Storage** - Each grid cell contains cloned MultiPolygon
   - One residential area spanning 100 cells = 100 clones
   - MultiPolygon avg size: ~5-20 KB
   - A city's residential areas (500-1000 cells) = 7.5-15 MB
   - Full country: 500 MB - 1 GB for global grids

2. **Query Pattern** (Lines 153-160, 246-261)
```
find_closest_areas_refs(lat, lon, steps):
├─ get_cell_id(lat, lon) - Returns (lat*100, lon*100)
├─ For step in 0..=steps:
│  ├─ get_outer_cell_ids() - Expanding square boundary
│  └─ Collect all MultiPolygons in those cells
└─ Returns Option<Vec<&MultiPolygon>>
```

### Critical Bug: Flag Loss in GenerationGraph

#### src/map_data/generation_graph.rs:53-67

**THE BUG:**
```rust
pub fn insert_node(&mut self, node: OsmNode) {
    let point = MapDataPoint {
        id: node.id,
        lat: node.lat as f32,
        lon: node.lon as f32,
        lines: Vec::new(),
        rules: Vec::new(),
        residential_in_proximity: false,  // Line 60 - HARDCODED!
        nogo_area: false,                 // Line 61 - HARDCODED!
    };
    // ...
}
```

**Impact:**
- OsmNode has computed flags from proximity calculation
- When converted to MapDataPoint, flags are explicitly set to `false`
- All proximity calculations are rendered non-functional
- RMDF files contain only `false` flags

**Fix Required:**
```rust
residential_in_proximity: node.residential_in_proximity,
nogo_area: node.nogo_area,
```

### Flag Propagation Path

**Complete Lifecycle:**
```
1. COMPUTATION (pbf_streamer.rs:313-453)
   ├─ compute_residential_proximity() - 500m threshold
   ├─ compute_nogo_area() - 100m military boundary
   └─ Store: NODE_COORDS_TABLE (lat, lon, bool, bool)

2. CREATION (pbf_streamer.rs:174-187)
   ├─ Read flags from database
   └─ Create OsmNode {residential_in_proximity, nogo_area}

3. INTERMEDIATE STORAGE (intermediate.rs:36-37)
   └─ IntermediateTile stores OsmNode with flags

4. ❌ BUG: CONVERSION (generation_graph.rs:60-61)
   └─ insert_node() LOSES FLAGS - hardcodes both to false

5. SERIALIZATION (writer.rs:238-247)
   ├─ Encode flags into PointRecord.flags (u16)
   ├─ Bit 0: residential_in_proximity
   └─ Bit 1: nogo_area
   └─ Problem: Already false from bug

6. USAGE IN ROUTING
   ├─ tile_manager.rs:120-122 - Filter by residential_in_proximity
   ├─ weights.rs:438-457 - Hard block routes through nogo_area
   └─ weights.rs:169 - Check residential for short detours
   └─ ❌ All checks fail - flags are false
```

### PBF Filtering and Geographic Bounds

#### src/rmdf/generator/pbf_streamer.rs:458-502

**Tile Assignment with Border Handling:**
```rust
get_tiles_for_point(lat, lon) → Vec<TileId>
├─ Primary tile: floor((lon+180)/tile_size), floor((lat+90)/tile_size)
├─ Horizontal border check (0.0001° tolerance)
│  ├─ Western edge: Add (col-1, row)
│  └─ Eastern edge: Add (col+1, row)
├─ Vertical border check
│  ├─ Southern edge: Add (col, row-1)
│  └─ Northern edge: Add (col, row+1)
└─ Corner cases: Return up to 4 tiles
```

**Border Tolerance:** 0.0001 degrees (~11 meters at equator)

#### src/osm_data/pbf_reader.rs:62-76

**PBF Filtering with get_objs_and_deps:**
```rust
let elements = pbf.get_objs_and_deps(|obj| {
    obj.is_way()
    && obj.tags().contains_key("highway")
    && ALLOWED_HIGHWAY_VALUES.contains(&obj.tag("highway"))
    && !(obj.tags().contains("motor_vehicle", "destination"))
    || (obj.is_way() && obj.tags().contains("motorcycle", "yes"))
})?;
```

**Dependency Resolution:**
- Automatically includes all nodes referenced by filtered ways
- Includes relations referencing those ways (turn restrictions)
- Ensures complete graph connectivity

### Rayon Parallel Patterns

#### src/rmdf/generator/pbf_streamer.rs:337-347

**Primary Pattern: Parallel Node Processing**
```rust
let results: Vec<(u64, bool, bool)> = nodes.par_iter()
    .map(|(node_id, lat, lon)| {
        let residential = Self::compute_residential_proximity(
            *lat as f64, *lon as f64, &residential_areas
        );
        let nogo = Self::compute_nogo_area(
            *lat as f64, *lon as f64, &military_areas
        );
        (*node_id, residential, nogo)
    })
    .collect();
```

**Characteristics:**
- Shared read-only access to AreaGrids (no cloning per thread)
- Infallible computation (no error handling needed)
- Results collected into Vec for batch database write

#### src/rmdf/generator/proximity.rs:76-79

**Alternative Pattern: try_for_each**
```rust
tile_ids.par_iter()
    .try_for_each(|tile_id| -> Result<()> {
        self.compute_tile(*tile_id, tiles_db)
    })?;
```

**Error Handling:**
- Short-circuits on first error
- Used for side-effect operations (database writes)

### PBF Reader Access Patterns

#### Multiple Sequential Passes

**src/rmdf/generator/pbf_streamer.rs** uses separate file handles:
```
Pass 1 (Lines 74-76): Collect node coordinates
  └─ File::open(&self.input_file) + OsmPbfReader::new()

Pass 2A (Line 316-317): Extract area grids
  ├─ extract_residential_areas() - Opens file again (375-378)
  └─ extract_military_areas() - Opens file again (389-391)

Pass 2B (Lines 113-115): Partition elements
  └─ File::open(&self.input_file) + OsmPbfReader::new()
```

**Pattern:**
- No reader cloning for multi-threading
- Separate file handles for each pass
- File dropped after scope, reopened for next pass
- Parallel iteration on extracted data, not on reader

#### src/osm_data/pbf_reader.rs:43-62

**Mutable Borrow Reuse:**
```rust
let mut pbf = osmpbfreader::OsmPbfReader::new(r);

let mut boundary_reader = PbfAreaReader::new(&mut pbf);  // First read
boundary_reader.read(...)?;

let mut boundary_reader = PbfAreaReader::new(&mut pbf);  // Second read
boundary_reader.read(...)?;

let elements = pbf.get_objs_and_deps(...)?;  // Third read
```

**Thread-safe approach:**
- Mutable borrowing (`&'a mut`), not cloning
- Sequential pass through PBF
- No concurrent PBF reading

### Tile-Based Processing Approach

#### Current Tile Discovery

**src/rmdf/generator/intermediate.rs:280-297**
```rust
pub fn discover_tiles_from_redb(db: &Database) -> Result<Vec<TileId>> {
    // Iterate all keys in TILE_NODES table
    // Extract unique (col, row) pairs
    // Sort by (col, row)
}
```

**src/rmdf/generator/mod.rs:65-71**
```
FOR each tile_id (sequential):
  ├─ Load IntermediateTile from redb
  ├─ Build GenerationGraph (with BUG at insert_node)
  ├─ Build spatial index
  ├─ Serialize all sections
  └─ Write tile_###_###.rmdf
```

#### Proposed Tile-Based Proximity Computation

**Pattern from src/rmdf/generator/proximity.rs:86-119** (deprecated but shows approach):
```rust
fn compute_tile(tile_id: TileId) -> Result<()> {
    // 1. Compute tile bounds
    let core_bounds = self.compute_tile_bounds(tile_id);

    // 2. Add 500m buffer
    let buffer_degrees = 500.0 / 111_000.0;  // ~0.0045°
    let buffered_bounds = TileBounds {
        lat_min: (core_bounds.lat_min - buffer_degrees).max(-90.0),
        lat_max: (core_bounds.lat_max + buffer_degrees).min(90.0),
        lon_min: (core_bounds.lon_min - buffer_degrees).max(-180.0),
        lon_max: (core_bounds.lon_max + buffer_degrees).min(180.0),
    };

    // 3. Extract areas only in buffered bounds
    let residential = extract_areas_in_bounds(buffered_bounds)?;
    let military = extract_areas_in_bounds(buffered_bounds)?;

    // 4. Compute proximity for nodes in tile
    for node in tile.nodes:
        if point_in_bounds(node.lat, node.lon, core_bounds):
            node.residential_in_proximity = compute_residential_proximity(...);
            node.nogo_area = compute_nogo_area(...);

    // 5. Grids dropped here (memory reclaimed)
}
```

**Benefits:**
- Memory: ~10-50x reduction (only areas affecting current tile)
- Parallelization: Process tiles independently with Rayon
- No intermediate database for proximity flags

## Code References

### Key Implementation Files

- `src/rmdf/generator/pbf_streamer.rs:58-165` - Main partition pipeline
- `src/rmdf/generator/pbf_streamer.rs:313-372` - Parallel proximity computation
- `src/rmdf/generator/pbf_streamer.rs:401-453` - Proximity algorithms
- `src/map_data/proximity.rs:62-164` - AreaGrid implementation
- `src/map_data/proximity.rs:246-261` - Grid query algorithm
- `src/map_data/generation_graph.rs:53-67` - **BUG LOCATION**
- `src/rmdf/generator/writer.rs:238-247` - Flag serialization to RMDF
- `src/rmdf/format.rs:94-108` - PointRecord flag encoding
- `src/router/weights.rs:438-457` - NoGo area routing avoidance
- `src/router/weights.rs:169` - Residential proximity routing weight

### Database Schema

**node_coords.redb (Temporary):**
- Table: `NODE_COORDS_TABLE`
- Key: `u64` (node ID)
- Value: `(lat: f32, lon: f32, residential: bool, nogo: bool)`
- Size: ~24 bytes per node
- Lifecycle: Created in Phase 2.1 → Updated in Phase 2A → Deleted in Phase 2.3

**intermediate_tiles.redb (Temporary):**
- Table: `TILE_NODES` - Key: `(col: u16, row: u16, osm_id: u64)`, Value: bincode-serialized OsmNode
- Table: `TILE_WAYS` - Key: `(col: u16, row: u16, way_id: u64)`, Value: bincode-serialized OsmWay
- Table: `TILE_RELATIONS` - Key: `(col: u16, row: u16, rel_id: u64)`, Value: bincode-serialized OsmRelation
- Lifecycle: Written during Phase 2.3 → Read during Phase 3 → Deleted after RMDF writing

### Constants

```rust
// src/rmdf/generator/pbf_streamer.rs:26-32
const RESIDENTIAL_PROXIMITY_THRESHOLD_METERS: f64 = 500.0;
const RESIDENTIAL_PART_COVERED: f64 = 0.10;
const THRESHOLD_AREA: f64 = 78,539.82;  // π × 500² × 0.10
const MILITARY_ENTRY_MAX_M: f64 = 100.0;
const FLUSH_INTERVAL: usize = 10_000;  // Ways between database flushes

// src/map_data/proximity.rs:14-15
const GRID_CALC_DECIMAL_PLACES: usize = 2;
const GRID_CALC_PRECISION: i16 = 100;  // ~1.1km per cell
```

## Architecture Insights

### Current Architecture Strengths

1. **Effective Parallelization** - Successfully parallelizes proximity computation across all nodes
2. **Incremental Flushing** - Prevents unbounded memory growth during partitioning (10K way intervals)
3. **Single Transactions** - Batch database writes minimize disk I/O
4. **Border Handling** - 0.0001° tolerance for tile boundary duplication

### Current Architecture Weaknesses

1. **Global AreaGrids** - Memory scales with entire PBF, not tile size
   - Redundant MultiPolygon cloning (100s of copies per area)
   - Full-country grids: 500 MB - 1 GB
   - Fails on planet.osm.pbf

2. **Intermediate Database Overhead**
   - node_coords.redb: ~1.1GB for 50M nodes
   - Disk I/O for write + read + delete
   - Could be eliminated with tile-based approach

3. **CRITICAL BUG** - Proximity flags computed but lost
   - `GenerationGraph.insert_node()` hardcodes flags to false
   - All proximity calculations non-functional in routing
   - **Must fix before refactoring**

4. **Missing Border Buffers**
   - No buffer zone around tiles during proximity computation
   - Nodes near boundaries may be incorrectly classified

### Target Architecture Benefits

1. **Controlled Memory** - Tile size controls peak memory, not PBF size
   - AreaGrids scoped to tile + 500m buffer
   - ~10-50x memory reduction
   - Scales to planet.osm.pbf

2. **Eliminated Disk I/O** - No intermediate database
   - Direct PBF → GenerationGraph → RMDF
   - Faster processing (no database sync overhead)

3. **Buffer Zones** - 500m buffer ensures correct border classification
   - AreaGrid includes areas within 500m of tile boundary
   - Nodes near edges get correct proximity classification

4. **Parallel Tile Processing** - Rayon par_iter over tiles
   - Each tile independently: read PBF → build AreaGrids → compute → write RMDF
   - No shared state between tiles

### Implementation Challenges

1. **PBF Filtering by Bounds**
   - Need to filter PBF data by geographic bounds efficiently
   - `get_objs_and_deps` doesn't natively support bounds filtering
   - May need to iterate and filter manually

2. **Duplicate Processing**
   - Ways/relations spanning tiles processed multiple times
   - Acceptable cost for independence

3. **Buffer Zone Calculation**
   - Fixed degree offset must guarantee ≥500m at all latitudes
   - Latitude-dependent meter-to-degree conversion
   - Or simple over-buffering at equator

4. **Rayon Error Handling**
   - Tile failures must propagate correctly
   - `try_for_each` pattern from proximity.rs:76-79

## Historical Context (from thoughts/)

### thoughts/tickets/debt_tile_based_proximity_calculation.md

Complete specification of the proposed refactoring:
- Target architecture: Parallel tile processing with buffer zones
- Functional requirements: Tile-based AreaGrid, eliminate databases, parallel processing
- Success criteria: Unit tests, performance tests, code quality checks
- Implementation priority: Fix bug first, then tile-based AreaGrid

### thoughts/plans/rmdf_memory_mapped_tiles/03_proximity_computation.md

Phase 3 plan document showing proximity computation design:
- 500m buffer overlap strategy
- Parallel per-tile computation with rayon
- Residential area and military zone detection
- Direct PBF → tile processing flow

### thoughts/reviews/rmdf_memory_mapped_tiles-review.md

Comprehensive validation report:
- Implementation status by phase
- Critical findings about proximity calculation
- Recommendations for optimization

### thoughts/research/2026-01-14_rmdf_memory_mapped_tiles.md

Previous research on RMDF format:
- Current architecture analysis
- Serialization and caching system examination
- Spatial indexing investigation
- Platform compatibility analysis

## Related Research

- `thoughts/research/2026-01-14_rmdf_memory_mapped_tiles.md` - RMDF format and generation architecture
- `thoughts/plans/rmdf_memory_mapped_tiles/` - 9-phase implementation plan
- `thoughts/tickets/feature_rmdf_memory_mapped_tiles.md` - Original feature specification

## Open Questions

1. **PBF Bounds Filtering** - How to efficiently filter PBF data by geographic bounds?
   - Does osmpbfreader support spatial filtering?
   - Need to iterate full PBF for each tile?
   - Performance impact of multiple PBF passes?

2. **Buffer Zone Precision** - What's the optimal buffer calculation?
   - Fixed degree offset (simple, over-buffered at equator)
   - Latitude-adjusted (complex, precise)
   - Impact on memory/performance trade-offs?

3. **Tile Processing Order** - Sequential or fully parallel?
   - Memory constraints for parallel tile processing
   - How many tiles can be processed concurrently?
   - File descriptor limits for concurrent PBF reading?

4. **Bug Fix Timing** - Fix bug before or after refactoring?
   - **Recommendation: Fix immediately** - current system is broken
   - Simple 2-line fix enables validation of refactoring
   - Can verify flag propagation with unit tests

5. **Backward Compatibility** - Support both architectures during transition?
   - Feature flag for old vs new implementation?
   - Or complete replacement?
   - Testing strategy for equivalence?

6. **Performance Validation** - How to measure success?
   - Baseline current implementation (with bug fixed)
   - Compare memory usage on large PBFs
   - Measure disk I/O reduction
   - Processing time comparison

## Recommendations

### Immediate Actions (Critical Priority)

1. **Fix GenerationGraph Bug** (src/map_data/generation_graph.rs:60-61)
   ```rust
   residential_in_proximity: node.residential_in_proximity,
   nogo_area: node.nogo_area,
   ```
   - Add unit test to prevent regression
   - Verify flags in RMDF output
   - Test routing behavior with correct flags

2. **Validate Current System** - Ensure proximity calculation works correctly after bug fix
   - Generate test RMDF files
   - Inspect PointRecord flags
   - Test routing with residential proximity and nogo areas

### Refactoring Implementation (High Priority)

3. **Research PBF Bounds Filtering**
   - Investigate osmpbfreader API for spatial filtering
   - Prototype tile+buffer extraction
   - Measure performance impact

4. **Implement Tile-Based AreaGrid**
   - Start with single tile (non-parallel) proof of concept
   - Add 500m buffer zone calculation
   - Verify border node classification

5. **Remove Intermediate Database**
   - Direct PBF → GenerationGraph flow
   - Eliminate node_coords.redb
   - Benchmark I/O reduction

6. **Add Parallel Tile Processing**
   - Rayon par_iter over tiles
   - Error propagation with try_for_each
   - Memory monitoring

### Testing and Validation (Medium Priority)

7. **Unit Tests**
   - Buffer zone degree calculation
   - Tile boundary node handling
   - Flag propagation regression test
   - AreaGrid scoping

8. **Integration Tests**
   - Compare old vs new implementation output
   - Large PBF processing (multi-GB)
   - Memory usage profiling

9. **Performance Benchmarks**
   - Disk I/O measurement
   - Peak memory usage
   - Processing time comparison
   - Planet.osm.pbf feasibility test
