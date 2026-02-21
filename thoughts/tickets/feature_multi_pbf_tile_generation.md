---
type: feature
priority: high
created: 2026-02-21T00:00:00Z
status: researched
tags: [tile-generation, multi-pbf, cli, rmdf]
keywords: [generate-tiles, MultiPbfGenerator, PbfStreamer, TileGenerator, input-dir, redb, overlap]
patterns: [cli argument parsing, parallel processing, proximity grid merge, tile deduplication]
---

# FEATURE: Multi-PBF Tile Generation for `generate-tiles` Command

## Description

Enable the `generate-tiles` CLI command to process all PBF files in an input directory and generate a unified set of RMDF tiles in the output directory. Currently, the command only accepts a single PBF file via `--input`.

## Context

The tile-based routing architecture requires preprocessing OSM PBF files into RMDF tiles. For users with multiple regional PBF files (e.g., `latvia.pbf`, `estonia.pbf`), the current implementation requires running `generate-tiles` separately for each file, which produces separate tile directories that cannot be used together.

This feature enables processing all PBF files in a directory in a single command, producing a unified tile set that covers all input regions with proper handling of overlapping border zones.

## Requirements

### Functional Requirements

1. **CLI Changes**
   - Replace `--input FILE` with `--input-dir DIR` in the `GenerateTiles` command
   - The command should discover and process all `.pbf` files in the input directory
   - If no PBF files are found, exit with an error message

2. **Multi-PBF Processing Pipeline**
   - Load and process each PBF file independently
   - Generate tiles for each PBF using existing `PbfStreamer` logic
   - Handle overlapping regions between PBF files (border zones)

3. **Overlap Handling**
   - Identify geographic overlap zones between PBF files
   - Merge proximity grids in overlap regions (MAX for residential sectors, OR for nogo)
   - Merge tile content when multiple PBFs produce the same TileId (combine nodes/ways/relations)

4. **Manifest Generation**
   - Generate `manifest.json` with:
     - Combined geographic bounds covering all input PBFs
     - List of all source PBF filenames
     - Tile size used

5. **Intermediate Storage**
   - Use redb database in a temp directory for intermediate processing
   - Auto-cleanup temp database after generation completes

### Non-Functional Requirements

- **Error Handling**: Fail fast on any processing error (corrupt PBF, I/O errors)
- **Progress Reporting**: Report both PBF loading progress and tile generation progress
- **Memory**: Memory usage controlled by tile size (existing behavior from single-PBF mode)

## Current State

### What's Implemented

1. **Single-PBF Tile Generation** (`src/rmdf/generator/mod.rs:18-77`)
   - `TileGenerator` struct handles single PBF file
   - Uses `PbfStreamer` for parallel tile processing
   - Generates manifest after tile discovery

2. **Multi-PBF Scaffold** (`src/rmdf/generator/mod.rs:108-306`)
   - `MultiPbfGenerator` struct exists with full pipeline design
   - `discover_pbf_files()` function implemented
   - `process_single_pbf()` partially implemented
   - `identify_overlap_zones()` implemented
   - Grid storage via redb (`GridStorage` in `intermediate.rs`)

3. **PBF Processing** (`src/rmdf/generator/pbf_streamer.rs`)
   - `PbfStreamer` extracts nodes/ways/relations for tile bounds
   - Builds `GenerationGraph` with pre-computed proximity flags
   - Writes RMDF tiles via `RmdfWriter`

### What's Missing / Incomplete

1. **CLI Integration** (`src/router_runner.rs:213-226`)
   - `GenerateTiles` uses `--input FILE` not `--input-dir DIR`
   - No invocation of `MultiPbfGenerator`

2. **Grid Exposure from InMemoryPbf**
   - `InMemoryPbf::from_pbf_file_with_flags()` computes proximity grid but doesn't expose it
   - `process_single_pbf()` at line 231 passes empty grid bytes: `&[] // Placeholder for grid bytes`

3. **Overlap Zone Re-evaluation** (`src/rmdf/generator/mod.rs:280-295`)
   - `reevaluate_overlap_zones()` is stubbed with TODO comment
   - Needs to: load grids, merge with MAX/OR, query nodes, re-evaluate flags, update nodes

4. **Tile Content Merge** (`src/rmdf/generator/mod.rs:297-305`)
   - `write_final_tiles()` just logs "tiles written during phase 1"
   - When multiple PBFs produce same TileId, content needs merging (nodes/ways/relations)

5. **Grid Serialization**
   - `RasterizedProximityGrid` needs serialization for redb storage
   - Deserialization for loading and merging

## Desired State

Running `ridi-router generate-tiles --input-dir ./map-data/input --output-dir ./map-data/output`:

1. Discovers all `.pbf` files in `./map-data/input`
2. For each PBF file:
   - Loads into memory with pre-computed proximity flags
   - Extracts and stores proximity grid to temp redb
   - Generates tiles using `PbfStreamer`
3. Identifies overlap zones between PBF bounds
4. For each overlap zone:
   - Loads affected grids from redb
   - Merges grids (MAX for sectors, OR for nogo)
   - Re-evaluates flags for nodes in zone
   - Updates affected tiles
5. For tiles with same TileId from multiple PBFs:
   - Merges node/way/relation content
   - Writes unified tile
6. Generates manifest with combined bounds and source list
7. Cleans up temp database
8. Reports progress throughout

## Research Context

### Keywords to Search

- `MultiPbfGenerator` - Main multi-PBF orchestration class
- `TileGenerator` - Single-PBF class to be replaced in CLI
- `PbfStreamer` - Parallel tile extraction and writing
- `RasterizedProximityGrid` - Grid that needs serialization
- `GridStorage` - redb storage abstraction
- `GridBounds` - Overlap detection structure
- `reevaluate_overlap_zones` - Key TODO function
- `write_final_tiles` - Tile merge logic needed

### Patterns to Investigate

- **Grid Serialization**: How `RasterizedProximityGrid` stores data internally; how to serialize/deserialize for redb
- **Tile Merge**: How `TileData` struct combines nodes/ways/relations; HashSet for deduplication
- **Progress Reporting**: How `PbfStreamer::partition_parallel()` reports progress; adapt for multi-PBF
- **Error Propagation**: How `anyhow::Context` is used for error wrapping

### Key Decisions Made

| Decision | Rationale |
|----------|-----------|
| Replace `--input` with `--input-dir` | Simpler UX, no ambiguity between single/multi mode |
| Merge proximity grids only (not full node dedup) | Preserves per-PBF node identity; grid merge handles border correctness |
| Temp dir for redb | No leftover artifacts; automatic cleanup |
| Fail fast on errors | Data integrity over partial results |
| Merge tile content on TileId conflict | Required when multiple PBFs cover same geographic area |
| No post-generation validation | Trust generation process; validation adds latency |

## Implementation Plan

### Phase 1: CLI Changes
- Modify `CliMode::GenerateTiles` in `src/router_runner.rs`
- Change `input: PathBuf` to `input_dir: PathBuf`
- Update help text
- Wire to `MultiPbfGenerator` instead of `TileGenerator`

### Phase 2: Grid Exposure
- Add method to `InMemoryPbf` to expose the `RasterizedProximityGrid`
- Add serialization to `RasterizedProximityGrid` (bincode or custom)
- Add deserialization for grid merge

### Phase 3: Grid Storage
- Implement grid serialization in `process_single_pbf()`
- Store actual grid bytes instead of placeholder `&[]`

### Phase 4: Overlap Re-evaluation
- Implement `reevaluate_overlap_zones()`:
  - Load grids from redb for overlap region
  - Merge grids (MAX for sectors, OR for nogo flags)
  - Find tiles affected by overlap zones
  - Re-extract tile data with merged grid
  - Re-write affected tiles

### Phase 5: Tile Merge
- Implement tile content merging in `write_final_tiles()`:
  - Track which tiles were written by which PBFs
  - For duplicate TileIds, merge `TileData` structures
  - Deduplicate nodes/ways by OSM ID
  - Re-write merged tiles

### Phase 6: Manifest Enhancement
- Modify `ManifestGenerator::generate()` to accept multiple source paths
- Compute combined bounds from all PBF bounds
- Include source file list in manifest

### Phase 7: Progress Reporting
- Add outer progress loop for PBF files
- Report: "Processing PBF 1/3: latvia.pbf"
- Preserve existing tile progress within each PBF

## Success Criteria

### Automated Verification
- [ ] `cargo test -p ridi-router` passes
- [ ] `cargo clippy --all-targets` passes
- [ ] Existing single-PBF test continues to work

### Manual Verification
- [ ] Command accepts `--input-dir` instead of `--input`
- [ ] Error when `--input-dir` contains no `.pbf` files
- [ ] Single PBF in input dir produces same output as before
- [ ] Multiple non-overlapping PBFs produce combined tile set
- [ ] Overlapping PBFs (e.g., latvia + estonia) produce merged border tiles
- [ ] `manifest.json` contains combined bounds and source list
- [ ] Temp database is cleaned up after completion
- [ ] Progress reporting shows PBF-level and tile-level progress

## Related Information

- Design doc: `thoughts/plans/rmdf_memory_mapped_tiles/` (phases 2-4 cover multi-PBF)
- Prior ticket: `thoughts/tickets/feature_rmdf_memory_mapped_tiles.md`
- Related code: `src/proximity/rasterized_grid.rs` (grid implementation)
- Related code: `src/rmdf/generator/intermediate.rs` (redb storage)

## Notes

- The `RasterizedProximityGrid` uses SIMD-accelerated lookups; ensure serialization preserves exact byte representation for correctness
- Tile merge for same TileId requires careful handling of way/relation references to avoid orphaned data
- Consider memory: merging large tile content may require streaming approach for planet-scale inputs
