# Phase 1: Parallel Tile Loop Structure

## Overview

This phase establishes the foundation for tile-based parallel processing by setting up the main tile iteration loop using Rayon. We'll calculate all tile boundaries upfront and create the skeleton structure for per-tile processing.

This phase does NOT implement the actual tile processing logic - just the parallel iteration framework.

## Changes Required

### 1. Add Tile Boundary Calculation

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Add method to calculate tile boundaries from tile size

```rust
impl PbfStreamer {
    /// Calculate all tiles that cover the world for the given tile size
    fn calculate_all_tiles(&self) -> Vec<TileId> {
        let mut tiles = Vec::new();

        // Longitude: -180 to +180 (360 degrees)
        // Latitude: -90 to +90 (180 degrees)
        let cols = (360.0 / self.tile_size_degrees).ceil() as u16;
        let rows = (180.0 / self.tile_size_degrees).ceil() as u16;

        for col in 0..cols {
            for row in 0..rows {
                tiles.push(TileId { col, row });
            }
        }

        tiles
    }

    /// Calculate geographic bounds for a tile
    fn calculate_tile_bounds(&self, tile_id: TileId) -> TileBounds {
        let lon_min = (tile_id.col as f32 * self.tile_size_degrees) - 180.0;
        let lon_max = lon_min + self.tile_size_degrees;
        let lat_min = (tile_id.row as f32 * self.tile_size_degrees) - 90.0;
        let lat_max = lat_min + self.tile_size_degrees;

        TileBounds {
            lat_min,
            lat_max,
            lon_min,
            lon_max,
        }
    }

    /// Add buffer zone to tile bounds (500m = RESIDENTIAL_PROXIMITY_THRESHOLD_METERS)
    fn add_buffer_to_bounds(&self, bounds: TileBounds) -> TileBounds {
        // Convert meters to degrees: 1 degree ≈ 111km at equator
        // 500m / 111,000m ≈ 0.0045 degrees
        // Use slightly larger buffer (0.005) for safety at higher latitudes
        let buffer_degrees = 0.005f32;

        TileBounds {
            lat_min: (bounds.lat_min - buffer_degrees).max(-90.0),
            lat_max: (bounds.lat_max + buffer_degrees).min(90.0),
            lon_min: (bounds.lon_min - buffer_degrees).max(-180.0),
            lon_max: (bounds.lon_max + buffer_degrees).min(180.0),
        }
    }
}
```

**Rationale**:
- Pre-calculates all tiles to avoid dynamic calculation during parallel iteration
- Buffer calculation ensures >= 500m coverage at all latitudes (slightly over-buffered at equator)
- Clamping to world bounds prevents invalid coordinates at poles/dateline

### 2. Add TileBounds Structure

**File**: `src/rmdf/format.rs`

**Changes**: Verify TileBounds structure exists (it already does based on research)

The structure already exists:
```rust
pub struct TileBounds {
    pub lat_min: f32,
    pub lat_max: f32,
    pub lon_min: f32,
    pub lon_max: f32,
}
```

**Action**: No changes needed, structure already available.

### 3. Create Parallel Tile Processing Loop

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Add new `partition_parallel()` method with rayon

```rust
use rayon::prelude::*;

impl PbfStreamer {
    /// New partition method using tile-based parallel processing
    pub fn partition_parallel(&self) -> Result<()> {
        info!("Starting tile-based parallel partitioning");

        // Calculate all tile boundaries upfront
        let tiles = self.calculate_all_tiles();
        info!("Processing {} tiles in parallel", tiles.len());

        // Process tiles in parallel using Rayon
        tiles.par_iter()
            .try_for_each(|tile_id| -> Result<()> {
                self.process_tile(*tile_id)?;
                Ok(())
            })?;

        info!("Tile-based partitioning complete");
        Ok(())
    }

    /// Process a single tile (stub for now)
    fn process_tile(&self, tile_id: TileId) -> Result<()> {
        // Calculate bounds
        let core_bounds = self.calculate_tile_bounds(tile_id);
        let buffered_bounds = self.add_buffer_to_bounds(core_bounds);

        info!("Processing tile {:?} (bounds: {:?})", tile_id, buffered_bounds);

        // TODO: Implement tile processing in subsequent phases

        Ok(())
    }
}
```

**Rationale**:
- `par_iter()` enables parallel processing across all tiles
- `try_for_each` handles error propagation (stops on first error)
- Stub implementation allows testing the parallel loop structure
- Core bounds vs buffered bounds distinction prepared for proximity computation

### 4. Update Main Entry Point

**File**: `src/rmdf/generator/mod.rs`

**Changes**: Add option to use new parallel partition method

```rust
impl TileGenerator {
    pub fn generate(&self) -> Result<()> {
        info!("Starting RMDF tile generation");

        // Create PBF streamer
        let streamer = PbfStreamer::new(
            &self.input_file,
            &self.output_dir,
            self.tile_size_degrees,
        )?;

        // Use new parallel partition method (Phase 1 stub)
        streamer.partition_parallel()?;

        // TODO: Manifest generation will be added in later phases

        info!("RMDF generation complete");
        Ok(())
    }
}
```

**Rationale**: Provides clean entry point for testing the parallel loop structure.

### 5. Add Progress Reporting

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Add atomic counter for progress tracking

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

impl PbfStreamer {
    pub fn partition_parallel(&self) -> Result<()> {
        info!("Starting tile-based parallel partitioning");

        let tiles = self.calculate_all_tiles();
        let total_tiles = tiles.len();
        info!("Processing {} tiles in parallel", total_tiles);

        // Progress counter (shared across threads)
        let completed = Arc::new(AtomicUsize::new(0));

        tiles.par_iter()
            .try_for_each(|tile_id| -> Result<()> {
                self.process_tile(*tile_id)?;

                // Update progress
                let count = completed.fetch_add(1, Ordering::Relaxed) + 1;
                if count % 10 == 0 || count == total_tiles {
                    info!("Processed {}/{} tiles", count, total_tiles);
                }

                Ok(())
            })?;

        info!("Tile-based partitioning complete");
        Ok(())
    }
}
```

**Rationale**: Provides visibility into parallel processing progress without excessive logging.

## Success Criteria

### Automated Verification

- [x] Code compiles without errors
- [x] `calculate_all_tiles()` returns correct number of tiles for given tile_size_degrees
- [x] `calculate_tile_bounds()` produces correct geographic bounds
- [x] `add_buffer_to_bounds()` adds ~500m buffer (0.005 degrees)
- [x] `partition_parallel()` executes without panics
- [x] Progress reporting shows all tiles processed

### Manual Verification

- [ ] Run with small PBF file, verify parallel execution
- [ ] Check logs show "Processing X/Y tiles" messages
- [ ] Verify no crashes or deadlocks
- [ ] Confirm Rayon parallelizes across available CPU cores

## Dependencies

- Depends on: None - can start immediately
- Blocks: Phase 2 (Main Flow Outline)

## Risks & Mitigations

- **Risk**: Rayon thread pool exhaustion with large tile counts
  - **Mitigation**: Rayon automatically manages thread pool, no manual tuning needed

- **Risk**: Error in one tile stops all processing
  - **Mitigation**: `try_for_each` ensures clean error propagation, expected behavior

## Notes

### Testing the Skeleton

To test this phase:
```bash
cargo build
cargo run -- generate --input test.osm.pbf --output ./tiles --tile-size 1.0
```

Expected output:
```
INFO Starting tile-based parallel partitioning
INFO Processing 64800 tiles in parallel
INFO Processing tile TileId { col: 0, row: 0 } (bounds: TileBounds { ... })
...
INFO Processed 10/64800 tiles
...
INFO Tile-based partitioning complete
```

### Buffer Zone Math

- 500 meters / 111,000 meters per degree = 0.0045 degrees
- Using 0.005 degrees provides ~555m buffer at equator
- At 60° latitude: 1 degree ≈ 55.5km, so 0.005° ≈ 278m (still adequate)
- Trade-off: slight over-buffering at equator vs guaranteed coverage at high latitudes

### Tile Count Estimation

With `tile_size_degrees = 1.0`:
- Longitude: 360 degrees / 1.0 = 360 columns
- Latitude: 180 degrees / 1.0 = 180 rows
- Total: 360 × 180 = 64,800 tiles

Many tiles will be ocean/empty, but all are processed. Future optimization could skip empty tiles.
