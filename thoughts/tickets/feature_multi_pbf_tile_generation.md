---
type: feature
priority: high
created: 2026-02-21T17:00:00Z
status: created
tags: [rmdf, tiles, pbf, multi-file, proximity, grid, overlap]
keywords: [MultiPbfGenerator, TileGenerator, InMemoryPbf, RasterizedProximityGrid, reevaluate_overlap_zones, discover_pbf_files, input-dir, output-dir]
patterns: [multi-file processing, grid merging, tile deduplication, overlap detection, proximity flag re-evaluation]
research_document: thoughts/research/2026-02-21_multi_pbf_tile_generation.md
---

# FEATURE-001: Multi-PBF Tile Generation Support

## Description

Enable `ridi-router generate-tiles` to process multiple PBF files from an input directory, generating a unified set of RMDF tiles that correctly handle overlapping data between PBF files.

The current implementation only supports single PBF file processing. This feature extends the tile generator to:
1. Discover and load multiple PBF files from an input directory
2. Process each PBF independently with its own proximity grid
3. Detect geographic overlaps between PBF files
4. Re-evaluate nodes in overlap zones using combined grids
5. Deduplicate and merge tiles from multiple sources
6. Output unified RMDF tiles covering all input PBFs

## Context

### Current State (Single PBF)
```
ridi-router generate-tiles --input file.pbf --output ./tiles
```
- Loads single PBF into `InMemoryPbf`
- Computes proximity grid with residential/nogo flags
- Partitions into tiles using `PbfStreamer`
- Writes RMDF tiles directly

### Target State (Multi-PBF)
```
ridi-router generate-tiles --input-dir ./map-data/input --output-dir ./map-data/output
```
- Discovers all `.pbf` files in input directory
- Processes each PBF independently (Phase 1)
- Stores intermediate grids and flagged nodes in redb
- Identifies overlap zones between PBFs (Phase 2)
- Re-evaluates overlap nodes with combined grids (Phase 3)
- Deduplicates and writes final tiles (Phase 4)

### Use Cases
- Processing country extracts that overlap (e.g., Germany + Austria border regions)
- Building regional maps from multiple OSM extracts
- Incremental map updates with changed PBF files
- Planet-scale processing split into manageable chunks

## Requirements

### Functional Requirements

#### FR1: CLI Input Directory Support
- Accept `--input-dir` instead of `--input` file
- Discover all `.pbf` and `.osm.pbf` files in directory
- Sort files for deterministic processing order
- Error if no PBF files found

#### FR2: Individual PBF Processing
- Load each PBF with `InMemoryPbf::from_pbf_file_with_flags()`
- Build proximity grid for each PBF's bounds
- Store grid bounds and metadata in redb
- Partition PBF into intermediate tiles
- Store flagged nodes in redb per-tile

#### FR3: Proximity Grid Storage
- Serialize `RasterizedProximityGrid` to bytes
- Store grid per-PBF in redb (`PROXIMITY_GRIDS` table)
- Include grid bounds and PBF metadata
- Support grid deserialization for re-evaluation

#### FR4: Overlap Detection
- Compare geographic bounds of all processed PBFs
- Identify pairwise overlapping regions
- Calculate intersection bounds with buffer (500m)
- Return list of overlap zones for re-evaluation

#### FR5: Grid Combination and Re-evaluation
- Load grids covering each overlap zone
- Merge grids using MAX for sectors, OR for military
- Query nodes in overlap zones from redb
- Re-compute proximity flags with combined grid
- Update node flags in redb

#### FR6: Tile Deduplication and Merging
- Load intermediate tiles for same (col, row) from multiple PBFs
- Deduplicate nodes/ways/relations by OSM ID
- Merge tile data preserving correct flags
- Write final unified RMDF tiles

#### FR7: Unified Manifest Generation
- Generate single `manifest.json` for all output tiles
- Include all PBF source files in metadata
- Calculate combined bounds from all inputs

### Non-Functional Requirements

#### NFR1: Memory Efficiency
- Don't load all PBFs simultaneously
- Process PBFs sequentially in Phase 1
- Unload PBF data after grid storage
- Keep only overlap zones in memory during Phase 3

#### NFR2: Performance
- Parallel tile processing within each PBF (already implemented)
- Efficient grid serialization/deserialization
- Batch redb operations for node updates

#### NFR3: Correctness
- Nodes in overlap zones must use combined grid for flag calculation
- No duplicate nodes/ways in final tiles
- Correct handling of tiles spanning multiple PBFs

#### NFR4: Error Handling
- Continue processing if one PBF fails (log error)
- Validate grid serialization/deserialization
- Check redb integrity before final write
- Rollback partial output on failure

## Research Context

### Keywords to Search
- `MultiPbfGenerator` - Main generator struct (exists, incomplete)
- `discover_pbf_files` - File discovery function (exists)
- `GridStorage` - redb storage for grids (exists)
- `intermediate::GridBounds` - Bounds representation (exists)
- `RasterizedProximityGrid::to_bytes/from_bytes` - Serialization (exists)
- `InMemoryPbf` - PBF loader with flags (complete)
- `find_overlapping_cells` - Cell-level overlap detection (exists)
- `build_combined_grid` - Grid merging logic (exists)

### Patterns to Investigate
1. **Grid Serialization**: `RasterizedProximityGrid` already has `to_bytes()`/`from_bytes()`
2. **Grid Storage**: `GridStorage::store_grid()` exists but grid parameter is placeholder
3. **Overlap Detection**: `identify_overlap_zones()` exists and calculates intersections
4. **Grid Merging**: `build_combined_grid()` in intermediate.rs merges multiple grids
5. **Node Storage**: `IntermediateTile::save_to_redb()` stores nodes per-tile
6. **Tile Merging**: `IntermediateTile::merge()` and `deduplicate()` exist

### Key Files
- `src/rmdf/generator/mod.rs` - `MultiPbfGenerator` implementation (lines 109-306)
- `src/rmdf/generator/intermediate.rs` - Grid storage, tile merging (lines 1-598)
- `src/proximity/rasterized_grid.rs` - Grid serialization (lines 328-407)
- `src/osm_data/in_memory_pbf.rs` - PBF loading with flags (lines 1-1038)
- `src/rmdf/generator/pbf_streamer.rs` - Single PBF tile partitioning (lines 1-826)

### Key Decisions Made
1. **Use redb for intermediate storage** - Already implemented, suitable for multi-PBF workflow
2. **Grid cell merging strategy** - MAX for residential sectors (avoid double-counting), OR for military
3. **Overlap buffer** - 500m (same as `RESIDENTIAL_PROXIMITY_THRESHOLD_METERS`)
4. **Sequential PBF processing** - Memory efficient, enables incremental processing

## Success Criteria

### Acceptance Criteria

#### AC1: CLI Works with Directory Input
```bash
ridi-router generate-tiles --input-dir ./map-data/input --output-dir ./map-data/output
# Successfully processes all PBF files
```

#### AC2: Multiple PBFs Generate Unified Tiles
- Input: 2+ overlapping PBF files
- Output: Single set of RMDF tiles covering union of all inputs
- No duplicate nodes/ways in any tile

#### AC3: Overlap Zones Correctly Processed
- Nodes in overlap zones have correct proximity flags
- Flags computed using combined grid from overlapping PBFs
- Manual verification: Check specific nodes on PBF borders

#### AC4: Manifest Includes All Sources
```json
{
  "tile_size_degrees": 1.0,
  "source_files": ["germany.pbf", "austria.pbf", "czechia.pbf"],
  "bounds": { "lat_min": 47.0, "lat_max": 55.0, "lon_min": 5.0, "lon_max": 17.0 }
}
```

#### AC5: Performance Acceptable
- Multi-PBF processing time < sum of individual processing times × 1.5
- Memory usage < 2× single PBF processing for largest input

### Automated Verification
- [ ] Unit tests for grid serialization/deserialization
- [ ] Unit tests for overlap detection
- [ ] Unit tests for grid merging
- [ ] Unit tests for tile deduplication
- [ ] Integration test with 2 overlapping mock PBFs
- [ ] Memory