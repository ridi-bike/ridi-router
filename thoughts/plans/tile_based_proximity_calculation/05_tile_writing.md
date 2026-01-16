# Phase 5: Tile Writing

## Overview

This phase implements GenerationGraph construction with correct proximity flags and RMDF file writing. We'll build the graph directly from tile data (bypassing IntermediateTile), ensuring flags propagate correctly to the final RMDF output.

This is where the critical bug fix happens: GenerationGraph will be populated with correct flags from OsmNode data.

## Changes Required

### 1. Fix GenerationGraph to Accept Flags

**File**: `src/map_data/generation_graph.rs`

**Changes**: Modify `insert_node()` to use actual flag values

```rust
impl GenerationGraph {
    /// Insert a node from OSM data
    pub fn insert_node(&mut self, node: OsmNode) {
        let point = MapDataPoint {
            id: node.id,
            lat: node.lat as f32,
            lon: node.lon as f32,
            lines: Vec::new(),
            rules: Vec::new(),
            residential_in_proximity: node.residential_in_proximity,  // FIX: Use actual value
            nogo_area: node.nogo_area,                                // FIX: Use actual value
        };

        let idx = self.points.len();
        self.points.push(point);
        self.points_map.insert(node.id, idx);
    }
}
```

**Rationale**: This is the critical bug fix - use actual computed flags instead of hardcoding to false.

### 2. Implement Build Generation Graph

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Replace stub with implementation that populates GenerationGraph

```rust
impl PbfStreamer {
    /// Build GenerationGraph from tile data with proximity flags
    fn build_generation_graph(
        &self,
        nodes: HashMap<u64, OsmNode>,
        ways: Vec<OsmWay>,
        relations: Vec<OsmRelation>,
    ) -> Result<GenerationGraph> {
        let mut graph = GenerationGraph::new();

        // Insert all nodes (with correct proximity flags)
        for (_node_id, node) in nodes {
            graph.insert_node(node);
        }

        // Insert all ways
        for way in ways {
            graph.insert_way(way)
                .context("Failed to insert way into generation graph")?;
        }

        // Insert all relations (turn restrictions)
        for relation in relations {
            graph.insert_relation(relation)
                .context("Failed to insert relation into generation graph")?;
        }

        // Generate point hashes for spatial indexing
        graph.generate_point_hashes();

        Ok(graph)
    }
}
```

**Rationale**:
- Straightforward population of graph from tile data
- Flags from nodes propagate through insert_node() (now fixed)
- Way and relation insertion match current behavior

### 3. Implement Complete Way Insertion Logic

**File**: `src/map_data/generation_graph.rs`

**Changes**: Implement the TODO for `insert_way()`

This is complex logic that needs to be ported from the old MapDataGraph. The key requirements:

```rust
impl GenerationGraph {
    pub fn insert_way(&mut self, way: OsmWay) -> Result<(), MapDataError> {
        // Validate way has at least 2 nodes
        if way.point_ids.len() < 2 {
            return Ok(()); // Skip invalid ways
        }

        // Get tags for this way
        let tags = way.tags.as_ref();
        if tags.is_none() {
            return Ok(()); // Skip ways without tags
        }

        // Extract highway tag
        let highway_tag = tags.unwrap().get("highway");
        if highway_tag.is_none() {
            return Ok(()); // Skip non-highway ways
        }

        // Create line segments between consecutive nodes
        for i in 0..way.point_ids.len() - 1 {
            let from_id = way.point_ids[i];
            let to_id = way.point_ids[i + 1];

            // Look up node indices in graph
            let from_idx = self.points_map.get(&from_id);
            let to_idx = self.points_map.get(&to_id);

            if from_idx.is_none() || to_idx.is_none() {
                // Nodes not in graph (outside tile bounds), skip segment
                continue;
            }

            let from_idx = *from_idx.unwrap();
            let to_idx = *to_idx.unwrap();

            // Create bidirectional line
            let line = MapDataLine {
                from: from_idx,
                to: to_idx,
                distance: self.calculate_distance(from_idx, to_idx),
                way_id: way.id,
            };

            let line_idx = self.lines.len();
            self.lines.push(line);

            // Add line reference to both points
            self.points[from_idx].lines.push(line_idx);
            self.points[to_idx].lines.push(line_idx);

            // Store tags for this line
            let tag_idx = self.tags.insert_or_get(tags.unwrap().clone());
            // Store tag association (way_id -> tag_idx) if needed
        }

        Ok(())
    }

    fn calculate_distance(&self, from_idx: usize, to_idx: usize) -> f32 {
        let from_point = &self.points[from_idx];
        let to_point = &self.points[to_idx];

        // Use Haversine distance
        let from_geo = Point::new(from_point.lon as f64, from_point.lat as f64);
        let to_geo = Point::new(to_point.lon as f64, to_point.lat as f64);
        Haversine.distance(from_geo, to_geo) as f32
    }
}
```

**Note**: This is simplified. The actual implementation needs to match the existing MapDataGraph logic exactly. Review the old implementation for complete logic including:
- Tag handling and storage
- Bidirectional line creation
- Line indexing
- Distance calculation

**Recommendation**: Port the exact logic from the old MapDataGraph if it exists, or implement based on existing patterns in the codebase.

### 4. Implement Relation Insertion Logic

**File**: `src/map_data/generation_graph.rs`

**Changes**: Implement the TODO for `insert_relation()`

```rust
impl GenerationGraph {
    pub fn insert_relation(&mut self, relation: OsmRelation) -> Result<(), MapDataError> {
        // Turn restrictions are processed during routing
        // For now, store the relation data in the tags structure

        // Extract restriction type
        let restriction_type = relation.tags.get("restriction");
        if restriction_type.is_none() {
            return Ok(()); // Skip non-restriction relations
        }

        // Store relation for later processing
        // TODO: Implement proper turn restriction storage
        // This may require adding a relations field to GenerationGraph

        Ok(())
    }
}
```

**Note**: Turn restrictions may require additional data structures in GenerationGraph. Review the existing implementation to understand how relations are stored and used.

### 5. Implement RMDF Tile Writing

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Replace stub with RmdfWriter integration

```rust
use crate::rmdf::generator::writer::RmdfWriter;

impl PbfStreamer {
    /// Write RMDF tile file from generation graph
    fn write_rmdf_tile(
        &self,
        tile_id: TileId,
        graph: GenerationGraph,
    ) -> Result<()> {
        let output_path = self.output_dir.join(tile_id.to_filename());

        // Create RmdfWriter (reusing existing implementation)
        let writer = RmdfWriter::new(self.tile_size_degrees);

        // Write tile (this internally builds spatial index and serializes all sections)
        writer.write_tile_from_graph(tile_id, graph, &output_path)
            .with_context(|| format!("Failed to write RMDF tile {:?}", tile_id))?;

        Ok(())
    }
}
```

**Rationale**: Reuses existing RmdfWriter implementation.

### 6. Add RmdfWriter Method for GenerationGraph

**File**: `src/rmdf/generator/writer.rs`

**Changes**: Add method to write from GenerationGraph

```rust
impl RmdfWriter {
    /// Write RMDF tile directly from GenerationGraph
    pub fn write_tile_from_graph(
        &self,
        tile_id: TileId,
        graph: GenerationGraph,
        output_path: &Path,
    ) -> Result<()> {
        // Build spatial index from graph
        let spatial_index = self.build_spatial_index(&graph)?;

        // Serialize all sections
        let points_data = self.serialize_points(graph.get_points())?;
        let lines_data = self.serialize_lines(graph.get_lines())?;
        let tags_data = self.serialize_tags(graph.get_tags())?;
        let spatial_index_data = self.serialize_spatial_index(&spatial_index)?;

        // Write file header and sections
        self.write_rmdf_file(
            output_path,
            tile_id,
            points_data,
            lines_data,
            tags_data,
            spatial_index_data,
        )?;

        Ok(())
    }
}
```

**Note**: This method may already exist in some form. Review `src/rmdf/generator/writer.rs` to see if integration with GenerationGraph is already implemented. If not, this method connects the graph to the RMDF serialization.

### 7. Verify Flag Serialization

**File**: `src/rmdf/generator/writer.rs`

**Verify**: Check that `serialize_points()` correctly encodes flags (lines 238-247)

The existing code should have:
```rust
flags: {
    let mut flags = 0u16;
    if point.residential_in_proximity {
        flags |= PointRecord::RESIDENTIAL_IN_PROXIMITY_FLAG;
    }
    if point.nogo_area {
        flags |= PointRecord::NOGO_AREA_FLAG;
    }
    flags
},
```

**Action**: Verify this logic exists. If it does, no changes needed.

## Success Criteria

### Automated Verification

- [ ] GenerationGraph.insert_node() uses actual flag values (not hardcoded false)
- [ ] build_generation_graph() successfully creates graph from tile data
- [ ] insert_way() creates line segments and links to points
- [ ] insert_relation() handles turn restrictions
- [ ] write_rmdf_tile() creates valid RMDF files
- [ ] Flag serialization encodes proximity flags into PointRecord
- [ ] Spatial index is built correctly from graph

### Manual Verification

- [ ] Run full pipeline on test PBF file
- [ ] Verify RMDF files are created with non-zero size
- [ ] Open RMDF file and check PointRecord flags field
- [ ] Verify nodes near residential areas have flag bit 0 set
- [ ] Verify nodes in military areas have flag bit 1 set
- [ ] Load RMDF in routing system and verify flags are readable
- [ ] Test routing behavior respects proximity flags

## Dependencies

- Depends on: Phase 4 (Proximity and NoGo Computation)
- Blocks: Phase 6 (Remaining Bits)

## Risks & Mitigations

- **Risk**: Way/relation insertion logic is complex and may have bugs
  - **Mitigation**: Port exact logic from existing MapDataGraph implementation
  - **Testing**: Compare generated tiles with old implementation (if possible)

- **Risk**: GenerationGraph doesn't store all necessary data for RMDF
  - **Mitigation**: Review existing RmdfWriter to understand requirements

- **Risk**: Flags don't propagate correctly through serialization
  - **Mitigation**: Add explicit verification in tests

## Notes

### Critical Bug Fix

This phase fixes the critical bug identified in the research:

**Before** (generation_graph.rs:60-61):
```rust
residential_in_proximity: false,  // BUG: Always false
nogo_area: false,                 // BUG: Always false
```

**After** (this phase):
```rust
residential_in_proximity: node.residential_in_proximity,  // FIX: Use actual value
nogo_area: node.nogo_area,                                // FIX: Use actual value
```

This is the most important change in the entire refactoring - it makes proximity calculations functional.

### Testing Flag Propagation

To verify flags work end-to-end:

1. **Generate tile with known data**:
   ```bash
   cargo run -- generate --input test.osm.pbf --output ./tiles --tile-size 1.0
   ```

2. **Inspect RMDF file**:
   ```rust
   // Write a test program to read PointRecord and check flags
   let tile = RmdfTile::load("tile_204_146.rmdf")?;
   for point in tile.points() {
       if point.residential_in_proximity() {
           println!("Point {} is near residential", point.osm_id);
       }
       if point.nogo_area() {
           println!("Point {} is in nogo area", point.osm_id);
       }
   }
   ```

3. **Verify routing behavior**:
   - Load tiles in routing system
   - Test route through residential area (should apply proximity rules)
   - Test route through military area (should avoid nogo zones)

### GenerationGraph vs MapDataGraph

- **GenerationGraph**: Used during tile generation (write-only)
- **MapDataGraph**: Used during routing (read-only, memory-mapped)

They have similar structures but different purposes. Don't confuse them during implementation.

### RMDF File Format

The RMDF format stores proximity flags in PointRecord.flags (u16):
- Bit 0: residential_in_proximity
- Bit 1: nogo_area
- Bits 2-15: Reserved for future use

This encoding is already implemented in `src/rmdf/format.rs` (lines 94-109).
