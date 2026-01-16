# Phase 2: Main Flow Outline

## Overview

This phase structures the complete per-tile processing pipeline with stub implementations for each major step. This establishes the data flow and interfaces that subsequent phases will implement.

The goal is to create a complete skeleton that compiles and runs (even if it produces empty tiles), establishing the architecture for all subsequent work.

## Changes Required

### 1. Define Tile Processing Data Structures

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Add structures to hold tile-specific data

```rust
/// Data extracted from PBF for a single tile
struct TileData {
    tile_id: TileId,
    nodes: HashMap<u64, OsmNode>,      // OSM ID -> Node with coordinates
    ways: Vec<OsmWay>,
    relations: Vec<OsmRelation>,
}

impl TileData {
    fn new(tile_id: TileId) -> Self {
        Self {
            tile_id,
            nodes: HashMap::new(),
            ways: Vec::new(),
            relations: Vec::new(),
        }
    }
}
```

**Rationale**: Replaces IntermediateTile with in-memory structure (no database serialization).

### 2. Outline Complete Tile Processing Pipeline

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Expand `process_tile()` with full pipeline structure

```rust
impl PbfStreamer {
    fn process_tile(&self, tile_id: TileId) -> Result<()> {
        // Step 1: Calculate bounds
        let core_bounds = self.calculate_tile_bounds(tile_id);
        let buffered_bounds = self.add_buffer_to_bounds(core_bounds);

        // Step 2: Extract PBF data for this tile (stub)
        let tile_data = self.extract_tile_data(tile_id, buffered_bounds)
            .context("Failed to extract tile data from PBF")?;

        // Step 3: Build area grids for proximity computation (stub)
        let (residential_grid, military_grid) = self.build_area_grids(&tile_data, buffered_bounds)
            .context("Failed to build area grids")?;

        // Step 4: Compute proximity flags for nodes in core bounds (stub)
        let nodes_with_flags = self.compute_proximity_flags(
            tile_data.nodes,
            core_bounds,
            &residential_grid,
            &military_grid,
        ).context("Failed to compute proximity flags")?;

        // Step 5: Build generation graph (stub)
        let graph = self.build_generation_graph(
            nodes_with_flags,
            tile_data.ways,
            tile_data.relations,
        ).context("Failed to build generation graph")?;

        // Step 6: Write RMDF tile file (stub)
        self.write_rmdf_tile(tile_id, graph)
            .context("Failed to write RMDF tile")?;

        Ok(())
    }

    // Stub implementations follow...
}
```

**Rationale**:
- Clear step-by-step pipeline mirrors the overview architecture
- Each step has explicit error context for debugging
- Stubs allow compilation and testing of parallel loop
- Later phases fill in each stub

### 3. Add Stub: Extract Tile Data

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Add stub for PBF extraction

```rust
impl PbfStreamer {
    /// Extract nodes, ways, and relations within buffered bounds
    /// TODO: Implement in Phase 3
    fn extract_tile_data(
        &self,
        tile_id: TileId,
        buffered_bounds: TileBounds,
    ) -> Result<TileData> {
        // Stub: Return empty tile data
        info!("Extracting PBF data for tile {:?} (bounds: {:?})", tile_id, buffered_bounds);
        Ok(TileData::new(tile_id))
    }
}
```

**Rationale**: Placeholder for Phase 3 implementation.

### 4. Add Stub: Build Area Grids

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Add stub for area grid construction

```rust
impl PbfStreamer {
    /// Build residential and military AreaGrids from tile data
    /// TODO: Implement in Phase 4
    fn build_area_grids(
        &self,
        tile_data: &TileData,
        buffered_bounds: TileBounds,
    ) -> Result<(AreaGrid, AreaGrid)> {
        // Stub: Return empty grids
        info!("Building area grids for tile {:?}", tile_data.tile_id);
        Ok((AreaGrid::new(), AreaGrid::new()))
    }
}
```

**Rationale**: Placeholder for Phase 4 implementation.

### 5. Add Stub: Compute Proximity Flags

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Add stub for proximity computation

```rust
impl PbfStreamer {
    /// Compute proximity and nogo flags for nodes in core bounds
    /// TODO: Implement in Phase 4
    fn compute_proximity_flags(
        &self,
        nodes: HashMap<u64, OsmNode>,
        core_bounds: TileBounds,
        residential_grid: &AreaGrid,
        military_grid: &AreaGrid,
    ) -> Result<HashMap<u64, OsmNode>> {
        // Stub: Return nodes unchanged (flags will be false)
        info!("Computing proximity flags for {} nodes", nodes.len());
        Ok(nodes)
    }
}
```

**Rationale**: Placeholder for Phase 4 implementation.

### 6. Add Stub: Build Generation Graph

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Add stub for graph construction

```rust
use crate::map_data::generation_graph::GenerationGraph;

impl PbfStreamer {
    /// Build GenerationGraph from tile data with proximity flags
    /// TODO: Implement in Phase 5
    fn build_generation_graph(
        &self,
        nodes: HashMap<u64, OsmNode>,
        ways: Vec<OsmWay>,
        relations: Vec<OsmRelation>,
    ) -> Result<GenerationGraph> {
        // Stub: Return empty graph
        info!("Building generation graph from {} nodes, {} ways, {} relations",
              nodes.len(), ways.len(), relations.len());
        Ok(GenerationGraph::new())
    }
}
```

**Rationale**: Placeholder for Phase 5 implementation.

### 7. Add Stub: Write RMDF Tile

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Add stub for RMDF writing

```rust
impl PbfStreamer {
    /// Write RMDF tile file from generation graph
    /// TODO: Implement in Phase 5
    fn write_rmdf_tile(
        &self,
        tile_id: TileId,
        graph: GenerationGraph,
    ) -> Result<()> {
        // Stub: Create empty file to verify tile writing works
        let output_path = self.output_dir.join(tile_id.to_filename());
        info!("Writing RMDF tile to {:?}", output_path);

        // Create empty file as placeholder
        std::fs::File::create(&output_path)
            .context("Failed to create RMDF tile file")?;

        Ok(())
    }
}
```

**Rationale**:
- Creates actual files to verify parallel writing works
- Will be replaced with real RMDF serialization in Phase 5

### 8. Update Imports

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Add necessary imports for new structures

```rust
use std::collections::HashMap;
use crate::map_data::generation_graph::GenerationGraph;
use crate::map_data::proximity::AreaGrid;
use crate::rmdf::format::TileBounds;
```

**Rationale**: Required for compilation of new code.

## Success Criteria

### Automated Verification

- [ ] Code compiles without errors
- [ ] `process_tile()` executes all 6 steps without panics
- [ ] Each stub function logs its execution
- [ ] Empty RMDF files are created for each tile
- [ ] No crashes during parallel execution

### Manual Verification

- [ ] Run with test PBF file
- [ ] Verify output directory contains tile_*.rmdf files (even if empty)
- [ ] Check logs show all 6 pipeline steps executing for each tile
- [ ] Confirm parallel execution completes successfully
- [ ] Verify no deadlocks or race conditions

## Dependencies

- Depends on: Phase 1 (Parallel Tile Loop Structure)
- Blocks: Phase 3, 4, 5, 6 (all implementation phases)

## Risks & Mitigations

- **Risk**: Stub implementations hide integration issues
  - **Mitigation**: Each stub creates minimal output (logs, empty files) to verify execution

- **Risk**: Data flow between steps not properly structured
  - **Mitigation**: Type signatures and Result wrapping force proper error handling

## Notes

### Pipeline Verification

After this phase, you can verify the pipeline structure:

```bash
cargo build
cargo run -- generate --input test.osm.pbf --output ./tiles --tile-size 1.0
```

Expected output:
```
INFO Starting tile-based parallel partitioning
INFO Processing 64800 tiles in parallel
INFO Extracting PBF data for tile TileId { col: 0, row: 0 } (bounds: ...)
INFO Building area grids for tile TileId { col: 0, row: 0 }
INFO Computing proximity flags for 0 nodes
INFO Building generation graph from 0 nodes, 0 ways, 0 relations
INFO Writing RMDF tile to "./tiles/tile_0_0.rmdf"
...
INFO Processed 10/64800 tiles
...
INFO Tile-based partitioning complete
```

Output directory should contain many empty `tile_*_*.rmdf` files.

### Architecture Benefits

This phase establishes:
- Clear separation of concerns (each step is independent function)
- Type-safe data flow (TileData → AreaGrids → flags → GenerationGraph → RMDF)
- Testable structure (each function can be unit tested)
- Error handling boundaries (each step can fail independently)

### Memory Characteristics

With stubs:
- Memory per tile: ~1 KB (empty structures)
- Rayon will process multiple tiles concurrently based on CPU cores
- No memory pressure from actual data yet

Once implemented:
- Memory per tile: Variable based on tile density (urban vs rural)
- Rayon automatically limits concurrency based on available memory
- AreaGrids are dropped after tile processing (memory reclaimed)
