---
type: feature
priority: high
created: 2026-01-13T00:00:00Z
status: planned
tags: [map-data, memory-mapping, tiling, performance, architecture]
keywords: [rmdf, memory-map, mmap, tiles, TileManager, spatial-index, pbf-streaming, zero-copy, bincode-replacement]
patterns: [tile-generation, border-deduplication, lazy-loading, parallel-processing]
researched_date: 2026-01-14T00:02:14+02:00
research_doc: thoughts/research/2026-01-14_rmdf_memory_mapped_tiles.md
planned_date: 2026-01-14T14:20:00+02:00
plan_doc: thoughts/plans/rmdf_memory_mapped_tiles/00_overview.md
---

# FEATURE-001: RMDF Memory-Mapped Tile Format

## Description

Replace the current bincode-based binary serialization format with a custom memory-mapped file format (RMDF - Ridi Map Data Format) that supports tiling, zero-copy access, and transparent multi-tile routing. This is a major architectural change that redesigns how map data is loaded, stored, and processed.

The new format will:
- Support efficient memory-mapped access without deserialization overhead
- Enable geographic tiling (1° × 1° tiles by default)
- Allow routing across tile boundaries transparently
- Support offline mobile use cases (iOS, Android)
- Eliminate the current bincode cache system entirely

## Context

### Current System Limitations

**Current Architecture** (`src/map_data/graph.rs`, `src/map_data_cache.rs`):
- Single monolithic `MapDataGraph` loaded into memory via bincode deserialization
- Global static `OnceLock<MapDataGraph>` accessed throughout routing code
- Binary cache files (4 separate `.cache` files per region)
- Latvia dataset: ~157MB cache (95MB points, 46MB lines, 16MB point_grid, 777KB tags)
- Requires full deserialization on startup (parallel with rayon, but still overhead)
- Cannot load partial regions or combine multiple regions efficiently
- Memory footprint grows linearly with dataset size

**Current Data Flow:**
```
OSM PBF → PBF Reader → MapDataGraph → pack() → Bincode Cache → unpack() → Global Static
```

### Why This Change Is Needed

1. **Scalability:** Current approach doesn't scale to global datasets (110GB PBF → massive memory usage)
2. **Flexibility:** Cannot mix-and-match regions or load only needed areas
3. **Startup Time:** Deserialization overhead on every launch
4. **Mobile Support:** Offline apps need to download and use only relevant geographic tiles
5. **Future Drawing:** Foundation for map visualization requiring detailed geographic data

### Business Impact

- Enables global routing capability
- Reduces mobile app download sizes (download only needed tiles)
- Faster cold start times (memory mapping vs. deserialization)
- Foundation for future map drawing features

## Requirements

### Functional Requirements

#### 1. RMDF Binary Format Design

**File Structure (Sequential Layout):**
```
[Header - 64 bytes]
  Offset 0x0000:
    - magic: [u8; 4] = b"RMDF"
    - version: u32 (little-endian)
    - tile_bounds: TileBounds {
        lat_min: f32,
        lat_max: f32,
        lon_min: f32,
        lon_max: f32,
      }
    - point_count: u64
    - line_count: u64
    - spatial_grid_cell_count: u32
    - tag_value_count: u32
    - tag_set_count: u32
    - rule_count: u32
    - section_offsets: [u64; 7] (offsets to each section below)
    - checksum: [u8; 32] (SHA256 of entire file excluding this field)

[Spatial Index Directory Section]
  - Sorted array of GridCellEntry structs
  - Binary searchable by cell_id

  GridCellEntry (16 bytes):
    - cell_id: u32 (encoded from grid_x: i16, grid_y: i16)
    - points_offset: u64 (offset into Points Section)
    - points_count: u32
    - _padding: u32

[Points Section]
  - Array of PointRecord structs
  - Ordered by grid cell for cache locality
  - Referenced via offsets from Spatial Index Directory

  PointRecord (48 bytes):
    - osm_id: u64
    - lat: f32
    - lon: f32
    - lines_offset: u64 (offset into Line References Section)
    - lines_count: u32
    - rules_offset: u64 (offset into Rules Section)
    - rules_count: u32
    - flags: u16 (bit 0: residential_in_proximity, bit 1: nogo_area, bits 2-15: reserved)
    - _padding: [u8; 6]

[Lines Section]
  - Array of LineRecord structs
  - Indexed by local line index

  LineRecord (40 bytes):
    - point_a_osm_id: u64
    - point_a_lat: f32
    - point_a_lon: f32
    - point_b_osm_id: u64
    - point_b_lat: f32
    - point_b_lon: f32
    - direction: u8 (0=BothWays, 1=OneWay, 2=Roundabout)
    - tag_set_index: u32
    - _padding: [u8; 3]

  Note: Endpoint coordinates are denormalized (duplicated from PointRecord)
        to enable efficient tile boundary crossing detection without
        requiring point lookups across tiles.

[Line References Section]
  - Flat array of u64 (OSM way IDs + segment indices encoded)
  - Referenced from PointRecord.lines_offset
  - Format: ((way_id << 16) | segment_index)

[Tag Values Section]
  - Array of StringEntry structs followed by string data pool

  StringEntry (10 bytes):
    - offset: u64 (offset into String Data Pool)
    - length: u16

  String Data Pool:
    - Contiguous UTF-8 bytes
    - No null terminators (length-prefixed)

[Tag Sets Section]
  - Array of TagSetRecord structs
  - Deduplicated (identical tag combinations share same index)

  TagSetRecord (20 bytes):
    - name_idx: u32 (index into Tag Values, 0xFFFFFFFF = none)
    - hw_ref_idx: u32
    - highway_idx: u32
    - surface_idx: u32
    - smoothness_idx: u32

[Rules Section]
  - Array of RuleRecord structs
  - Referenced from PointRecord.rules_offset

  RuleRecord (40 bytes):
    - from_lines_offset: u64
    - from_lines_count: u32
    - to_lines_offset: u64
    - to_lines_count: u32
    - rule_type: u8
    - _padding: [u8; 7]

  Rule Line References:
    - Flat array of u64 (line identifiers)
    - Referenced from RuleRecord offsets
```

**Memory Alignment:**
- All structs use `#[repr(C)]` for predictable layout
- All multi-byte integers are little-endian
- No padding between array elements
- 8-byte alignment for performance-critical sections

**Zero-Copy Access Pattern:**
```rust
// Example: accessing a point's lines
let file_bytes: &[u8] = mmap_file("tile_123_45.rmdf");
let header: &Header = cast_ref(&file_bytes[0..64]);
let points: &[PointRecord] = cast_slice(&file_bytes[header.points_offset..]);
let line_refs: &[u64] = cast_slice(&file_bytes[header.line_refs_offset..]);

// Get a point's lines (zero-copy slice)
let point = &points[idx];
let point_line_refs = &line_refs[point.lines_offset..][..point.lines_count];
```

#### 2. Tile Generation from PBF

**Pipeline Architecture:**

**Phase 1: Streaming PBF Partition**
```
Input: planet.osm.pbf (or regional PBF)
Process:
  1. Stream PBF using osmpbfreader (no full load into memory)
  2. Extract residential/military areas into spatial index (in-memory or temp file)
  3. For each node/way:
     - Determine tile(s) it belongs to (based on bounds)
     - Write to intermediate per-tile buffers (disk-based)
  4. Handle border cases:
     - Points on exact borders included in both tiles
     - Points on corners included in all 4 corner tiles
     - Lines crossing borders duplicated in both tiles
```

**Phase 2: Parallel Proximity/Nogo Computation**
```
Process using overlap strategy:
  1. For each tile (parallelized with rayon):
     - Load tile data with 500m buffer around edges
     - Query residential/military areas within (tile_bounds + 500m)
     - Compute proximity flags for points
     - Mark nogo areas
     - Store only core tile area (without buffer)

Note: 500m buffer required for RESIDENTIAL_PROXIMITY_THRESHOLD_METERS
```

**Phase 3: Parallel RMDF Generation**
```
For each tile (parallelized with rayon):
  1. Load intermediate buffers for tile
  2. Apply proximity/nogo results
  3. Build spatial index:
     - Group points by grid cell (0.01° precision = ~1.1km cells)
     - Sort points by cell_id
     - Build GridCellEntry directory
  4. Serialize to RMDF format:
     - Write header
     - Write spatial index directory
     - Write points (ordered by cell)
     - Write lines
     - Write auxiliary arrays (line refs, tag values, rules)
  5. Compute SHA256 checksum
  6. Write final RMDF file
```

**Configuration:**
- Tile size: Configurable (default 1.0 degrees)
- Grid cell precision: 0.01 degrees (GRID_CALC_PRECISION = 100)
- Parallelization: Automatic via rayon (CPU core detection)

**Memory Constraints:**
- Assume 16GB RAM for tile generation
- Intermediate buffers written to disk (SSD preferred)
- Residential/military spatial index kept in memory (~1GB estimated)

#### 3. Tile Naming and Organization

**Filename Convention:**
```
tile_{col}_{row}.rmdf

Where:
  col = floor(longitude + 180.0)  // Range: 0-359
  row = floor(latitude + 90.0)    // Range: 0-179

Examples:
  Riga (56.9°N, 24.1°E) → tile_204_146.rmdf
  New York (40.7°N, -74.0°W) → tile_106_130.rmdf
  Sydney (-33.9°S, 151.2°E) → tile_331_56.rmdf
```

**Directory Structure:**
```
map-data/
├── manifest.json
├── tile_0_0.rmdf
├── tile_0_1.rmdf
├── ...
└── tile_359_179.rmdf

Maximum tiles for global coverage: 360 × 180 = 64,800 tiles
Estimated size: ~520GB for global routing data (assuming Latvia density)
```

**Manifest Format (`manifest.json`):**
```json
{
  "version": "1.0.0",
  "tile_size_degrees": 1.0,
  "generated_at": "2026-01-13T10:30:00Z",
  "source_files": ["planet.osm.pbf"],
  "format_version": 1,
  "tiles": [
    {
      "filename": "tile_204_146.rmdf",
      "col": 204,
      "row": 146,
      "bounds": {
        "lat_min": 56.0,
        "lat_max": 57.0,
        "lon_min": 24.0,
        "lon_max": 25.0
      },
      "neighbors": {
        "north": "tile_204_147.rmdf",
        "south": "tile_204_145.rmdf",
        "east": "tile_205_146.rmdf",
        "west": "tile_203_146.rmdf",
        "northeast": "tile_205_147.rmdf",
        "northwest": "tile_203_147.rmdf",
        "southeast": "tile_205_145.rmdf",
        "southwest": "tile_203_145.rmdf"
      },
      "size_bytes": 8457600,
      "point_count": 45231,
      "line_count": 89456,
      "checksum": "sha256:abc123..."
    }
  ]
}
```

**Neighbor Handling:**
- All 8 neighbors listed (N, S, E, W, NE, NW, SE, SW)
- Missing tiles (ocean, unpopulated areas) listed as null
- Enables preemptive loading of adjacent tiles

#### 4. TileManager Implementation

**Core Abstraction:**

The `TileManager` replaces the global static `MapDataGraph` and provides transparent multi-tile access to the routing algorithm.

**API Design:**
```rust
pub struct TileManager {
    tile_dir: PathBuf,
    manifest: TileManifest,
    loaded_tiles: HashMap<TileId, MappedTile>,
    current_routing_tile: Option<TileId>,  // Tracks active tile during routing traversal
}

impl TileManager {
    /// Initialize from tile directory and manifest
    pub fn new(tile_dir: PathBuf) -> Result<Self>;

    /// Compute tile ID from coordinates
    fn compute_tile_id(lat: f32, lon: f32) -> TileId;

    /// Load tiles dynamically as routing accesses them
    fn ensure_tile_loaded(&mut self, tile_id: TileId) -> Result<()>;

    /// Unload tile to free file descriptor (if needed)
    fn unload_tile(&mut self, tile_id: TileId);

    // API compatible with current MapDataGraph:
    pub fn get_point(&mut self, point_ref: PointRef) -> Option<&PointRecord>;
    pub fn get_line(&mut self, line_ref: LineRef) -> Option<&LineRecord>;
    pub fn get_adjacent(&mut self, point_ref: PointRef) -> Vec<(LineRef, PointRef)>;
    pub fn get_closest_to_coords(&mut self, lat: f32, lon: f32, ...) -> Option<PointRef>;
}

struct MappedTile {
    tile_id: TileId,
    bounds: TileBounds,
    file: memmap2::Mmap,
    header: &'static Header,
    points: &'static [PointRecord],
    lines: &'static [LineRecord],
    spatial_index: &'static [GridCellEntry],
    // ... other sections
}

// References now include tile context
struct PointRef {
    tile_id: TileId,
    osm_id: u64,
    lat: f32,  // Cached for quick tile determination
    lon: f32,
}

struct LineRef {
    tile_id: TileId,
    line_id: u64,
}
```

**Tile Loading Strategy:**

Tiles are loaded based on routing context, NOT via global ID lookup:

1. **Initial Route Start** (coordinate-based):
   ```rust
   fn get_closest_to_coords(&mut self, lat: f32, lon: f32, ...) -> Option<PointRef> {
       // Compute which tile contains these coordinates
       let tile_id = self.compute_tile_id(lat, lon);
       self.ensure_tile_loaded(tile_id)?;

       // Search within tile's spatial index
       let tile = &self.loaded_tiles[&tile_id];
       let point = tile.find_closest_point(lat, lon, ...)?;

       Some(PointRef {
           tile_id,
           osm_id: point.osm_id,
           lat: point.lat,
           lon: point.lon,
       })
   }
   ```

2. **Routing Traversal** (reference-based):
   ```rust
   fn get_adjacent(&mut self, point_ref: PointRef) -> Vec<(LineRef, PointRef)> {
       // We already know which tile the point is in
       let tile = &self.loaded_tiles[&point_ref.tile_id];
       let point = tile.find_point_by_osm_id(point_ref.osm_id)?;

       point.lines.iter().map(|line_id| {
           let line = tile.get_line(line_id);
           let other_point_osm_id = /* get other endpoint */;

           // Check if other point is in current tile
           if let Some(other_point) = tile.find_point_by_osm_id(other_point_osm_id) {
               // Same tile - easy case
               return (LineRef { tile_id: point_ref.tile_id, line_id },
                      PointRef { tile_id: point_ref.tile_id, ... });
           } else {
               // Border crossing - determine adjacent tile from point coordinates
               let next_tile_id = self.compute_tile_id(other_point.lat, other_point.lon);
               self.ensure_tile_loaded(next_tile_id)?;

               let next_tile = &self.loaded_tiles[&next_tile_id];
               let other_point = next_tile.find_point_by_osm_id(other_point_osm_id)?;

               return (LineRef { tile_id: point_ref.tile_id, line_id },
                      PointRef { tile_id: next_tile_id, ... });
           }
       }).collect()
   }
   ```

3. **Border/Corner Handling**:
   - Lines crossing borders store both endpoint OSM IDs
   - Line record also stores endpoint coordinates (or they're looked up)
   - When crossing border, use endpoint coordinates to compute next tile ID
   - For corners: may need to load up to 3 adjacent tiles (load based on line direction)

**Dynamic Tile Loading:**
- Tiles loaded on-demand when routing crosses into new tile
- File descriptor limit awareness (typically 1024-4096 per process)
- LRU eviction strategy if fd limit approached (configurable)
- No global OSM ID → tile mapping required (saves memory and initialization time)

**Border Point Deduplication:**

No explicit deduplication needed! The routing algorithm naturally handles duplicated border points:

```rust
// When a line crosses a tile boundary:
// - Point A is in Tile 1 (and duplicated in Tile 2 if on border)
// - Point B is in Tile 2 (and duplicated in Tile 1 if on border)
// - Line AB exists in both tiles

// During routing from Point A:
1. We're in Tile 1, access Point A
2. get_adjacent() returns Line AB
3. Line AB knows Point B's OSM ID and coordinates
4. Compute tile ID from Point B's coordinates → Tile 2
5. Load Tile 2 if not already loaded
6. Find Point B in Tile 2 by OSM ID (local scan or spatial index)
7. Continue routing from Point B in Tile 2

// No need to track which tiles contain which points globally!
```

**Missing Tile Handling:**
```rust
// When routing tries to cross into a missing tile:
fn get_adjacent(&mut self, point_ref: PointRef) -> Vec<(LineRef, PointRef)> {
    let tile = &self.loaded_tiles[&point_ref.tile_id];
    let point = tile.find_point_by_osm_id(point_ref.osm_id)?;

    point.lines.iter().filter_map(|line_id| {
        let line = tile.get_line(line_id);
        let other_point_osm_id = /* get other endpoint */;

        // Determine which tile contains the other point
        let next_tile_id = self.compute_tile_id(other_point.lat, other_point.lon);

        // Try to load the tile
        match self.ensure_tile_loaded(next_tile_id) {
            Ok(_) => {
                let next_tile = &self.loaded_tiles[&next_tile_id];
                let other_point = next_tile.find_point_by_osm_id(other_point_osm_id)?;
                Some((LineRef { tile_id: point_ref.tile_id, line_id },
                     PointRef { tile_id: next_tile_id, ... }))
            },
            Err(_) => {
                // Tile missing - treat this line as a dead-end
                warn!("Tile {} not available, treating line {} as dead-end",
                      next_tile_id, line_id);
                None  // Filter out this adjacent line
            }
        }
    }).collect()
}

// At routing level, report to user if no valid routes found:
if route.is_none() {
    return Err(RoutingError::NoRouteFound {
        reason: "One or more required tiles are missing. Please download tiles for the route area."
    });
}
```

#### 5. Routing Algorithm Integration

**Minimal Changes Required:**

Current routing code (`src/router/walker.rs`, `src/router/navigator.rs`) accesses map data via:
```rust
MapDataGraph::get().get_adjacent(point_ref)
MapDataGraph::get().get_closest_to_coords(...)
```

New code will use:
```rust
TileManager::get().get_adjacent(point_ref)
TileManager::get().get_closest_to_coords(...)
```

**Key Compatibility Points:**
- Same data structures (PointRecord, LineRecord with same fields)
- Same reference semantics (OSM IDs as primary keys)
- Same adjacency traversal logic
- Same traffic rule application
- Same tag-based filtering

**Expected Changes:**
- Replace `&'static` lifetimes with tile-scoped lifetimes
- Change from `OnceLock` to `TileManager` singleton
- Add tile loading calls in initialization code
- Handle `None` results gracefully (missing tiles)

#### 6. Validation and Error Handling

**Essential Validations (In Scope):**

**At Tile Load Time:**
```rust
fn validate_tile(file: &[u8]) -> Result<(), TileError> {
    // 1. Magic number check
    if &file[0..4] != b"RMDF" {
        return Err(TileError::InvalidMagic);
    }

    // 2. Version check
    let version = read_u32_le(&file[4..8]);
    if version != CURRENT_RMDF_VERSION {
        return Err(TileError::IncompatibleVersion {
            found: version,
            expected: CURRENT_RMDF_VERSION
        });
    }

    // 3. Checksum validation
    let stored_checksum = &file[file.len()-32..];
    let computed_checksum = sha256(&file[..file.len()-32]);
    if stored_checksum != computed_checksum {
        return Err(TileError::ChecksumMismatch);
    }

    Ok(())
}
```

**Error Recovery:**
```rust
// Corrupted tile handling
match tile_manager.load_tile(tile_id) {
    Ok(_) => { /* proceed */ },
    Err(TileError::ChecksumMismatch) => {
        // Treat as missing + inform user to redownload
        warn!("Tile {} corrupted, treating as unavailable", tile_id);
        return Err(RoutingError::CorruptedTile { tile_id });
    }
}
```

**Not In Scope (Future):**
- Topology validation (border node matching between tiles)
- Reference validation (all IDs point to valid entities)
- Bounds validation (tile contents match declared bounds)

### Non-Functional Requirements

#### Performance

- **Cold start:** Memory mapping faster than bincode deserialization (target: <100ms per tile)
- **Memory usage:** Only loaded tiles consume RAM (not entire dataset)
- **Routing speed:** Comparable to current implementation (zero-copy access compensates for tile indirection)
- **Tile generation:** Parallelized with rayon (target: process global PBF in <24 hours on 16-core machine)

#### Scalability

- Support up to 64,800 tiles (global coverage at 1° resolution)
- Support routing across 1000+ tiles in single request
- File descriptor management (dynamic load/unload)
- Configurable tile size for memory/granularity trade-offs

#### Compatibility

- Little-endian byte order (iOS ARM64, Android ARM64, Linux x86_64/ARM64, macOS ARM64/x86_64)
- Memory-mapped file support (all target platforms support mmap)
- No backward compatibility with old bincode cache format

#### Maintainability

- Clean removal of old code paths (JSON, bincode, cache)
- Clear separation: tile generation vs. tile consumption
- Documented binary format specification
- Version field for future format evolution

## Current State

**Files and Components:**

**Map Data Core:**
- `src/map_data/graph.rs` (1633 lines): MapDataGraph, references, tags, serialization
- `src/map_data/point.rs`: MapDataPoint struct (id, lat, lon, lines, rules, flags)
- `src/map_data/line.rs`: MapDataLine struct (points, direction, tags)
- `src/map_data/rule.rs`: MapDataRule struct (traffic restrictions)
- `src/map_data/proximity.rs`: PointGrid spatial index

**Data Loading:**
- `src/osm_data/data_reader.rs`: Orchestrates PBF/JSON reading
- `src/osm_data/pbf_reader.rs`: PBF parsing with osmpbfreader, proximity/nogo computation
- `src/osm_data/json_reader.rs`: JSON parsing (to be removed)
- `src/osm_data/json_parser.rs`: JSON state machine (to be removed)

**Caching:**
- `src/map_data_cache.rs`: Bincode cache management (to be removed)

**Routing:**
- `src/router/walker.rs`: Graph traversal, uses `MapDataGraph::get().get_adjacent()`
- `src/router/navigator.rs`: Route generation, accesses points/lines via references

**Dependencies (Cargo.toml):**
- `bincode = "1.3.3"` (to be removed)
- `osmpbfreader = "0.16.1"` (keep for tile generation)
- `rayon = "1.10.0"` (keep for parallelization)
- `serde` (keep for manifest JSON)

**Key Patterns:**
- Global static graph: `pub static MAP_DATA_GRAPH: OnceLock<MapDataGraph>`
- Index-based references resolved via array access
- Tag deduplication with shared storage
- Parallel I/O with rayon for bincode pack/unpack
- Two-phase loading: PBF → Graph → Pack → Cache → Unpack → Static

## Desired State

**New Architecture:**

```
OSM PBF → Tile Generator → RMDF Tiles + Manifest
                              ↓
                         TileManager (mmap) → Routing Algorithm
```

**New Files and Components:**

**Tile Format:**
- `src/rmdf/format.rs`: Binary format specification, header/record structs
- `src/rmdf/validation.rs`: Magic, version, checksum validation
- `src/rmdf/io.rs`: Memory-mapped file I/O, zero-copy casting

**Tile Generation:**
- `src/rmdf/generator/mod.rs`: Orchestration of tile generation pipeline
- `src/rmdf/generator/pbf_streamer.rs`: Streaming PBF parser, tile partitioning
- `src/rmdf/generator/proximity.rs`: Proximity/nogo computation with overlap
- `src/rmdf/generator/spatial_index.rs`: Grid-based spatial index builder
- `src/rmdf/generator/writer.rs`: RMDF file writer, serialization
- `src/rmdf/generator/manifest.rs`: Manifest generation

**Tile Management:**
- `src/rmdf/tile_manager.rs`: TileManager implementation, dynamic loading
- `src/rmdf/tile.rs`: MappedTile wrapper, section accessors

**Routing Integration:**
- Modify `src/router/walker.rs`: Replace `MapDataGraph::get()` with `TileManager::get()`
- Modify `src/router/navigator.rs`: Same replacement
- Modify `src/router_runner.rs`: Initialize TileManager instead of MapDataGraph

**Removed Files:**
- `src/osm_data/json_reader.rs`
- `src/osm_data/json_parser.rs`
- `src/map_data_cache.rs`
- Remove `MapDataGraph::pack()` and `::unpack()` methods from `graph.rs`

**Updated Dependencies:**
```toml
# Remove:
# bincode = "1.3.3"

# Add:
memmap2 = "0.9"      # Memory-mapped file I/O
bytemuck = "1.14"    # Safe zero-copy casting
```

**CLI Changes:**
```bash
# Old:
ridi-router --data-source pbf --file map.osm.pbf --cache-dir ./cache

# New (tile generation):
ridi-router generate-tiles --input planet.osm.pbf --output ./tiles --tile-size 1.0

# New (routing):
ridi-router route --tiles ./tiles --from "56.9,24.1" --to "57.1,24.3"
```

## Research Context

### Keywords to Search

**Core Concepts:**
- `MapDataGraph` - Current graph structure to be replaced/refactored
- `bincode` - Serialization to be removed
- `pack` / `unpack` - Methods to be removed
- `MapDataPoint` - Core data structure to maintain compatibility
- `MapDataLine` - Line structure to maintain
- `PointGrid` - Spatial index to be redesigned per-tile
- `get_adjacent` - Key routing API to preserve
- `get_closest_to_coords` - Spatial query API to preserve
- `residential_in_proximity` - 500m proximity computation
- `nogo_area` - Military/forbidden area marking
- `OnceLock` - Global static pattern to replace

**Technical Patterns:**
- `mmap` / `memory-map` - Core technique for new format
- `zero-copy` - Deserialization-free access pattern
- `osmpbfreader` - PBF parsing library (keep for generation)
- `rayon` - Parallelization (keep and expand usage)
- `sha2` - Checksum validation (already in dependencies)

**File Patterns:**
- `src/map_data/*.rs` - Core data structures
- `src/osm_data/*.rs` - Data loading to be refactored
- `src/router/*.rs` - Routing algorithm to be minimally modified
- `src/map_data_cache.rs` - Cache system to be removed

### Patterns to Investigate

**Current Access Patterns:**
- How routing code traverses graph via `get_adjacent()`
- How spatial queries use PointGrid
- How tags are deduplicated and accessed
- How traffic rules are stored and applied
- How reference resolution works (index-based lookup)

**Memory Management:**
- Current use of `&'static` lifetimes
- Global static initialization pattern
- Memory footprint of current system

**Serialization:**
- Bincode serialization/deserialization details
- Cache file structure and naming
- SHA256 checksum computation

**PBF Processing:**
- Streaming vs. full-load strategies
- Residential area extraction (PbfAreaReader)
- Proximity computation algorithm (500m threshold)
- Parallel processing with rayon

### Key Decisions Made

**Format Design:**
- Sequential block layout (simpler than section-based)
- Zero-copy via fixed-size records + auxiliary arrays
- Little-endian byte order
- String pool for variable-length tags
- Grid-based spatial index (0.01° cells)

**Tiling Strategy:**
- 1.0° × 1.0° default tile size (configurable)
- Grid-based naming: `tile_{col}_{row}.rmdf`
- Border elements duplicated across tiles
- Corner points in all 4 corner tiles
- TileManager deduplicates transparently

**Generation Pipeline:**
- Streaming PBF (no full load into memory)
- Overlap processing for proximity/nogo (500m buffer)
- Parallel tile generation with rayon
- Disk-based intermediate buffers

**Tile Management:**
- Dynamic loading on-demand (fd limit awareness)
- OSM IDs as primary keys (maintain existing semantics)
- Routing-aware tile selection for border points
- Missing tiles treated as dead-ends + error reported

**Scope Boundaries:**
- Drawing data format: OUT OF SCOPE (future ticket)
- Incremental tile updates: OUT OF SCOPE
- Compression: OUT OF SCOPE (future optimization)
- Debug tools: OUT OF SCOPE
- Performance benchmarks: OUT OF SCOPE (POC first)
- Topology validation: OUT OF SCOPE (only essential validation)

## Success Criteria

### Automated Verification

- [ ] Unit tests for RMDF format parsing
  - [ ] Header deserialization
  - [ ] PointRecord zero-copy access
  - [ ] LineRecord access
  - [ ] Spatial index binary search
  - [ ] String pool access
  - [ ] Tag set lookup
  - [ ] Checksum validation

- [ ] Unit tests for TileManager
  - [ ] Dynamic tile loading
  - [ ] Point lookup across tiles
  - [ ] Border point deduplication
  - [ ] Missing tile error handling
  - [ ] Spatial query delegation to correct tile

- [ ] Integration test: Montenegro
  - [ ] Download Montenegro PBF (~10MB)
  - [ ] Generate RMDF tiles (should create 2-4 tiles)
  - [ ] Load tiles into TileManager
  - [ ] Generate route between two coordinates (within single tile)
  - [ ] Generate route crossing tile boundary
  - [ ] Verify route matches expected path

- [ ] Integration test: Missing tile handling
  - [ ] Generate Montenegro tiles
  - [ ] Delete one middle tile
  - [ ] Route around missing tile (verify detour)
  - [ ] Delete most tiles, keep only start/end
  - [ ] Verify routing fails with appropriate error message

- [ ] No bincode/JSON code remains
  - [ ] `bincode` removed from Cargo.toml
  - [ ] `src/map_data_cache.rs` deleted
  - [ ] `src/osm_data/json_reader.rs` deleted
  - [ ] `src/osm_data/json_parser.rs` deleted
  - [ ] No references to `pack()` or `unpack()` in routing code

### Manual Verification

- [ ] Generate tiles for larger region (Latvia or similar)
  - [ ] Tile generation completes without errors
  - [ ] All expected tiles created (based on bounds)
  - [ ] Manifest.json contains all tiles with neighbors
  - [ ] Spot-check 3-5 tiles for valid RMDF format

- [ ] Route with Latvia tiles
  - [ ] Load tiles into TileManager
  - [ ] Generate multiple routes
  - [ ] Verify routes are sensible (use debug viewer if available)
  - [ ] Check memory usage (should be lower than old system)

- [ ] Border crossing verification
  - [ ] Manually identify a route crossing tile boundary
  - [ ] Verify routing works seamlessly
  - [ ] Check logs for tile loading behavior
  - [ ] Verify point deduplication (border point accessed correctly)

- [ ] File descriptor check
  - [ ] Load many tiles (20+)
  - [ ] Verify no fd exhaustion
  - [ ] Check dynamic load/unload behavior

## Related Information

### Technical References

**Memory Mapping:**
- `memmap2` crate: https://docs.rs/memmap2/
- `bytemuck` for safe casting: https://docs.rs/bytemuck/

**PBF Streaming:**
- `osmpbfreader` crate: https://docs.rs/osmpbfreader/
- OSM PBF format: https://wiki.openstreetmap.org/wiki/PBF_Format

**Current Implementation:**
- MapDataGraph: `src/map_data/graph.rs:262-270`
- Bincode serialization: `src/map_data/graph.rs:292-342`
- PointGrid spatial index: `src/map_data/proximity.rs:166-262`
- Proximity threshold: `src/osm_data/pbf_reader.rs:16` (500 meters)

### Size Estimates

**Current System (Latvia):**
- Input: `latvia.osm.pbf` (~130MB)
- Cache: 157MB total (4 files)
  - points.cache: 95MB
  - lines.cache: 46MB
  - point_grid.cache: 16MB
  - tags.cache: 777KB

**RMDF System (Latvia, 1° tiles):**
- Input: Same PBF
- Tiles: ~6-8 tiles (Latvia spans ~56-58°N, 21-28°E)
- Per tile breakdown (estimated):
  - Points: 45k × 48 bytes = 2.16 MB
  - Lines: 89k × 40 bytes = 3.56 MB (includes denormalized coordinates)
  - Line Refs: ~270k × 8 bytes = 2.16 MB
  - Tags: ~0.8 MB
  - Spatial Index: ~2k cells × 16 bytes = 32 KB
  - **Total per tile: ~9 MB (routing only)**
- Total for Latvia: ~6-8 tiles = ~54-72 MB

**Global Coverage Estimate:**
- Tiles: 360 × 180 = 64,800 tiles
- Average: ~9 MB per tile (assuming Latvia density)
- Total: ~583 GB (actual will be lower due to oceans/deserts)

### Future Work

**Follow-on Tickets (Out of Scope):**

1. **Drawing Data Format (.rdraw)**
   - Separate binary format for map visualization
   - Buildings, natural features, POIs, footpaths
   - Multiple detail levels (minimal, standard, detailed)
   - Companion to RMDF routing tiles

2. **Tile Compression**
   - Investigate zstd/lz4 compression
   - Trade-off: disk space vs. mmap requirement
   - May need decompression-on-load

3. **Incremental Tile Updates**
   - Regenerate only changed tiles
   - Tile versioning and diff mechanism

4. **Debug and Diagnostic Tools**
   - RMDF tile inspector CLI
   - Format validator
   - Topology checker (border node matching)
   - Performance profiler

5. **Performance Optimization**
   - Benchmark RMDF vs. bincode
   - Optimize tile loading strategy
   - Cache-aware data layout
   - Prefetching adjacent tiles

6. **Tile Distribution Infrastructure**
   - CDN hosting
   - Version management
   - Delta updates
   - Client-side download manager

## Notes

### Implementation Strategy

**Recommended Phases:**

**Phase 1: Format and Basic I/O (Week 1)**
- Define RMDF structs in `src/rmdf/format.rs`
- Implement memory-mapped file loading
- Write validation code (magic, version, checksum)
- Unit tests for format parsing

**Phase 2: Tile Generation (Week 2)**
- Streaming PBF reader with tile partitioning
- Proximity/nogo computation with overlap
- RMDF file writer
- Spatial index generation
- Test with Montenegro PBF

**Phase 3: TileManager (Week 3)**
- TileManager implementation
- Dynamic tile loading
- Point/line lookup with deduplication
- Border crossing logic
- Integration tests

**Phase 4: Routing Integration (Week 4)**
- Replace MapDataGraph with TileManager in routing code
- Handle missing tiles gracefully
- Update CLI for tile-based workflows
- Remove old code (bincode, JSON, cache)
- Full Latvia test

**Phase 5: Manifest and Cleanup (Week 5)**
- Manifest generation
- Neighbor calculation
- Documentation
- Final testing and validation

### Open Questions for Implementation

1. **Tile Size Configurability:**
   - Should tile size be a runtime parameter or compile-time constant?
   - Current plan: Compile-time constant (1.0 degrees), make configurable later if needed

2. **Tag Deduplication Across Tiles:**
   - Each tile has its own tag pool, leading to duplication
   - Alternative: Global tag dictionary (but defeats tiling purpose)
   - Decision: Accept per-tile duplication for independence

3. **Line ID Encoding:**
   - Current proposal: `(way_id << 16 | segment_index)` in u64
   - Assumes segment_index < 65536 (reasonable for most ways)
   - Add assertion in generator to catch violations

4. **Grid Cell ID Encoding:**
   - Current proposal: Pack `(x: i16, y: i16)` into u32
   - Alternative: Use compound key struct
   - Decision: u32 encoding for simpler binary search

5. **Checksum Performance:**
   - SHA256 of entire tile may be slow for large tiles
   - Alternative: xxHash or CRC32 (faster but less secure)
   - Decision: SHA256 for now, optimize later if needed

### Risk Mitigation

**Risk: PBF Streaming Memory Overflow**
- Mitigation: Monitor memory usage, implement disk-based buffers early
- Test with large PBF files (100GB planet file)

**Risk: File Descriptor Exhaustion**
- Mitigation: Implement LRU eviction from start
- Test with 100+ tile loading scenario

**Risk: Border Point Logic Bugs**
- Mitigation: Extensive testing of corner cases (literally)
- Manual verification with debug viewer

**Risk: Performance Regression**
- Mitigation: Accept for POC, optimize in follow-on tickets
- Zero-copy should compensate for indirection

**Risk: Incompatible Format Changes**
- Mitigation: Version field in header
- Clear documentation of format spec
- Ability to regenerate all tiles if needed
