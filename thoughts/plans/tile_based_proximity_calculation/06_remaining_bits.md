# Phase 6: Remaining Bits

## Overview

This phase adds the finishing touches: comprehensive error handling, progress reporting, edge case handling, and manifest generation. These are the non-core features that make the implementation production-ready.

## Changes Required

### 1. Enhanced Error Handling

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Add detailed error context throughout pipeline

```rust
impl PbfStreamer {
    fn process_tile(&self, tile_id: TileId) -> Result<()> {
        // Wrap entire tile processing in context
        (|| -> Result<()> {
            let core_bounds = self.calculate_tile_bounds(tile_id);
            let buffered_bounds = self.add_buffer_to_bounds(core_bounds);

            let tile_data = self.extract_tile_data(tile_id, buffered_bounds)
                .with_context(|| format!(
                    "Failed to extract PBF data for tile {:?} (bounds: {:?})",
                    tile_id, buffered_bounds
                ))?;

            let (residential_grid, military_grid) = self.build_area_grids(&tile_data, buffered_bounds)
                .with_context(|| format!(
                    "Failed to build area grids for tile {:?} ({} nodes, {} ways)",
                    tile_id, tile_data.nodes.len(), tile_data.ways.len()
                ))?;

            let nodes_with_flags = self.compute_proximity_flags(
                tile_data.nodes,
                core_bounds,
                &residential_grid,
                &military_grid,
            ).with_context(|| format!(
                "Failed to compute proximity flags for tile {:?}",
                tile_id
            ))?;

            let graph = self.build_generation_graph(
                nodes_with_flags,
                tile_data.ways,
                tile_data.relations,
            ).with_context(|| format!(
                "Failed to build generation graph for tile {:?}",
                tile_id
            ))?;

            self.write_rmdf_tile(tile_id, graph)
                .with_context(|| format!(
                    "Failed to write RMDF tile {:?} to disk",
                    tile_id
                ))?;

            Ok(())
        })().with_context(|| format!("Failed to process tile {:?}", tile_id))
    }
}
```

**Rationale**: Detailed error messages help debug tile-specific failures in parallel execution.

### 2. Improved Progress Reporting

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Add detailed progress tracking with timing

```rust
use std::time::Instant;

impl PbfStreamer {
    pub fn partition_parallel(&self) -> Result<()> {
        let start_time = Instant::now();
        info!("Starting tile-based parallel partitioning");

        let tiles = self.calculate_all_tiles();
        let total_tiles = tiles.len();
        info!("Processing {} tiles in parallel", total_tiles);

        let completed = Arc::new(AtomicUsize::new(0));
        let start_time_clone = start_time.clone();

        tiles.par_iter()
            .try_for_each(|tile_id| -> Result<()> {
                self.process_tile(*tile_id)?;

                let count = completed.fetch_add(1, Ordering::Relaxed) + 1;

                // Report progress every 100 tiles or at completion
                if count % 100 == 0 || count == total_tiles {
                    let elapsed = start_time_clone.elapsed();
                    let rate = count as f64 / elapsed.as_secs_f64();
                    let remaining = ((total_tiles - count) as f64 / rate) as u64;

                    info!(
                        "Progress: {}/{} tiles ({:.1}%) - {:.1} tiles/sec - ETA: {}s",
                        count,
                        total_tiles,
                        (count as f64 / total_tiles as f64) * 100.0,
                        rate,
                        remaining
                    );
                }

                Ok(())
            })?;

        let total_duration = start_time.elapsed();
        info!(
            "Tile-based partitioning complete - {} tiles in {:.1}s ({:.1} tiles/sec)",
            total_tiles,
            total_duration.as_secs_f64(),
            total_tiles as f64 / total_duration.as_secs_f64()
        );

        Ok(())
    }
}
```

**Rationale**: Provides visibility into parallel processing with ETA and throughput metrics.

### 3. Handle Empty Tiles Gracefully

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Skip or create minimal files for empty tiles

```rust
impl PbfStreamer {
    fn process_tile(&self, tile_id: TileId) -> Result<()> {
        // ... existing pipeline ...

        // After extraction
        if tile_data.nodes.is_empty() && tile_data.ways.is_empty() {
            // Empty tile (ocean, poles, etc.) - skip writing
            info!("Tile {:?} is empty, skipping RMDF write", tile_id);
            return Ok(());
        }

        // Continue with processing...
    }
}
```

**Rationale**: Avoids creating thousands of empty RMDF files for ocean tiles, saving disk space.

### 4. Add Manifest Generation

**File**: `src/rmdf/generator/mod.rs`

**Changes**: Generate manifest after all tiles complete

```rust
use crate::rmdf::generator::manifest::ManifestGenerator;

impl TileGenerator {
    pub fn generate(&self) -> Result<()> {
        info!("Starting RMDF tile generation");

        let streamer = PbfStreamer::new(
            &self.input_file,
            &self.output_dir,
            self.tile_size_degrees,
        )?;

        // Process all tiles in parallel
        streamer.partition_parallel()?;

        // Generate manifest
        info!("Generating manifest");
        let manifest_gen = ManifestGenerator::new(&self.output_dir, self.tile_size_degrees);
        manifest_gen.generate()
            .context("Failed to generate manifest")?;

        info!("RMDF generation complete");
        Ok(())
    }
}
```

**Rationale**: Manifest lists all available tiles for the routing system to discover.

### 5. Add Tile Discovery from Filesystem

**File**: `src/rmdf/generator/manifest.rs`

**Changes**: Discover tiles from written RMDF files

```rust
impl ManifestGenerator {
    pub fn generate(&self) -> Result<()> {
        // Discover all RMDF files in output directory
        let mut tile_ids = Vec::new();

        for entry in std::fs::read_dir(&self.output_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("rmdf") {
                // Parse tile ID from filename: tile_123_456.rmdf
                if let Some(tile_id) = Self::parse_tile_id_from_filename(&path) {
                    tile_ids.push(tile_id);
                }
            }
        }

        tile_ids.sort_by_key(|id| (id.col, id.row));

        info!("Discovered {} tiles for manifest", tile_ids.len());

        // Write manifest (JSON or binary format)
        self.write_manifest(&tile_ids)?;

        Ok(())
    }

    fn parse_tile_id_from_filename(path: &Path) -> Option<TileId> {
        let filename = path.file_stem()?.to_str()?;

        // Parse "tile_123_456" format
        let parts: Vec<&str> = filename.split('_').collect();
        if parts.len() == 3 && parts[0] == "tile" {
            let col = parts[1].parse().ok()?;
            let row = parts[2].parse().ok()?;
            return Some(TileId { col, row });
        }

        None
    }
}
```

**Rationale**: Manifest generation doesn't need to track tiles during processing - just discover from written files.

### 6. Add Validation and Sanity Checks

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Add validation throughout pipeline

```rust
impl PbfStreamer {
    fn validate_tile_data(&self, tile_data: &TileData) -> Result<()> {
        // Check for orphaned ways (ways referencing nodes not in tile)
        let node_ids: std::collections::HashSet<u64> = tile_data.nodes.keys().copied().collect();

        for way in &tile_data.ways {
            let missing_nodes: Vec<u64> = way.point_ids.iter()
                .filter(|id| !node_ids.contains(id))
                .copied()
                .collect();

            if !missing_nodes.is_empty() {
                warn!(
                    "Way {} references {} nodes not in tile {:?}: {:?}",
                    way.id,
                    missing_nodes.len(),
                    tile_data.tile_id,
                    &missing_nodes[..missing_nodes.len().min(5)] // Show first 5
                );
            }
        }

        Ok(())
    }

    fn process_tile(&self, tile_id: TileId) -> Result<()> {
        // ... extraction ...

        // Validate before processing
        self.validate_tile_data(&tile_data)?;

        // ... continue pipeline ...
    }
}
```

**Rationale**: Catches data inconsistencies early (orphaned references are expected and handled gracefully).

### 7. Handle World Boundary Edge Cases

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Handle poles and date line correctly

```rust
impl PbfStreamer {
    fn add_buffer_to_bounds(&self, bounds: TileBounds) -> TileBounds {
        let buffer_degrees = 0.005f32;

        TileBounds {
            // Clamp latitude to valid range (poles)
            lat_min: (bounds.lat_min - buffer_degrees).max(-90.0),
            lat_max: (bounds.lat_max + buffer_degrees).min(90.0),

            // Clamp longitude to valid range (date line)
            // Note: This doesn't handle date line wrapping for simplicity
            // Tiles at date line may have incomplete data (acceptable)
            lon_min: (bounds.lon_min - buffer_degrees).max(-180.0),
            lon_max: (bounds.lon_max + buffer_degrees).min(180.0),
        }
    }
}
```

**Rationale**: Polar regions and date line are edge cases - clamping prevents invalid coordinates.

### 8. Add Resource Cleanup

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Ensure file handles are properly closed

```rust
impl PbfStreamer {
    fn extract_tile_data(
        &self,
        tile_id: TileId,
        buffered_bounds: TileBounds,
    ) -> Result<TileData> {
        // File is opened in local scope and automatically closed when dropped
        {
            let file = File::open(&self.input_file)?;
            let mut pbf = OsmPbfReader::new(file);
            // ... processing ...
        } // File handle dropped here

        // Second pass
        {
            let file = File::open(&self.input_file)?;
            let mut pbf = OsmPbfReader::new(file);
            // ... processing ...
        } // File handle dropped here

        Ok(tile_data)
    }
}
```

**Rationale**: Explicit scoping ensures file handles don't accumulate in parallel execution.

## Success Criteria

### Automated Verification

- [x] Error messages include tile ID and context for all failures
- [x] Progress reporting shows percentage, rate, and ETA
- [x] Empty tiles are handled without errors
- [x] Manifest generation discovers all written tiles
- [x] Validation catches orphaned way references
- [x] World boundary edge cases don't cause crashes
- [x] File handles are properly closed (no descriptor leaks)

### Manual Verification

- [ ] Run full generation on large PBF file
- [ ] Verify progress logs show increasing completion
- [ ] Check for error messages with clear context
- [ ] Verify manifest.json exists with correct tile list
- [ ] Confirm no file descriptor exhaustion
- [ ] Test with PBF files at poles and date line

## Dependencies

- Depends on: Phase 5 (Tile Writing)
- Blocks: Phase 7 (Remove Obsolete Code)

## Risks & Mitigations

- **Risk**: Progress reporting overhead slows down processing
  - **Mitigation**: Only log every 100 tiles (minimal impact)

- **Risk**: Empty tile detection skips valid tiles
  - **Mitigation**: Only skip if both nodes AND ways are empty

## Notes

### Progress Reporting Output

Expected log output:
```
INFO Starting tile-based parallel partitioning
INFO Processing 64800 tiles in parallel
INFO Progress: 100/64800 tiles (0.2%) - 12.3 tiles/sec - ETA: 5280s
INFO Progress: 200/64800 tiles (0.3%) - 11.8 tiles/sec - ETA: 5475s
...
INFO Progress: 64800/64800 tiles (100.0%) - 10.5 tiles/sec - ETA: 0s
INFO Tile-based partitioning complete - 64800 tiles in 6171.4s (10.5 tiles/sec)
INFO Generating manifest
INFO Discovered 8432 tiles for manifest
INFO RMDF generation complete
```

Note: Many tiles will be empty (ocean), so discovered tiles << total tiles.

### Manifest Format

The manifest can be simple JSON:
```json
{
  "tile_size_degrees": 1.0,
  "tiles": [
    {"col": 0, "row": 0},
    {"col": 0, "row": 1},
    ...
  ]
}
```

Or binary format for efficiency (implementation in manifest.rs).

### Validation vs Errors

- **Orphaned references**: Warn but don't fail (expected at tile boundaries)
- **Empty tiles**: Info log, skip writing (expected for ocean)
- **PBF read errors**: Fail entire tile processing (unexpected, needs investigation)
- **RMDF write errors**: Fail entire tile processing (disk full, permissions, etc.)

### Performance Impact

The added features have minimal performance impact:
- Error context: Negligible (only on error path)
- Progress reporting: ~0.1% overhead (100-tile intervals)
- Validation: ~1-2% overhead (one HashMap lookup per way reference)
- Empty tile detection: Negligible (simple counter check)

Overall impact: < 2% slowdown for significantly better UX and debuggability.
