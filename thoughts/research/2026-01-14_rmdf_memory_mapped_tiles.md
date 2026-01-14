---
date: 2026-01-14T00:02:14+02:00
git_commit: 96f32699cc5b5c634886fe14a093ed6cacb16c03
branch: feat-ridi-map-format
repository: ridi-router
topic: "RMDF Memory-Mapped Tile Format Implementation"
tags: [research, codebase, map-data, memory-mapping, tiling, rmdf, architecture]
last_updated: 2026-01-14
---

## Ticket Synopsis

**FEATURE-001: RMDF Memory-Mapped Tile Format**

Replace the current bincode-based binary serialization format with a custom memory-mapped file format (RMDF - Ridi Map Data Format) that supports tiling, zero-copy access, and transparent multi-tile routing. This is a major architectural change that redesigns how map data is loaded, stored, and processed.

**Key Goals:**
- Support efficient memory-mapped access without deserialization overhead
- Enable geographic tiling (1° × 1° tiles by default)
- Allow routing across tile boundaries transparently
- Support offline mobile use cases (iOS, Android)
- Eliminate the current bincode cache system entirely

## Summary

This research provides a comprehensive analysis of the ridi-router codebase to understand the current architecture and assess the feasibility of implementing the RMDF memory-mapped tile format. The findings reveal a well-structured codebase with clear separation of concerns, making it amenable to the proposed tiled architecture.

**Key Findings:**
1. **Current System**: Monolithic `MapDataGraph` with bincode serialization (4-file cache, 151MB for Latvia)
2. **Zero-Copy Ready**: No existing memory-mapped file usage; current system fully deserializes all data
3. **Spatial Indexing**: Existing PointGrid with 0.01° precision (~1.1km cells) can be adapted per-tile
4. **Reference Architecture**: Index-based references (not pointers) are naturally tile-compatible
5. **Cross-Platform**: Desktop platforms (Linux/macOS/Windows) ready; mobile requires additional work
6. **Performance**: Current system has deserialization overhead; RMDF will eliminate this

The research confirms the RMDF proposal is technically sound and identifies specific implementation pathways.

## Detailed Findings

### 1. Current Map Data Architecture

src/map_data/graph.rs:49-270

**Core Structure: MapDataGraph**

The graph stores all map data in a singleton accessible via `OnceLock`:

```rust
pub static MAP_DATA_GRAPH: OnceLock<MapDataGraph> = OnceLock::new();

pub struct MapDataGraph {
    points: Vec<MapDataPoint>,           // All nodes (45k for Riga tile)
    points_map: HashMap<u64, usize>,     // OSM ID → index (cleared post-init)
    point_grid: PointGrid<MapDataPointRef>, // Spatial index
    ways_lines: HashMap<u64, Vec<MapDataLineRef>>, // Way → lines (cleared post-init)
    lines: Vec<MapDataLine>,              // All edges (89k for Riga tile)
    tags: ElementTags,                    // Deduplicated tag storage
}
```

**Memory Footprint (Latvia dataset):**
- Points: ~40-60 bytes each × 1M points = 40-60 MB
- Lines: ~16 bytes each × 2M lines = 32 MB
- PointGrid: 5-10 MB (HashMap of ~10k cells)
- Tags: 5-10 MB (deduplicated)
- **Total:** 85-175 MB in-memory

**Key Methods:**
- `get_adjacent(point)` (src/map_data/graph.rs:634) - Returns adjacent (line, point) pairs
- `get_closest_to_coords(lat, lon, ...)` (src/map_data/graph.rs:681) - Spatial query with filtering

**Reference Types:**
- `MapDataPointRef` - Wraps `usize` index into points array
- `MapDataLineRef` - Wraps `usize` index into lines array
- `.borrow()` returns `&'static T` via index lookup

**Implications for RMDF:**
✅ Index-based references naturally extend to (tile_id, local_idx)
✅ Reference resolution pattern requires minimal changes
✅ Existing spatial index can be adapted per-tile

### 2. Current Serialization and Caching System

**src/map_data_cache.rs:1-221** and **src/map_data/graph.rs:272-342**

**4-File Cache Structure:**
1. `points.cache` - 91 MB (bincode-serialized Vec<MapDataPoint>)
2. `lines.cache` - 44 MB (bincode-serialized Vec<MapDataLine>)
3. `point_grid.cache` - 16 MB (bincode-serialized PointGrid)
4. `tags.cache` - 760 KB (bincode-serialized ElementTags)
5. `metadata.json` - 112 bytes (SHA256 hash + version)

**pack() Method** (src/map_data/graph.rs:292-342):
- Parallel serialization with rayon (4-way)
- Uses `bincode::serialize()` for each component
- Returns `MapDataGraphPacked` with 4 `Vec<u8>` fields

**unpack() Method** (src/map_data/graph.rs:767-829):
- Parallel deserialization with rayon (4-way)
- Uses `bincode::deserialize()` for each component
- Stores in static `MAP_DATA_GRAPH` OnceLock

**Cache Validation** (src/map_data_cache.rs:109-138):
- SHA256 hash of input file (PBF/JSON)
- Router version check
- **Critical Issue:** Any version bump invalidates all caches
- **Performance Issue:** Hash computed on every startup (even cache hits)

**Why It Needs Replacement:**

| Issue | Impact | Evidence |
|-------|--------|----------|
| **No version safety** | Struct changes break all caches | Version check required (src/map_data_cache.rs:134) |
| **4 files = high I/O** | 8+ system calls minimum | Parallel reads reduce time but not I/O count |
| **No incremental updates** | Full 151MB rewrite on any change | All 4 files written (src/map_data_cache.rs:196-207) |
| **SHA256 on startup** | Hash entire input even on cache hit | Negates startup benefit (src/map_data_cache.rs:110) |
| **Memory-inefficient** | Full deserialization required | 4 parallel bincode deserializations (src/map_data/graph.rs:777-812) |
| **Brittle metadata** | Version bump invalidates caches | `new_metadata.router_version != old_metadata.router_version` |

### 3. OSM Data Loading and Proximity Computation

**src/osm_data/pbf_reader.rs:1-214**

**Three-Phase Loading:**

**Phase 1: Residential Area Extraction** (lines 50-55):
```rust
let mut boundary_reader = PbfAreaReader::new(&mut pbf);
boundary_reader.read(&|obj| {
    (obj.is_way() || obj.is_relation()) && obj.tags().contains("landuse", "residential")
})?;
let residential_area_grid = boundary_reader.get_area_grid();
```

**Phase 2: Military Area Extraction** (lines 56-60):
```rust
let mut boundary_reader = PbfAreaReader::new(&mut pbf);
boundary_reader.read(&|obj| {
    (obj.is_way() || obj.is_relation()) && obj.tags().contains("landuse", "military")
})?;
let military_area_grid = boundary_reader.get_area_grid();
```

**Phase 3: Proximity Computation** (lines 78-150):
```rust
elements.par_iter()  // Parallel with rayon
    .map(|element| {
        if element.is_node() {
            residential_in_proximity: {
                let tot_area = residential_area_grid
                    .find_closest_areas_refs(node.lat(), node.lon(), 1)
                    // Calculate distance to each polygon
                    // Accumulate areas within 500m threshold
                tot_area > THRESHOLD_AREA  // 78,539.8 m²
            }
        }
    })
```

**Constants** (src/osm_data/pbf_reader.rs:16-22):
- `RESIDENTIAL_PROXIMITY_THRESHOLD_METERS = 500.0`
- `RESIDENTIAL_PART_COVERED = 0.10` (10% of circle)
- `THRESHOLD_AREA = π × 500² × 0.10 = 78,539.8 m²`
- `MILITARY_ENTRY_MAX_M = 100.0`

**Algorithm:**
1. Query residential polygons within 1 grid step (~1.1km) of point
2. Calculate Haversine distance to each polygon boundary
3. Accumulate area of polygons within 500m
4. Set `residential_in_proximity = true` if area > 78.5 km²

**Nogo Area Logic** (lines 128-149):
- Point must be INSIDE military polygon (Closest::Intersection)
- Distance from boundary must be > 100m
- Prevents false positives at edges

**RMDF Tile Generation Implications:**
- Tile generation requires 500m buffer around edges
- Overlap processing: Load tile bounds + 500m, compute flags, store core only
- No inter-tile dependencies during proximity computation
- Can parallelize tile generation with rayon

### 4. Spatial Indexing System

**src/map_data/proximity.rs:11-262**

**PointGrid Structure** (lines 166-169):
```rust
pub struct PointGrid<T: Clone> {
    grid: HashMap<GpsCellId, Vec<T>>,  // (i16, i16) → items in cell
}
```

**Grid Parameters:**
- `GRID_CALC_DECIMAL_PLACES = 2`
- `GRID_CALC_PRECISION = 100` (10²)
- Cell size: 0.01° ≈ 1.1 km at equator
- Cell ID range: lat ±9000, lon ±18000

**Cell ID Calculation** (lines 178-182):
```rust
pub fn get_cell_id(lat: f32, lon: f32) -> GpsCellId {
    let lat_rounded = (lat * GRID_CALC_PRECISION as f32).round() as i16;
    let lon_rounded = (lon * GRID_CALC_PRECISION as f32).round() as i16;
    (lat_rounded, lon_rounded)
}
```

**Proximity Search** (lines 246-261):
- Expanding ring search algorithm
- `steps` parameter controls search radius
- Step 0: center cell only
- Step 1: 8 surrounding cells (3×3)
- Step N: concentric square rings

**AreaGrid for Polygons** (lines 62-164):
- Wraps PointGrid<MultiPolygon>
- Rasterizes polygon boundaries into grid cells
- Directional expansion (Up/Down/Left/Right) from vertices
- Used for residential/military area queries

**RMDF Spatial Index Design:**
- Reuse same 0.01° grid precision
- Store GridCellEntry directory per tile
- Binary searchable sorted array (not HashMap)
- Enables fast nearest-point queries within tile

### 5. Routing Algorithm Integration

**src/router/walker.rs:1-1384** and **src/router/navigator.rs:1-570**

**Core Traversal: get_adjacent()** (src/map_data/graph.rs:634-651):
```rust
pub fn get_adjacent(&self, center_point: MapDataPointRef)
    -> Vec<(MapDataLineRef, MapDataPointRef)> {
    center_point.borrow().lines.iter()
        .map(|line| {
            let other_point = if line.borrow().points.0 == center_point {
                line.borrow().points.1.clone()
            } else {
                line.borrow().points.0.clone()
            };
            (line.clone(), other_point)
        })
        .collect()
}
```

**Walker Movement Loop** (src/router/walker.rs:265-330):
1. Move forward until fork or finish
2. Get available segments (apply one-way rules)
3. Detect fork (multiple choices)
4. Select next segment or backtrack
5. Check for loops (visited junction tracking)
6. Add segment to route

**Traffic Rule Application** (src/router/walker.rs:101-159):
```rust
// Filter by turn restrictions
let not_allow_rules = center_point.rules.iter()
    .filter(|rule| rule.rule_type == MapDataRuleType::NotAllowed
                   && rule.from_lines.contains(center_line));

// Block prohibited turns
.filter(|(line_next, _)| {
    !not_allow_rules.iter().any(|rule| rule.to_lines.contains(line_next))
})
```

**Border Crossing Patterns:**
- References use indices, not pointers → naturally tile-aware
- Line endpoints stored explicitly → can detect border crossings
- `get_adjacent()` returns other_point with coordinates
- TileManager can compute tile_id from coordinates
- Load adjacent tile when routing crosses boundary

**Minimal Changes Required:**
```rust
// Old: MapDataGraph::get().get_adjacent(point)
// New: TileManager::get().get_adjacent(point)
```

Same API, different backend implementation.

### 6. Tag Deduplication Mechanism

**src/map_data/graph.rs:51-165**

**Tag Value Storage** (lines 51-72):
```rust
struct ElementTagValueRef {
    pub tag_value_pos: u32,  // 0 = None, 1+ = index+1
}

impl ElementTagValueRef {
    pub fn borrow(&self) -> Option<&smartstring::alias::String> {
        let idx = if self.tag_value_pos == 0 {
            return None;
        } else {
            self.tag_value_pos - 1
        };
        Some(&MapDataGraph::get().tags.tag_values[idx as usize])
    }
}
```

**Tag Set Structure** (lines 89-95):
```rust
pub struct ElementTagSet {
    name: ElementTagValueRef,       // Road name
    hw_ref: ElementTagValueRef,     // Highway reference (A1, M6)
    highway: ElementTagValueRef,    // primary, secondary, etc.
    surface: ElementTagValueRef,    // asphalt, gravel, etc.
    smoothness: ElementTagValueRef, // excellent, good, bad, etc.
}
```

**Deduplication Logic** (lines 134-165):
- `tag_map: HashMap<String, u32>` maps values to indices
- `tag_set_map: HashMap<ElementTagSet, u32>` deduplicates combinations
- Single reference per unique tag combination

**Memory Savings Example:**
- 100k lines with 1000 unique tag combinations
- Without dedup: 100k × 20 bytes = 2 MB
- With dedup: 1000 × 20 bytes + 100k × 4 bytes = 420 KB
- **Savings:** ~80% reduction

**RMDF Format Implications:**
- Each tile has own tag pool (accept duplication across tiles)
- Maintains tile independence
- Tag Values Section (string pool) + Tag Sets Section
- Reference via u32 indices

### 7. No Existing Memory-Mapped File Usage

**Finding:** Zero instances of memory-mapped files in current codebase.

**Searched Patterns:**
- `memmap`, `memmap2`, `mmap` - Not found
- `MmapOptions`, `MmapMut` - Not found
- `unsafe` blocks in map data code - Not found
- Zero-copy deserialization - Not used

**Current Approach:**
- Standard `std::fs::read()` loads entire file
- Bincode deserialization creates full in-memory structures
- No direct memory access to binary files

**RMDF Will Introduce:**
```rust
use memmap2::MmapOptions;
use bytemuck::{Pod, Zeroable, cast_slice};

let file = File::open("tile_204_146.rmdf")?;
let mmap = unsafe { MmapOptions::new().map(&file)? };
let header: &Header = bytemuck::cast_ref(&mmap[0..64]);
let points: &[PointRecord] = bytemuck::cast_slice(&mmap[header.points_offset..]);
```

**Dependencies to Add:**
- `memmap2 = "0.9"` - Memory-mapped file I/O
- `bytemuck = "1.14"` - Safe zero-copy casting

### 8. Spatial Partitioning Patterns

**Existing Grid-Based Partitioning** (src/map_data/proximity.rs:178-182):
- 0.01° cells (~1.1 km)
- Used for in-memory proximity queries
- HashMap-based spatial index

**Existing Cache File Partitioning** (src/map_data_cache.rs:196-207):
- 4 separate files by data type
- points, lines, tags, point_grid
- Parallel I/O with rayon

**No Geographic Tiling:**
- Single monolithic graph for entire region
- Cannot load partial regions
- Cannot combine multiple regions

**RMDF Tiling Strategy:**
- 1.0° × 1.0° default tile size
- Grid-based naming: `tile_{col}_{row}.rmdf`
- Border points duplicated in adjacent tiles
- Transparent cross-tile routing

**Tile ID Calculation:**
```
col = floor(longitude + 180.0)  // 0-359
row = floor(latitude + 90.0)    // 0-179

Riga (56.9°N, 24.1°E) → tile_204_146.rmdf
```

### 9. Parallel Processing Patterns with Rayon

**Current Usage Locations:**

1. **PBF Element Processing** (src/osm_data/pbf_reader.rs:79):
```rust
elements.par_iter()
    .map(|element| {
        // Parallel proximity computation for each node
    })
```

2. **Graph Serialization** (src/map_data/graph.rs:307-323):
```rust
rayon::scope(|scope| {
    scope.spawn(|_| points = bincode::serialize(&self.points));
    scope.spawn(|_| point_grid = bincode::serialize(&self.point_grid));
    scope.spawn(|_| lines = bincode::serialize(&self.lines));
    scope.spawn(|_| tags = bincode::serialize(&self.tags));
});
```

3. **Graph Deserialization** (src/map_data/graph.rs:777-812):
```rust
rayon::scope(|scope| {
    scope.spawn(|_| points = bincode::deserialize(&packed.points));
    // ... 3 more parallel tasks
});
```

4. **Route Generation** (src/router/generator.rs:338-403):
```rust
itineraries.into_par_iter()
    .map(|itinerary| Navigator::new(...).generate_routes())
```

5. **Cache File I/O** (src/map_data_cache.rs:196-207):
```rust
let tasks = [0u8; 4];
tasks.par_iter().enumerate().map(|(i, _)| {
    match i {
        0 => write_cache_file("points", &data.points),
        // ... 3 more files
    }
})
```

**RMDF Parallel Opportunities:**
- Tile generation (one tile per rayon task)
- Proximity computation with overlap (tiles independent)
- Spatial index building per tile
- Multi-tile loading for route requests

### 10. Reference Resolution Patterns

**Two-Level Reference System:**

1. **Reference Types** (src/map_data/graph.rs:205-228):
```rust
pub struct MapDataElementRef<T: MapDataElement> {
    idx: usize,              // Array index
    _marker: PhantomData<T>, // Type marker
}
```

2. **Lazy Dereferencing:**
```rust
pub fn borrow(&self) -> &'static T {
    T::get(self.idx)  // Static lookup via trait
}
```

3. **Trait Implementation:**
```rust
impl MapDataElement for MapDataPoint {
    fn get(idx: usize) -> &'static MapDataPoint {
        &MapDataGraph::get().points[idx]
    }
}
```

**Usage in Routing** (src/router/walker.rs:50-85):
```rust
let center_point_borrowed = center_point.borrow();  // Dereference once
center_point_borrowed.lines.iter()  // Use multiple times
```

**Chained Borrows** (src/router/walker.rs:87-159):
```rust
if line.borrow().is_one_way() && line.borrow().points.1 == center_point {
    return false;
}
```

**RMDF Extension:**
```rust
pub struct PointRef {
    tile_id: TileId,    // Which tile
    osm_id: u64,        // OSM node ID
    lat: f32,           // Cached for tile determination
    lon: f32,
}
```

**Tile-Aware Resolution:**
- Routing stores (tile_id, osm_id) references
- TileManager resolves via tile lookup
- Border crossing: compute new tile_id from coordinates

## Code References

### Map Data Architecture
- `src/map_data/graph.rs:49` - OnceLock singleton declaration
- `src/map_data/graph.rs:263-270` - MapDataGraph structure
- `src/map_data/graph.rs:634-651` - get_adjacent() implementation
- `src/map_data/graph.rs:681-766` - get_closest_to_coords() implementation
- `src/map_data/point.rs:15-24` - MapDataPoint structure
- `src/map_data/line.rs:14-20` - MapDataLine structure
- `src/map_data/rule.rs:14-39` - MapDataRule structure

### Serialization and Caching
- `src/map_data_cache.rs:59-63` - CacheMetadata structure
- `src/map_data_cache.rs:81-106` - SHA256 hash generation
- `src/map_data_cache.rs:109-138` - Cache validation logic
- `src/map_data_cache.rs:140-167` - Parallel cache reading
- `src/map_data_cache.rs:171-212` - Parallel cache writing
- `src/map_data/graph.rs:272-278` - MapDataGraphPacked structure
- `src/map_data/graph.rs:292-342` - pack() method
- `src/map_data/graph.rs:767-829` - unpack() method

### OSM Data Loading
- `src/osm_data/pbf_reader.rs:16-22` - Proximity constants
- `src/osm_data/pbf_reader.rs:50-60` - Area grid extraction
- `src/osm_data/pbf_reader.rs:78-150` - Parallel proximity computation
- `src/osm_data/pbf_area_reader.rs:18-276` - Boundary assembly
- `src/osm_data/data_reader.rs:40` - Orchestration

### Spatial Indexing
- `src/map_data/proximity.rs:14-15` - Grid constants
- `src/map_data/proximity.rs:166-169` - PointGrid structure
- `src/map_data/proximity.rs:178-182` - Cell ID calculation
- `src/map_data/proximity.rs:188-196` - Grid insertion
- `src/map_data/proximity.rs:206-243` - Expanding ring search
- `src/map_data/proximity.rs:246-261` - Closest point search
- `src/map_data/proximity.rs:62-164` - AreaGrid for polygons

### Routing Integration
- `src/router/walker.rs:50-85` - get_segments_for_point()
- `src/router/walker.rs:87-159` - get_fork_segments_for_segment()
- `src/router/walker.rs:265-330` - move_forward_to_next_fork()
- `src/router/navigator.rs:199-335` - generate_routes() loop
- `src/router/weights.rs:314-438` - Tag-based weight calculations

### Tag Management
- `src/map_data/graph.rs:51-72` - ElementTagValueRef
- `src/map_data/graph.rs:89-95` - ElementTagSet structure
- `src/map_data/graph.rs:115-121` - ElementTags with deduplication
- `src/map_data/graph.rs:134-165` - Deduplication logic

### Parallel Processing
- `src/osm_data/pbf_reader.rs:79` - Parallel element processing
- `src/map_data/graph.rs:307-323` - Parallel serialization
- `src/map_data/graph.rs:777-812` - Parallel deserialization
- `src/router/generator.rs:338-403` - Parallel route generation
- `src/map_data_cache.rs:196-207` - Parallel cache file I/O

## Architecture Insights

### Current System Strengths

1. **Clean Separation of Concerns:**
   - Map data layer (src/map_data/)
   - OSM loading layer (src/osm_data/)
   - Routing layer (src/router/)
   - Clear boundaries make refactoring easier

2. **Index-Based References:**
   - Not pointer-based → serializable
   - Natural extension to (tile_id, local_idx)
   - Reference resolution abstracted via .borrow()

3. **Parallel Processing Infrastructure:**
   - Rayon already integrated
   - Proven patterns for scope-based parallelism
   - Easy to extend to tile generation

4. **Spatial Index Foundation:**
   - PointGrid with 0.01° precision works well
   - Expanding ring search algorithm efficient
   - Can be replicated per-tile

### Current System Weaknesses

1. **Bincode Fragility:**
   - No version safety
   - Struct changes break all caches
   - Version bump invalidates all data

2. **Monolithic Architecture:**
   - Single graph for entire region
   - Cannot load partial regions
   - Cannot scale to global datasets

3. **Deserialization Overhead:**
   - Full deserialization required on every load
   - Even with parallelization, CPU-intensive
   - Negates fast startup benefits

4. **Hash Validation Cost:**
   - SHA256 hash on every startup
   - I/O-bound operation
   - Reduces cache hit benefits

### RMDF Design Decisions

1. **Sequential Block Layout:**
   - Header → Spatial Index → Points → Lines → Auxiliary
   - Simpler than section-based format
   - Enables efficient sequential writes

2. **Fixed-Size Records + Auxiliary Arrays:**
   - PointRecord: 48 bytes fixed
   - LineRecord: 40 bytes fixed
   - Variable-length data (tags, rules) in separate arrays
   - Zero-copy access via bytemuck casting

3. **Per-Tile Spatial Index:**
   - GridCellEntry directory sorted by cell_id
   - Binary searchable (O(log n))
   - Replaces HashMap for zero-copy

4. **Border Point Duplication:**
   - Points on boundaries in both tiles
   - Lines crossing borders in both tiles
   - No explicit deduplication needed
   - Routing handles naturally via OSM IDs

5. **Tile Naming:**
   - Grid-based: tile_{col}_{row}.rmdf
   - col = floor(lon + 180), row = floor(lat + 90)
   - Deterministic tile ID from coordinates

6. **SHA256 Checksums:**
   - Per-tile validation
   - Stored in header (last 32 bytes)
   - Validates on load, not on every access

## Platform Analysis

### Desktop Platforms (Linux/macOS/Windows)

**Compatibility:** ✅ Full Support

**Memory-Mapped Files:**
- Linux: POSIX mmap() - full support
- macOS: BSD mmap() - full support
- Windows: MapViewOfFile() - full support via memmap2
- All platforms little-endian (Intel x86_64, ARM64)

**File Descriptor Management:**
- Linux: 1024 default, tunable to 65536+
- macOS: 256 default, tunable to 10240+
- Windows: No hard limit (memory-limited)

**RMDF Readiness:**
- Standard memmap2 + bytemuck usage
- No platform conditionals needed
- All existing code portable

### Mobile Platforms (Android/iOS)

**Compatibility:** ⚠️ Partial Support

**Android Issues:**
- App sandbox: Can only access /data/data/app-name/
- Scoped storage (API 29+): Severe file access restrictions
- FD limits: ~512 per app
- Cannot pre-bundle tiles in APK (size limits)
- Runtime tile download required
- Cache eviction risk on storage pressure

**iOS Issues:**
- App sandbox: Only Documents, Library, tmp accessible
- Bundle resources: Cannot mmap app-bundled files directly
- FD limits: ~128 per app
- Memory pressure: Aggressive app killing
- Background execution: Cannot access files in background
- iCloud sync: Can corrupt file pointers

**Mitigation Strategy:**
- Runtime tile download to app-private storage
- Aggressive LRU eviction (limit to 50-100 tiles)
- Memory-aware loading
- Platform abstraction layer for file I/O

**Recommended Approach:**
- MVP: Desktop-only (Linux/macOS/Windows)
- Phase 2: Mobile support as follow-on feature
- Separate tickets for Android JNI + iOS sandbox handling

### WebAssembly (WASM)

**Compatibility:** ❌ Not Supported

**Blockers:**
- No filesystem in browser context
- memmap2 not available
- File System Access API limited
- Would require ArrayBuffer-based approach

**Alternative:**
- HTTP range requests + ArrayBuffer
- IndexedDB for caching
- Separate tile format or network layer

**Recommendation:**
- Out of scope for initial RMDF
- Future work: WASM-specific tile loading strategy

### Cross-Platform Summary

| Platform | Status | Timeline | FD Limit | Tile Count |
|----------|--------|----------|----------|------------|
| **Linux** | ✅ Ready | Immediate | 1024+ | 1000+ |
| **macOS** | ✅ Ready | Immediate | 256+ | 1000+ |
| **Windows** | ✅ Ready | Immediate | Unlimited | 1000+ |
| **Android** | ⚠️ Maybe | 4-6 weeks | ~512 | ~100 |
| **iOS** | ⚠️ Maybe | 4-6 weeks | ~128 | ~50 |
| **WASM** | ❌ No | 2+ months | N/A | N/A |

## Historical Context (from thoughts/)

### Primary Document

**thoughts/tickets/feature_rmdf_memory_mapped_tiles.md:**
- Comprehensive 1048-line technical specification
- Binary format design with 7 sections
- 3-phase tile generation pipeline
- 5-week implementation plan
- Success criteria with unit/integration tests
- Size estimates: ~520GB for global coverage

### Supporting Documents

**task.md:**
- High-level requirements for tiling
- No backward compatibility needed
- Map drawing out of scope

**README.md:**
- Documents current caching with --cache-dir
- Notes memory expansion: PBF × 8-10x in memory
- Spain example: 1.2GB PBF → 9GB memory

## Related Research

### Dependencies Analysis

**Current Dependencies (Cargo.toml):**
```toml
bincode = "1.3.3"       # TO BE REMOVED
osmpbfreader = "0.16.1" # KEEP for tile generation
rayon = "1.10.0"        # KEEP for parallelization
serde = "1.0.201"       # KEEP for manifest JSON
sha2 = "0.10.8"         # KEEP for checksums
```

**Required Additions:**
```toml
memmap2 = "0.9"         # Memory-mapped file I/O
bytemuck = "1.14"       # Safe zero-copy casting
```

**Platform-Specific (Future):**
```toml
[target.'cfg(target_os = "android")'.dependencies]
jni = "0.21"                    # For Android file access
android_logger = "0.14"         # App logging

[target.'cfg(target_os = "ios")'.dependencies]
objc = "0.2"                    # Objective-C interop
core-foundation = "0.9"         # Apple frameworks
```

### File Size Estimates

**Current System (Latvia):**
- Input: latvia.osm.pbf (~130MB)
- Cache: 157MB total (4 files)
  - points.cache: 95MB
  - lines.cache: 46MB
  - point_grid.cache: 16MB
  - tags.cache: 777KB

**RMDF System (Latvia, 1° tiles):**
- Tiles: 6-8 tiles (56-58°N, 21-28°E)
- Per tile: ~9MB (45k points × 48B + 89k lines × 40B + refs + tags)
- Total: 54-72MB (64% reduction)

**Global Coverage:**
- 360 × 180 = 64,800 tiles maximum
- Assuming Latvia density: ~583GB
- Actual lower (oceans, deserts)
- Per-region subsets practical for mobile

## Open Questions

### Technical Implementation

1. **Tile Size Configurability:**
   - Runtime parameter or compile-time constant?
   - Recommendation: Compile-time for MVP (1.0°), configurable later

2. **Tag Deduplication Across Tiles:**
   - Accept per-tile duplication or global tag dictionary?
   - Recommendation: Per-tile for independence

3. **Line ID Encoding:**
   - (way_id << 16 | segment_index) in u64
   - Assumes segment_index < 65536
   - Add assertion in generator

4. **Checksum Performance:**
   - SHA256 may be slow for large tiles
   - Alternative: xxHash or CRC32
   - Recommendation: SHA256 for MVP, profile later

5. **Platform Abstraction:**
   - When to introduce platform conditionals?
   - Recommendation: Early stub, implement mobile later

### Architecture Decisions

1. **TileManager API:**
   - How closely mirror MapDataGraph API?
   - Recommendation: Identical API for minimal routing changes

2. **Missing Tile Handling:**
   - Error on missing tile or treat as dead-end?
   - Recommendation: Filter out in get_adjacent(), error at route level

3. **Manifest Format:**
   - JSON (current) or binary?
   - Recommendation: JSON for flexibility, human-readable

4. **Build System:**
   - Separate tile generation binary or cargo command?
   - Recommendation: Subcommand (ridi-router generate-tiles)

## Conclusion

The RMDF memory-mapped tile format is technically feasible and architecturally sound. The current codebase is well-structured for this transition, with clear separation of concerns and extensible patterns.

**Key Success Factors:**
1. **Index-based references** naturally extend to tiled architecture
2. **Existing spatial indexing** can be replicated per-tile with minimal changes
3. **Parallel processing infrastructure** ready for tile generation
4. **Desktop platforms** fully supported with memmap2
5. **Mobile support** achievable with platform-specific extensions

**Critical Risks:**
1. **Mobile FD limits** require aggressive LRU eviction
2. **Border crossing logic** needs careful testing
3. **Version evolution** requires schema versioning strategy
4. **Performance validation** needed (not just assumed)

**Recommended Next Steps:**
1. Implement RMDF format and tile generation (desktop-only)
2. Create TileManager with dynamic loading
3. Integrate with routing (minimal API changes)
4. Test with Montenegro (small) and Latvia (production) datasets
5. Validate performance benefits vs. current bincode system
6. Add mobile support as phase 2

This research provides the foundation for confident implementation of FEATURE-001.
