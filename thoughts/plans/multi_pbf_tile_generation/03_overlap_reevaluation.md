# Phase 3: Overlap Re-evaluation

## Overview

Implement the `reevaluate_overlap_zones()` method to re-compute proximity flags for nodes in geographic overlap regions between PBF files. This ensures nodes near PBF borders have correct flags computed from the combined proximity grid.

This phase comes after intermediate storage because it needs:
- Grids stored in redb (Phase 2)
- Nodes stored in redb (Phase 2)
- Overlap zones identified (already implemented in `identify_overlap_zones()`)

## Changes Required

### 1. Implement reevaluate_overlap_zones()

**File**: `src/rmdf/generator/mod.rs`

**Changes**: Replace placeholder implementation (lines 281-295) with actual logic.

**Algorithm**:
```
for each overlap_zone in overlap_zones:
    1. Load all grids covering this zone (GridStorage::load_grids_for_region())
    2. Merge grids into combined grid (build_combined_grid())
    3. Query nodes in overlap zone from redb
    4. For each node:
        a. Query combined grid for proximity values
        b. Update node's residential_in_proximity and nogo_area flags
    5. Update nodes in redb
```

### 2. Load Grids for Overlap Zone

Use existing `GridStorage::load_grids_for_region()`:
```rust
let grids = GridStorage::load_grids_for_region(db, &overlap_zone)?;
```

This returns `Vec<RasterizedProximityGrid>` covering the zone.

### 3. Merge Grids into Combined Grid

Use existing `build_combined_grid()` from `src/rmdf/generator/intermediate.rs:539-590`:
```rust
let combined_grid = build_combined_grid(&grids, &overlap_zone)?;
```

This uses:
- `MAX` for residential_sectors (avoid double-counting)
- `OR` for is_military_interior flag

### 4. Query Nodes in Overlap Zone

Need to add a method to query nodes by geographic bounds from redb.

**File**: `src/rmdf/generator/intermediate.rs`

**Changes**: Add method to `IntermediateTile` or create helper:

```rust
pub fn query_nodes_in_bounds(
    db: &redb::Database,
    bounds: &GridBounds,
) -> Result<Vec<(u16, u16, u64, OsmNode)>> {
    // Calculate tile range from bounds
    let (col_min, col_max) = calculate_col_range(bounds.lon_min, bounds.lon_max);
    let (row_min, row_max) = calculate_row_range(bounds.lat_min, bounds.lat_max);
    
    let mut nodes = Vec::new();
    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(TILE_NODES)?;
    
    for col in col_min..=col_max {
        for row in row_min..=row_max {
            let start = (col, row, 0);
            let end = (col, row, u64::MAX);
            for result in table.range(start..=end)? {
                let (key, value) = result?;
                let (_, _, osm_id) = key.value();
                let node: OsmNode = bincode::deserialize(value.value())?;
                
                // Check if node is actually in bounds (not just in tile)
                if bounds.contains(node.lat, node.lon) {
                    nodes.push((col, row, osm_id, node));
                }
            }
        }
    }
    
    Ok(nodes)
}
```

### 5. Re-evaluate Node Flags

Use existing `apply_grid_to_nodes()` from `src/proximity/flag_computer.rs:81-88`:

```rust
// Extract nodes as mutable HashMap
let mut nodes_map: HashMap<u64, OsmNode> = nodes.iter()
    .map(|(_, _, osm_id, node)| (*osm_id, node.clone()))
    .collect();

// Apply combined grid
apply_grid_to_nodes(&mut nodes_map, &combined_grid);

// Extract updated nodes
let updated_nodes: Vec<_> = nodes_map.into_iter().collect();
```

### 6. Update Nodes in redb

Add method to update individual nodes:

**File**: `src/rmdf/generator/intermediate.rs`

**Changes**: Add update method:

```rust
pub fn update_node(
    db: &redb::Database,
    col: u16,
    row: u16,
    osm_id: u64,
    node: &OsmNode,
) -> Result<()> {
    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(TILE_NODES)?;
        let key = (col, row, osm_id);
        let bytes = bincode::serialize(node)?;
        table.insert(key, &bytes[..])?;
    }
    write_txn.commit()?;
    Ok(())
}
```

### 7. Batch Update for Performance

For efficiency, batch update nodes per tile rather than individually:

```rust
pub fn update_nodes_in_tile(
    db: &redb::Database,
    col: u16,
    row: u16,
    nodes: &HashMap<u64, OsmNode>,
) -> Result<()> {
    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(TILE_NODES)?;
        for (osm_id, node) in nodes {
            let key = (col, row, *osm_id);
            let bytes = bincode::serialize(node)?;
            table.insert(key, &bytes[..])?;
        }
    }
    write_txn.commit()?;
    Ok(())
}
```

## Success Criteria

### Automated Verification:
- [ ] Unit test: `query_nodes_in_bounds()` returns correct nodes
- [ ] Unit test: `update_node()` correctly updates node in redb
- [ ] Unit test: Node flags change after re-evaluation with different grid
- [ ] Integration test: Two overlapping grids, node in overlap gets combined flag

### Manual Verification:
- [ ] Create test PBFs with known overlap
- [ ] Verify nodes in overlap zone have flags from combined grid
- [ ] Verify nodes outside overlap zone unchanged

## Dependencies

- Depends on: Phase 2 (Intermediate Storage) - needs grids and nodes in redb
- Blocks: Phase 4 (Final Tile Writing)

## Risks & Mitigations

- **Risk**: Large overlap zones may have many nodes, slow updates
  - **Mitigation**: Batch updates per tile, use single transaction per tile

- **Risk**: Overlap zone boundaries might miss edge nodes
  - **Mitigation**: Use 500m buffer (already in `identify_overlap_zones()`)

## Notes

The `apply_grid_to_nodes()` function exists but expects a `HashMap<u64, OsmNode>`. We need to:
1. Convert redb query results to HashMap
2. Apply grid
3. Convert back and update redb

Consider adding a helper method that does this flow directly.
