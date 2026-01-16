# Phase 4: Proximity and NoGo Computation

## Overview

This phase implements tile-specific AreaGrid construction and proximity flag computation. We'll build separate residential and military AreaGrids from the filtered PBF data, then compute proximity flags for nodes within the core tile bounds.

This replaces the stubs `build_area_grids()` and `compute_proximity_flags()` with real implementations that match the current proximity calculation logic.

## Changes Required

### 1. Implement Area Grid Construction

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Replace stub with implementation that builds AreaGrids from tile data

```rust
use crate::osm_data::pbf_area_reader::PbfAreaReader;
use geo::{MultiPolygon, Point, Distance, Haversine, HaversineClosestPoint, CoordsIter, GeodesicArea};

impl PbfStreamer {
    /// Build residential and military AreaGrids from tile data
    fn build_area_grids(
        &self,
        tile_data: &TileData,
        buffered_bounds: TileBounds,
    ) -> Result<(AreaGrid, AreaGrid)> {
        // Build grids by filtering PBF data for residential/military areas within buffered bounds
        // We need to re-read the PBF file to extract area polygons

        // Extract residential areas
        let residential_grid = self.extract_residential_areas_for_bounds(buffered_bounds)
            .context("Failed to extract residential areas")?;

        // Extract military areas
        let military_grid = self.extract_military_areas_for_bounds(buffered_bounds)
            .context("Failed to extract military areas")?;

        info!(
            "Built area grids for tile {:?}: {} residential cells, {} military cells",
            tile_data.tile_id,
            residential_grid.len(),
            military_grid.len()
        );

        Ok((residential_grid, military_grid))
    }

    /// Extract residential areas within buffered bounds
    fn extract_residential_areas_for_bounds(&self, bounds: TileBounds) -> Result<AreaGrid> {
        let file = File::open(&self.input_file)
            .context("Failed to open PBF file for residential areas")?;
        let mut pbf = OsmPbfReader::new(file);

        let mut boundary_reader = PbfAreaReader::new(&mut pbf);

        // Read all residential areas (landuse=residential)
        boundary_reader.read(&|obj| {
            (obj.is_way() || obj.is_relation()) && obj.tags().contains("landuse", "residential")
        })?;

        // Filter to only areas within or intersecting buffered bounds
        let area_grid = self.filter_area_grid_by_bounds(
            boundary_reader.get_area_grid(),
            bounds
        )?;

        Ok(area_grid)
    }

    /// Extract military areas within buffered bounds
    fn extract_military_areas_for_bounds(&self, bounds: TileBounds) -> Result<AreaGrid> {
        let file = File::open(&self.input_file)
            .context("Failed to open PBF file for military areas")?;
        let mut pbf = OsmPbfReader::new(file);

        let mut boundary_reader = PbfAreaReader::new(&mut pbf);

        // Read all military areas (landuse=military)
        boundary_reader.read(&|obj| {
            (obj.is_way() || obj.is_relation()) && obj.tags().contains("landuse", "military")
        })?;

        // Filter to only areas within or intersecting buffered bounds
        let area_grid = self.filter_area_grid_by_bounds(
            boundary_reader.get_area_grid(),
            bounds
        )?;

        Ok(area_grid)
    }

    /// Filter AreaGrid to only include polygons intersecting bounds
    fn filter_area_grid_by_bounds(
        &self,
        mut full_grid: AreaGrid,
        bounds: TileBounds,
    ) -> Result<AreaGrid> {
        // Note: Current implementation of AreaGrid doesn't support filtering
        // For now, return the full grid (inefficient but correct)
        // TODO: Implement grid filtering if memory becomes an issue

        // The grid will be dropped after tile processing, so memory impact is temporary
        Ok(full_grid)
    }
}
```

**Rationale**:
- Reuses existing `PbfAreaReader` to build AreaGrids (proven implementation)
- Filtering by bounds is deferred (TODO) - full grids work but use more memory
- Separate passes for residential and military areas (cleaner than combined)
- Grid filtering optimization can be added later if memory is a concern

**Note**: The current AreaGrid implementation (from `src/map_data/proximity.rs`) doesn't have a built-in filtering method. For the initial implementation, we'll extract all areas globally, which is inefficient but correct. The memory impact is mitigated because:
1. Grids are dropped after tile processing
2. Only Rayon's concurrency limit tiles are processed at once
3. Future optimization can add grid filtering

### 2. Implement Proximity Flag Computation

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Replace stub with implementation matching current proximity logic

```rust
impl PbfStreamer {
    /// Compute proximity and nogo flags for nodes in core bounds
    fn compute_proximity_flags(
        &self,
        mut nodes: HashMap<u64, OsmNode>,
        core_bounds: TileBounds,
        residential_grid: &AreaGrid,
        military_grid: &AreaGrid,
    ) -> Result<HashMap<u64, OsmNode>> {
        let mut computed_count = 0;

        for (node_id, node) in nodes.iter_mut() {
            // Only compute for nodes in core bounds (not buffer zone)
            if !self.point_in_bounds(node.lat, node.lon, core_bounds) {
                continue;
            }

            // Compute residential proximity flag
            node.residential_in_proximity = Self::compute_residential_proximity(
                node.lat,
                node.lon,
                residential_grid,
            );

            // Compute nogo area flag
            node.nogo_area = Self::compute_nogo_area(
                node.lat,
                node.lon,
                military_grid,
            );

            computed_count += 1;
        }

        info!("Computed proximity flags for {} nodes in core bounds", computed_count);

        Ok(nodes)
    }

    /// Compute residential proximity flag (from pbf_streamer.rs:401-432)
    fn compute_residential_proximity(lat: f64, lon: f64, residential_areas: &AreaGrid) -> bool {
        let tot_area = match residential_areas.find_closest_areas_refs(
            lat as f32,
            lon as f32,
            1,  // Search 1 grid step (~1.1km)
        ) {
            Some(areas) => areas.iter().fold(0., |tot, multi_polygon| {
                let geo_point = Point::new(lon, lat);
                let distance = match multi_polygon.haversine_closest_point(&geo_point) {
                    geo::Closest::Intersection(_) => 0.,
                    geo::Closest::SinglePoint(p) => {
                        Haversine.distance(p, geo_point)
                    }
                    geo::Closest::Indeterminate => multi_polygon
                        .coords_iter()
                        .fold(10000., |min, coords| {
                            let dist = Haversine.distance(geo_point, Point::from(coords));
                            if dist < min { dist } else { min }
                        }),
                };

                if distance <= RESIDENTIAL_PROXIMITY_THRESHOLD_METERS {
                    let area = multi_polygon.geodesic_area_signed().abs();
                    return tot + area;
                }
                tot
            }),
            None => 0.,
        };

        tot_area > THRESHOLD_AREA
    }

    /// Compute nogo area flag (from pbf_streamer.rs:435-453)
    fn compute_nogo_area(lat: f64, lon: f64, military_areas: &AreaGrid) -> bool {
        match military_areas.find_closest_areas_refs(
            lat as f32,
            lon as f32,
            1,
        ) {
            None => false,
            Some(areas) => areas.iter().any(|multi_polygon| {
                let geo_point = Point::new(lon, lat);
                match multi_polygon.haversine_closest_point(&geo_point) {
                    geo::Closest::Intersection(p) => {
                        // Only mark as nogo if inside military area >100m from boundary
                        Haversine.distance(geo_point, p) > MILITARY_ENTRY_MAX_M
                    }
                    geo::Closest::SinglePoint(_) => false,
                    geo::Closest::Indeterminate => false,
                }
            }),
        }
    }
}
```

**Rationale**:
- **Exact copy of current logic**: Ensures identical behavior to existing implementation
- **Core bounds filtering**: Only computes flags for nodes in core tile (not buffer zone)
- **Grid search radius**: Uses 1 step (~1.1km) which covers 500m threshold with margin
- **Haversine distance**: Geodesic distance calculation for accuracy

### 3. Update Constants (Already Exist)

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Verify**: Constants are already defined at lines 26-32

```rust
const RESIDENTIAL_PROXIMITY_THRESHOLD_METERS: f64 = 500.0;
const RESIDENTIAL_PART_COVERED: f64 = 0.10;
const THRESHOLD_AREA: f64 = (RESIDENTIAL_PROXIMITY_THRESHOLD_METERS
    * RESIDENTIAL_PROXIMITY_THRESHOLD_METERS
    * std::f64::consts::PI)
    * RESIDENTIAL_PART_COVERED;
const MILITARY_ENTRY_MAX_M: f64 = 100.0;
```

**Action**: No changes needed.

### 4. Add Fallback for AreaGrid Construction Failure

**File**: `src/rmdf/generator/pbf_streamer.rs`

**Changes**: Add graceful degradation if area extraction fails

```rust
impl PbfStreamer {
    fn build_area_grids(
        &self,
        tile_data: &TileData,
        buffered_bounds: TileBounds,
    ) -> Result<(AreaGrid, AreaGrid)> {
        // Try to extract residential areas
        let residential_grid = self.extract_residential_areas_for_bounds(buffered_bounds)
            .unwrap_or_else(|e| {
                warn!(
                    "Failed to extract residential areas for tile {:?}: {:?}. Using empty grid.",
                    tile_data.tile_id, e
                );
                AreaGrid::new()
            });

        // Try to extract military areas
        let military_grid = self.extract_military_areas_for_bounds(buffered_bounds)
            .unwrap_or_else(|e| {
                warn!(
                    "Failed to extract military areas for tile {:?}: {:?}. Using empty grid.",
                    tile_data.tile_id, e
                );
                AreaGrid::new()
            });

        info!(
            "Built area grids for tile {:?}: {} residential cells, {} military cells",
            tile_data.tile_id,
            residential_grid.len(),
            military_grid.len()
        );

        Ok((residential_grid, military_grid))
    }
}
```

**Rationale**: Per requirements, if AreaGrid construction fails, mark all flags as false (empty grid achieves this).

## Success Criteria

### Automated Verification

- [x] Code compiles without errors
- [x] `build_area_grids()` successfully extracts residential and military areas
- [x] `compute_proximity_flags()` computes flags only for core bounds nodes
- [x] Proximity calculation matches current implementation logic
- [x] NoGo calculation matches current implementation logic
- [x] Empty grids (no residential/military areas) don't cause errors
- [x] Flags remain false for nodes outside core bounds (buffer zone nodes)

### Manual Verification

- [ ] Run with PBF file containing known residential areas
- [ ] Verify nodes near residential areas get `residential_in_proximity = true`
- [ ] Verify nodes far from residential areas get `residential_in_proximity = false`
- [ ] Verify nodes inside military areas get `nogo_area = true`
- [ ] Verify nodes outside military areas get `nogo_area = false`
- [ ] Check logs show realistic area grid cell counts
- [ ] Verify memory usage per tile is reasonable (AreaGrids are large but temporary)

## Dependencies

- Depends on: Phase 3 (PBF Extraction and Filtering)
- Blocks: Phase 5 (Tile Writing)

## Risks & Mitigations

- **Risk**: Global area extraction (no bounds filtering) uses too much memory
  - **Mitigation**: Grids are dropped after tile processing, Rayon limits concurrency
  - **Future optimization**: Add grid filtering by bounds if needed

- **Risk**: Multiple PBF passes per tile (nodes, ways, residential, military) too slow
  - **Mitigation**: Accept slower processing for correct implementation
  - **Future optimization**: Combine area extraction passes if needed

- **Risk**: Proximity calculation doesn't match old implementation
  - **Mitigation**: Code is exact copy from current pbf_streamer.rs

## Notes

### PBF Pass Count Per Tile

Current implementation:
1. Extract nodes (first pass)
2. Extract ways/relations (second pass)
3. Extract residential areas (third pass)
4. Extract military areas (fourth pass)

**Total: 4 PBF file opens per tile**

With 64,800 tiles, this is 259,200 PBF iterations. However:
- Rayon parallelizes across tiles (N concurrent = N × 4 file opens)
- OS disk caching helps (same file read repeatedly)
- SSD random I/O performance mitigates sequential reading

### Memory Profile Per Tile

Components:
- TileData (nodes/ways/relations): ~10 KB - 10 MB
- Residential AreaGrid: ~1 MB - 100 MB (depending on urban density)
- Military AreaGrid: ~100 KB - 10 MB (typically smaller)
- GenerationGraph: ~10 KB - 10 MB

**Peak memory per tile**: ~2 MB - 120 MB
**Rayon concurrency**: 8 cores × 120 MB = 960 MB peak (acceptable)

### AreaGrid Filtering Optimization (Future)

To reduce memory, AreaGrid could be filtered by bounds:

```rust
fn filter_area_grid_by_bounds(
    &self,
    full_grid: AreaGrid,
    bounds: TileBounds,
) -> Result<AreaGrid> {
    let mut filtered_grid = AreaGrid::new();

    // Only insert polygons that intersect bounds
    for multi_polygon in full_grid.get_all_polygons() {
        if polygon_intersects_bounds(multi_polygon, bounds) {
            filtered_grid.insert_multi_polygon(multi_polygon);
        }
    }

    Ok(filtered_grid)
}
```

This requires adding `get_all_polygons()` to AreaGrid. Defer this optimization until profiling shows it's needed.

### Testing Proximity Calculation

To verify correctness:

1. **Known residential area test**:
   - Find coordinates inside a residential area in test PBF
   - Verify node gets `residential_in_proximity = true`

2. **Boundary test**:
   - Find coordinates exactly 500m from residential boundary
   - Verify flag is correctly set based on area coverage

3. **Military area test**:
   - Find coordinates >100m inside military area
   - Verify node gets `nogo_area = true`

4. **Edge of military area test**:
   - Find coordinates <100m from military boundary
   - Verify node gets `nogo_area = false`

### Buffer Zone Importance

The 500m buffer zone is critical for correct proximity calculation:

- Without buffer: Node 400m inside tile might be within 500m of residential area 100m outside tile
- With buffer: AreaGrid includes areas up to 555m outside core bounds
- Result: All nodes get correct classification regardless of position in tile
