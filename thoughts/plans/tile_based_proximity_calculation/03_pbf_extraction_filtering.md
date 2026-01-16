# Phase 3: PBF Extraction and Filtering

## Overview

This phase implements geographic bounds filtering to extract tile-specific PBF data. Since osmpbfreader doesn't support native spatial filtering, we'll iterate through the PBF file and filter elements by geographic coordinates.

This replaces the stub `extract_tile_data()` with real implementation that opens a new PBF reader per tile and extracts nodes, ways, and relations within buffered bounds.

## Changes Required

### 1. Implement Point-in-Bounds Check

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Add helper to check if coordinates are within bounds

```rust
impl PbfStreamer {
    /// Check if a point is within geographic bounds
    fn point_in_bounds(&self, lat: f64, lon: f64, bounds: TileBounds) -> bool {
        lat >= bounds.lat_min as f64 && lat < bounds.lat_max as f64 &&
        lon >= bounds.lon_min as f64 && lon < bounds.lon_max as f64
    }
}
```

**Rationale**: Simple comparison for filtering nodes by geographic location.

### 2. Implement PBF Data Extraction

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Replace stub with real implementation

```rust
impl PbfStreamer {
    /// Extract nodes, ways, and relations within buffered bounds
    fn extract_tile_data(
        &self,
        tile_id: TileId,
        buffered_bounds: TileBounds,
    ) -> Result<TileData> {
        // Open new PBF reader for this tile (thread-safe)
        let file = File::open(&self.input_file)
            .with_context(|| format!("Failed to open PBF file for tile {:?}", tile_id))?;
        let mut pbf = OsmPbfReader::new(file);

        let mut tile_data = TileData::new(tile_id);

        // Phase 1: Collect all nodes in buffered bounds
        // We need to track which nodes are in bounds for filtering ways/relations
        let mut nodes_in_bounds = std::collections::HashSet::new();

        for obj_result in pbf.iter() {
            let obj = obj_result
                .with_context(|| format!("Failed to read PBF object for tile {:?}", tile_id))?;

            match obj {
                OsmObj::Node(node) => {
                    let lat = node.lat();
                    let lon = node.lon();

                    if self.point_in_bounds(lat, lon, buffered_bounds) {
                        nodes_in_bounds.insert(node.id.0);

                        let osm_node = OsmNode {
                            id: node.id.0 as u64,
                            lat,
                            lon,
                            residential_in_proximity: false,  // Will be computed in Phase 4
                            nogo_area: false,                 // Will be computed in Phase 4
                        };

                        tile_data.nodes.insert(osm_node.id, osm_node);
                    }
                }
                _ => {} // Collect ways and relations in second pass
            }
        }

        // Phase 2: Re-open PBF and collect ways/relations that reference nodes in bounds
        let file = File::open(&self.input_file)
            .with_context(|| format!("Failed to reopen PBF file for tile {:?}", tile_id))?;
        let mut pbf = OsmPbfReader::new(file);

        for obj_result in pbf.iter() {
            let obj = obj_result?;

            match obj {
                OsmObj::Way(way) => {
                    // Filter highways only (matching current behavior)
                    let has_highway = way.tags.iter().any(|(k, v)| {
                        k == "highway" && (
                            ALLOWED_HIGHWAY_VALUES.contains(&v.as_str()) ||
                            (v == "path" && way.tags.iter().any(|(k2, v2)| k2 == "motorcycle" && v2 == "yes"))
                        )
                    });

                    if !has_highway {
                        continue;
                    }

                    // Include way if any node is in bounds
                    let has_node_in_bounds = way.nodes.iter()
                        .any(|node_id| nodes_in_bounds.contains(&node_id.0));

                    if has_node_in_bounds {
                        let osm_way = OsmWay {
                            id: way.id.0 as u64,
                            point_ids: way.nodes.iter().map(|n| n.0 as u64).collect(),
                            tags: Some(way.tags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()),
                        };
                        tile_data.ways.push(osm_way);
                    }
                }
                OsmObj::Relation(relation) => {
                    // Filter restriction relations only (matching current behavior)
                    let is_restriction = relation.tags.iter()
                        .any(|(k, v)| k == "type" && v.starts_with("restriction"));

                    if !is_restriction {
                        continue;
                    }

                    // Include relation if any member node is in bounds
                    let has_member_in_bounds = relation.refs.iter().any(|r| {
                        if let osmpbfreader::OsmId::Node(node_id) = r.member {
                            nodes_in_bounds.contains(&node_id.0)
                        } else {
                            false
                        }
                    });

                    if has_member_in_bounds {
                        let osm_relation = OsmRelation {
                            id: relation.id.0 as u64,
                            members: relation.refs.iter().filter_map(|r| {
                                let role = match r.role.as_str() {
                                    "from" => OsmRelationMemberRole::From,
                                    "to" => OsmRelationMemberRole::To,
                                    "via" => OsmRelationMemberRole::Via,
                                    _ => return None,
                                };

                                let (member_ref, member_type) = match r.member {
                                    osmpbfreader::OsmId::Way(id) => (id.0 as u64, OsmRelationMemberType::Way),
                                    osmpbfreader::OsmId::Node(id) => (id.0 as u64, OsmRelationMemberType::Node),
                                    osmpbfreader::OsmId::Relation(_) => return None, // Skip nested relations
                                };

                                Some(OsmRelationMember {
                                    member_ref,
                                    role,
                                    member_type,
                                })
                            }).collect(),
                            tags: relation.tags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
                        };
                        tile_data.relations.push(osm_relation);
                    }
                }
                _ => {} // Nodes already collected in first pass
            }
        }

        info!(
            "Extracted tile {:?}: {} nodes, {} ways, {} relations",
            tile_id,
            tile_data.nodes.len(),
            tile_data.ways.len(),
            tile_data.relations.len()
        );

        Ok(tile_data)
    }
}
```

**Rationale**:
- **Two-pass approach**: First pass collects nodes, second pass filters ways/relations
- **HashSet tracking**: nodes_in_bounds enables efficient way/relation filtering
- **File reopening**: OsmPbfReader requires &mut, cannot share across iterations
- **Filtering logic**: Matches current pbf_streamer.rs behavior (highways, restrictions only)
- **Buffered bounds**: Uses buffered bounds to capture border elements

### 3. Optimize: Single-Pass Collection

**Alternative implementation** (optional optimization):

If two PBF passes are too slow, we can use a single pass with delayed way/relation processing:

```rust
fn extract_tile_data_single_pass(
    &self,
    tile_id: TileId,
    buffered_bounds: TileBounds,
) -> Result<TileData> {
    let file = File::open(&self.input_file)?;
    let mut pbf = OsmPbfReader::new(file);

    let mut tile_data = TileData::new(tile_id);
    let mut nodes_in_bounds = std::collections::HashSet::new();
    let mut pending_ways = Vec::new();
    let mut pending_relations = Vec::new();

    // Collect everything in one pass
    for obj_result in pbf.iter() {
        let obj = obj_result?;
        match obj {
            OsmObj::Node(node) => {
                if self.point_in_bounds(node.lat(), node.lon(), buffered_bounds) {
                    nodes_in_bounds.insert(node.id.0);
                    // ... insert into tile_data.nodes
                }
            }
            OsmObj::Way(way) => {
                pending_ways.push(way);
            }
            OsmObj::Relation(relation) => {
                pending_relations.push(relation);
            }
        }
    }

    // Filter ways by nodes_in_bounds
    for way in pending_ways {
        // ... filter and add to tile_data.ways
    }

    // Filter relations by nodes_in_bounds
    for relation in pending_relations {
        // ... filter and add to tile_data.relations
    }

    Ok(tile_data)
}
```

**Trade-off**: Single pass is faster but uses more memory (stores all ways/relations in memory).

**Recommendation**: Start with two-pass approach for simplicity, optimize later if needed.

### 4. Add Validation and Edge Cases

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Add validation for empty tiles

```rust
impl PbfStreamer {
    fn extract_tile_data(
        &self,
        tile_id: TileId,
        buffered_bounds: TileBounds,
    ) -> Result<TileData> {
        // ... extraction logic ...

        // Log warning for empty tiles (common for ocean tiles)
        if tile_data.nodes.is_empty() && tile_data.ways.is_empty() {
            info!("Tile {:?} is empty (likely ocean or unpopulated area)", tile_id);
        }

        Ok(tile_data)
    }
}
```

**Rationale**: Many tiles will be empty (ocean, poles) - this is expected, not an error.

## Success Criteria

### Automated Verification

- [ ] Code compiles without errors
- [ ] `extract_tile_data()` successfully filters nodes by buffered bounds
- [ ] Ways are included if any node is in bounds
- [ ] Relations are included if any member node is in bounds
- [ ] Empty tiles (ocean) don't cause errors
- [ ] Highway filtering matches current behavior
- [ ] Restriction relation filtering matches current behavior

### Manual Verification

- [ ] Run with real PBF file containing known data
- [ ] Verify tiles at land locations contain nodes/ways
- [ ] Verify tiles at ocean locations are empty
- [ ] Verify tiles at borders include ways crossing boundaries (due to buffer)
- [ ] Check logs show realistic node/way/relation counts per tile
- [ ] Verify no crashes during parallel extraction

## Dependencies

- Depends on: Phase 2 (Main Flow Outline)
- Blocks: Phase 4 (Proximity and NoGo Computation)

## Risks & Mitigations

- **Risk**: Two PBF passes per tile may be too slow
  - **Mitigation**: Start with simple approach, profile, optimize to single-pass if needed

- **Risk**: Large PBF files cause file descriptor exhaustion
  - **Mitigation**: File is opened and closed per tile, no descriptor leaks

- **Risk**: Border elements missed due to insufficient buffer
  - **Mitigation**: Buffer is 0.005 degrees (~555m), exceeds 500m requirement

## Notes

### Performance Considerations

**PBF Reading Cost**:
- Two passes per tile × number of tiles = high I/O
- Example: 64,800 tiles × 2 passes = 129,600 PBF iterations
- Mitigated by: Rayon parallelism, OS disk caching, SSD speed

**Memory Usage**:
- Per-tile: ~10 KB - 10 MB depending on tile density
- Rayon: Processes N tiles concurrently (N = CPU cores)
- Total peak: Rayon concurrency × max tile size

### Testing Strategy

Test with small PBF file (e.g., city extract):
```bash
# Download test data
wget https://download.geofabrik.de/europe/monaco-latest.osm.pbf

# Generate tiles
cargo run -- generate --input monaco-latest.osm.pbf --output ./tiles --tile-size 0.1

# Verify output
ls -lh ./tiles/
# Should see tile files with varying sizes
```

### Buffer Zone Verification

To verify buffer zone works correctly:
1. Find a tile boundary in the data
2. Place a test node exactly on the boundary
3. Verify the node appears in both adjacent tiles' extracted data
4. Verify ways crossing the boundary are included in both tiles

### Highway Values Reference

Current allowed highway values (matching `pbf_streamer.rs:18-23`):
```rust
["motorway", "trunk", "primary", "secondary", "tertiary",
 "unclassified", "residential", "motorway_link", "trunk_link",
 "primary_link", "secondary_link", "tertiary_link",
 "living_street", "track", "escape", "raceway", "road"]
```

Plus: `highway=path` with `motorcycle=yes`

This filtering significantly reduces the amount of data per tile.
