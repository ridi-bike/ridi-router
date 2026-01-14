# RMDF Memory-Mapped Tile Format Implementation Plan

## Overview

Replace the current bincode-based binary serialization format with a custom memory-mapped file format (RMDF - Ridi Map Data Format) that supports tiling, zero-copy access, and transparent multi-tile routing. This is a **major architectural change** and a **breaking release** that redesigns how map data is loaded, stored, and processed.

**Breaking Changes:**
- Removes all existing cache system (4-file bincode cache)
- Removes JSON import support entirely
- Removes `--cache-dir` CLI parameter
- Incompatible with previous versions

**Target Platforms:**
- Linux (x86_64, ARM64)
- macOS (x86_64, ARM64)
- Windows (x86_64)

Mobile platforms (Android/iOS) are explicitly out of scope for this implementation.

## Current State Analysis

### What Exists Now

**Monolithic Architecture** (src/map_data/graph.rs:49-270):
- Single `MapDataGraph` stored in global `OnceLock<MapDataGraph>`
- Index-based references: `MapDataPointRef` wraps `usize` into points Vec
- All data loaded into memory via bincode deserialization
- Latvia dataset: ~151MB cache (4 files), full deserialization required

**4-File Cache System** (src/map_data_cache.rs):
- `points.cache` - 91 MB bincode-serialized Vec<MapDataPoint>
- `lines.cache` - 44 MB bincode-serialized Vec<MapDataLine>
- `point_grid.cache` - 16 MB bincode-serialized PointGrid
- `tags.cache` - 760 KB bincode-serialized ElementTags
- `metadata.json` - SHA256 hash + version
- **Problem:** SHA256 computed on every startup, version bump invalidates all caches

**PBF Loading** (src/osm_data/pbf_reader.rs):
- Three-phase: residential areas → military areas → highway network
- Parallel proximity computation with rayon
- 500m proximity threshold for residential areas
- 100m buffer for military nogo zones

**Spatial Indexing** (src/map_data/proximity.rs):
- PointGrid with 0.01° precision (~1.1km cells)
- HashMap-based cell storage
- Expanding ring search (up to 20 steps)

**Routing Access Patterns** (src/router/walker.rs, src/router/navigator.rs):
- `MapDataGraph::get().get_adjacent(point)` - Line 58 in walker.rs
- `MapDataGraph::get().get_closest_to_coords(...)` - Lines 315, 329 in router_runner.rs
- Reference chaining: `point_ref.borrow().lines.iter().map(|line| line.borrow())`

### Key Discoveries

**Strengths (enable RMDF transition):**
1. **Index-based references** - Not pointers, naturally extend to (tile_id, local_idx)
2. **Clean API boundary** - get_adjacent() and get_closest_to_coords() are only entry points
3. **Parallel infrastructure** - rayon already integrated throughout
4. **Spatial index foundation** - PointGrid works well, can replicate per-tile

**Constraints:**
1. **Static lifetimes** - Current code uses `&'static` references via OnceLock
2. **Global state** - MapDataGraph::get() called throughout routing code
3. **No geographic partitioning** - Single graph per region, cannot scale globally

## Desired End State

### RMDF Tile System

**Binary Format** (new: src/rmdf/format.rs):
```
[Header - 64 bytes]
  - magic: b"RMDF"
  - version: u32
  - tile_bounds: { lat_min, lat_max, lon_min, lon_max }
  - counts: point_count, line_count, grid_cell_count, tag counts
  - section_offsets: [u64; 7]
  - checksum: [u8; 32] (SHA256)

[Spatial Index Directory]
  - Sorted GridCellEntry[] - binary searchable

[Points Section]
  - PointRecord[48 bytes] - OSM ID, lat, lon, line refs, rule refs, flags

[Lines Section]
  - LineRecord[40 bytes] - point OSM IDs with coords, direction, tag index

[Line References Section]
  - u64[] - encoded (way_id << 16 | segment_index)

[Tag Values Section]
  - StringEntry[] + string pool

[Tag Sets Section]
  - TagSetRecord[20 bytes] - indices into tag values

[Rules Section]
  - RuleRecord[40 bytes] - turn restrictions
```

**Tile Organization**:
```
map-data/
├── manifest.json
├── tile_204_146.rmdf  (Riga: 56-57°N, 24-25°E)
├── tile_204_147.rmdf
├── tile_205_146.rmdf
└── ...

Naming: tile_{col}_{row}.rmdf
  col = floor(lon + 180.0)  // 0-359
  row = floor(lat + 90.0)   // 0-179
```

**Manifest Format** (manifest.json):
```json
{
  "version": "1.0.0",
  "tile_size_degrees": 0.1,
  "format_version": 1,
  "generated_at": "2026-01-14T10:30:00Z",
  "tiles": [
    {
      "filename": "tile_204_146.rmdf",
      "col": 204,
      "row": 146,
      "bounds": { "lat_min": 56.0, "lat_max": 57.0, ... },
      "neighbors": {
        "north": "tile_204_147.rmdf",
        "south": "tile_204_145.rmdf",
        ...
      },
      "point_count": 45231,
      "line_count": 89456,
      "checksum": "sha256:abc123..."
    }
  ]
}
```

**TileManager** (new: src/rmdf/tile_manager.rs):
```rust
pub struct TileManager {
    tile_dir: PathBuf,
    manifest: TileManifest,
    loaded_tiles: HashMap<TileId, MappedTile>,
}

impl TileManager {
    pub fn new(tile_dir: PathBuf) -> Result<Self>;
    pub fn get_adjacent(&mut self, point_ref: PointRef) -> Vec<(LineRef, PointRef)>;
    pub fn get_closest_to_coords(&mut self, lat: f32, lon: f32, ...) -> Option<PointRef>;
}

struct PointRef {
    tile_id: TileId,
    osm_id: u64,
    lat: f32,  // Cached for tile determination
    lon: f32,
}
```

### Verification Criteria

**Successful implementation means:**
- Generate Montenegro tiles (--tile-size 0.1) from 10MB PBF
- Route between two coordinates (cross-tile)
- Handle missing tiles gracefully (route around or fail with message)
- No bincode, no JSON, no cache system remains
- Routing performance comparable to current system

## What We're NOT Doing

**Explicitly out of scope:**
1. **Mobile platforms** - No Android/iOS support in this release
2. **Map drawing data** - Separate .rdraw format for visualization (future ticket)
3. **Tile compression** - No zstd/lz4 compression (future optimization)
4. **Incremental updates** - Cannot update single tile, must regenerate all
5. **Backward compatibility** - No migration path from old cache format
6. **Debug tools** - No tile inspector CLI or format validator
7. **Performance benchmarking** - Accept performance as-is, optimize later
8. **Topology validation** - Only basic validation (magic, version, checksum)
9. **WebAssembly** - Browser support not included

## Implementation Approach

### High-Level Strategy

1. **Additive first** - Build RMDF system alongside existing code
2. **Test early** - Unit tests after each phase
3. **Incremental integration** - Replace MapDataGraph only after TileManager ready
4. **Hard cutover** - Remove old code entirely in final phase

### Key Design Decisions

**1. Tile Size Configurability**
- CLI parameter: `--tile-size <DEGREES>` (e.g., 0.1, 1.0)
- Stored in manifest.json
- TileManager reads from manifest (no hardcoded assumptions)

**2. Tag Deduplication Strategy**
- Per-tile tag pools (accept cross-tile duplication)
- Maintains tile independence
- Simpler than global dictionary

**3. Border Point Handling**
- Points on borders duplicated in adjacent tiles
- Lines crossing borders duplicated in both tiles
- No explicit deduplication (routing handles via OSM IDs)

**4. Missing Tile Strategy**
- Filter out lines leading to missing tiles in get_adjacent()
- Report error at routing level: "Could not find route - missing tiles"
- No automatic download or fallback

**5. Memory Management**
- Dynamic tile loading on-demand
- LRU eviction when approaching FD limits
- Desktop platforms: support 1000+ tiles loaded

## Implementation Phases

### Phase 1: RMDF Format & Basic I/O
- Define binary format structs with #[repr(C)]
- Implement memory-mapped file loading
- Write validation logic (magic, version, checksum)
- **Deliverable:** Can load and validate RMDF file
- See: `01_rmdf_format.md`

### Phase 2: CLI Structure & Streaming PBF Partitioner
- Add generate-tiles subcommand
- Implement streaming PBF reader
- Partition by geographic tile
- **Deliverable:** Intermediate tile buffers on disk
- See: `02_streaming_partitioner.md`

### Phase 3: Proximity & Nogo Computation with Overlap
- Implement 500m buffer overlap strategy
- Parallel per-tile computation
- **Deliverable:** Proximity flags computed for all tiles
- See: `03_proximity_computation.md`

### Phase 4: RMDF Writer & Manifest Generation
- Write RMDF binary files
- Generate manifest.json
- **Deliverable:** Valid RMDF tiles + manifest
- See: `04_rmdf_writer.md`

### Phase 5: TileManager - Single Tile Access
- Create TileManager struct
- Implement single-tile queries
- **Deliverable:** TileManager can answer queries within one tile
- See: `05_tile_manager_single.md`

### Phase 6: TileManager - Multi-Tile & Border Crossing
- Implement cross-tile traversal
- Dynamic tile loading
- **Deliverable:** TileManager handles multi-tile routing
- See: `06_tile_manager_multi.md`

### Phase 7: Routing Integration
- Replace MapDataGraph::get() calls
- Update references to include tile_id
- **Deliverable:** Routing works with TileManager
- See: `07_routing_integration.md`

### Phase 8: Cleanup & Consolidation
- Remove old code (cache, JSON, bincode)
- Update dependencies
- **Deliverable:** Clean codebase with only RMDF system
- See: `08_cleanup.md`

### Phase 9: End-to-End Testing
- Montenegro test scenarios
- Missing tile handling
- **Deliverable:** All E2E tests pass
- See: `09_e2e_testing.md`

## Testing Strategy

### Unit Tests (per phase)
- Format parsing and validation
- Tile partitioning logic
- Proximity computation
- RMDF serialization
- TileManager tile loading
- Border crossing detection

### Integration Tests
- Multi-tile spatial queries
- Cross-tile routing traversal
- Missing tile error handling
- Manifest parsing and neighbor lookup

### End-to-End Tests (Phase 9)
- Generate Montenegro tiles (--tile-size 0.1)
- Route between coordinates (success)
- Delete middle tile (route around)
- Delete most tiles (routing fails gracefully)

### Manual Verification
- Inspect generated manifest.json
- Verify tile file sizes reasonable
- Check memory usage during routing
- Validate FD count stays reasonable

## Performance Considerations

**Expected Performance:**
- Tile generation: Montenegro (~10MB PBF) in <30 seconds
- Cold start: <100ms per tile (memory mapping vs deserialization)
- Routing speed: Comparable to current system
- Memory: Only loaded tiles consume RAM (not entire dataset)

**Known Trade-offs:**
- Tile indirection adds overhead (mitigated by zero-copy)
- Cross-tile routing requires additional tile loads
- Per-tile tag duplication increases total storage slightly

**Optimization Opportunities (future):**
- Prefetch adjacent tiles during routing
- Cache-aware data layout within tiles
- Compression for storage (at cost of mmap benefits)

## Migration Notes

**For Users:**
- Must regenerate all data from PBF source files
- No migration path from old cache format
- Breaking change: Major version bump required
- JSON import no longer supported

**For Developers:**
- MapDataGraph still exists as in-memory builder (during tile generation)
- TileManager replaces MapDataGraph for routing queries
- Reference types change from usize to (TileId, OsmId)

## References

- Original ticket: `thoughts/tickets/feature_rmdf_memory_mapped_tiles.md`
- Research document: `thoughts/research/2026-01-14_rmdf_memory_mapped_tiles.md`
- Current graph implementation: `src/map_data/graph.rs:262-270`
- Current cache system: `src/map_data_cache.rs`
- Current PBF reader: `src/osm_data/pbf_reader.rs`
- Routing walker: `src/router/walker.rs:58` (get_adjacent call)
