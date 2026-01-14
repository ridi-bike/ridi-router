# Phase 3: Proximity & Nogo Computation with Overlap

## Overview

Implement proximity and nogo area computation for each tile using a 500m buffer overlap strategy. This phase computes the `residential_in_proximity` and `nogo_area` flags for all nodes, which were initialized to false in Phase 2.

**Goals:**
- Extract residential and military area polygons from PBF
- Implement 500m tile boundary overlap for proximity computation
- Parallelize computation across tiles using rayon
- Update intermediate tile buffers with computed flags

## Changes Required

### 1. Create Proximity Computation Module

**File**: `src/rmdf/generator/proximity.rs` (new file)

**Changes**: Implement overlap-based proximity computation

```rust
use anyhow::{Context, Result};
use geo::{Point, Distance, Haversine, HaversineClosestPoint, CoordsIter, GeodesicArea};
use rayon::prelude::*;
use std::path::Path;
use tracing::info;

use crate::map_data::proximity::AreaGrid;
use crate::osm_data::pbf_area_reader::PbfAreaReader;
use crate::rmdf::format::{TileId, TileBounds};

use super::intermediate::{IntermediateTile, TileBuffers};

// Constants from src/osm_data/pbf_reader.rs
const RESIDENTIAL_PROXIMITY_THRESHOLD_METERS: f64 = 500.0;
const RESIDENTIAL_PART_COVERED: f64 = 0.10;
const THRESHOLD_AREA: f64 = (RESIDENTIAL_PROXIMITY_THRESHOLD_METERS
    * RESIDENTIAL_PROXIMITY_THRESHOLD_METERS
    * std::f64::consts::PI)
    * RESIDENTIAL_PART_COVERED;
const MILITARY_ENTRY_MAX_M: f64 = 100.0;

pub struct ProximityComputer {
    tile_size_degrees: f32,
    residential_areas: AreaGrid,
    military_areas: AreaGrid,
}

impl ProximityComputer {
    pub fn new(pbf_path: &Path, tile_size_degrees: f32) -> Result<Self> {
        info!("Extracting area grids from PBF");

        let file = std::fs::File::open(pbf_path)
            .context("Failed to open PBF file")?;
        let mut pbf = osmpbfreader::OsmPbfReader::new(file);

        // Extract residential areas
        let mut boundary_reader = PbfAreaReader::new(&mut pbf);
        boundary_reader.read(&|obj| {
            (obj.is_way() || obj.is_relation()) && obj.tags().contains("landuse", "residential")
        })?;
        let residential_areas = boundary_reader.get_area_grid();

        // Extract military areas
        let file = std::fs::File::open(pbf_path)
            .context("Failed to open PBF file for second pass")?;
        let mut pbf = osmpbfreader::OsmPbfReader::new(file);

        let mut boundary_reader = PbfAreaReader::new(&mut pbf);
        boundary_reader.read(&|obj| {
            (obj.is_way() || obj.is_relation()) && obj.tags().contains("landuse", "military")
        })?;
        let military_areas = boundary_reader.get_area_grid();

        info!("Area grids extracted");

        Ok(Self {
            tile_size_degrees,
            residential_areas,
            military_areas,
        })
    }

    /// Compute proximity flags for all tiles in parallel
    pub fn compute_all(&self, tile_buffers: &TileBuffers, output_dir: &Path) -> Result<()> {
        info!("Computing proximity flags for {} tiles", tile_buffers.tiles.len());

        let tile_ids: Vec<TileId> = tile_buffers.tiles.keys().cloned().collect();

        // Process tiles in parallel
        tile_ids.par_iter()
            .try_for_each(|tile_id| -> Result<()> {
                self.compute_tile(*tile_id, output_dir)
            })?;

        info!("Proximity computation complete");
        Ok(())
    }

    /// Compute flags for a single tile with 500m overlap
    fn compute_tile(&self, tile_id: TileId, output_dir: &Path) -> Result<()> {
        // Load intermediate tile
        let mut tile = IntermediateTile::load_from_disk(output_dir, tile_id)?;

        // Compute tile bounds with 500m buffer
        let core_bounds = self.compute_tile_bounds(tile_id);
        let buffered_bounds = self.add_buffer_to_bounds(core_bounds, RESIDENTIAL_PROXIMITY_THRESHOLD_METERS);

        // For each node in the core tile
        for (node_id, node) in tile.nodes.iter_mut() {
            // Only compute for nodes in core bounds (not buffer nodes)
            if !self.point_in_bounds(node.lat as f32, node.lon as f32, core_bounds) {
                continue;
            }

            // Compute residential proximity
            node.residential_in_proximity = self.compute_residential_proximity(
                node.lat,
                node.lon,
                buffered_bounds,
            );

            // Compute nogo area
            node.nogo_area = self.compute_nogo_area(
                node.lat,
                node.lon,
            );
        }

        // Save updated tile back to disk
        tile.save_to_disk(output_dir)?;

        Ok(())
    }

    fn compute_tile_bounds(&self, tile_id: TileId) -> TileBounds {
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

    fn add_buffer_to_bounds(&self, bounds: TileBounds, buffer_meters: f64) -> TileBounds {
        // Convert meters to approximate degrees
        // At equator: 1 degree ≈ 111km
        // This is approximate; for exact calculation use Haversine
        let buffer_degrees = (buffer_meters / 111000.0) as f32;

        TileBounds {
            lat_min: (bounds.lat_min - buffer_degrees).max(-90.0),
            lat_max: (bounds.lat_max + buffer_degrees).min(90.0),
            lon_min: (bounds.lon_min - buffer_degrees).max(-180.0),
            lon_max: (bounds.lon_max + buffer_degrees).min(180.0),
        }
    }

    fn point_in_bounds(&self, lat: f32, lon: f32, bounds: TileBounds) -> bool {
        lat >= bounds.lat_min && lat < bounds.lat_max &&
        lon >= bounds.lon_min && lon < bounds.lon_max
    }

    /// Compute residential proximity flag (from src/osm_data/pbf_reader.rs:90-126)
    fn compute_residential_proximity(&self, lat: f64, lon: f64, bounds: TileBounds) -> bool {
        let tot_area = match self.residential_areas.find_closest_areas_refs(
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

    /// Compute nogo area flag (from src/osm_data/pbf_reader.rs:128-149)
    fn compute_nogo_area(&self, lat: f64, lon: f64) -> bool {
        match self.military_areas.find_closest_areas_refs(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounds_computation() {
        let computer = ProximityComputer {
            tile_size_degrees: 1.0,
            residential_areas: AreaGrid::new(),
            military_areas: AreaGrid::new(),
        };

        let bounds = computer.compute_tile_bounds(TileId { col: 204, row: 146 });
        assert_eq!(bounds.lon_min, 24.0);
        assert_eq!(bounds.lon_max, 25.0);
        assert_eq!(bounds.lat_min, 56.0);
        assert_eq!(bounds.lat_max, 57.0);
    }

    #[test]
    fn test_buffer_expansion() {
        let computer = ProximityComputer {
            tile_size_degrees: 1.0,
            residential_areas: AreaGrid::new(),
            military_areas: AreaGrid::new(),
        };

        let bounds = TileBounds {
            lat_min: 56.0,
            lat_max: 57.0,
            lon_min: 24.0,
            lon_max: 25.0,
        };

        let buffered = computer.add_buffer_to_bounds(bounds, 500.0);

        // 500m ≈ 0.0045 degrees
        assert!(buffered.lat_min < 56.0);
        assert!(buffered.lat_max > 57.0);
        assert!(buffered.lon_min < 24.0);
        assert!(buffered.lon_max > 25.0);
    }
}
```

**Rationale**:
- Reuses existing proximity logic from pbf_reader.rs
- 500m buffer ensures correct computation at tile edges
- Parallel processing with rayon for performance
- Only updates nodes in core tile bounds (not buffer duplicates)

### 2. Integrate with Tile Generator

**File**: `src/rmdf/generator/mod.rs`

**Changes**: Add proximity computation phase

```rust
mod pbf_streamer;
mod intermediate;
mod proximity;  // NEW

pub use pbf_streamer::*;
pub use intermediate::*;
pub use proximity::*;  // NEW

use std::path::PathBuf;
use anyhow::Result;

pub struct TileGenerator {
    input_file: PathBuf,
    output_dir: PathBuf,
    tile_size_degrees: f32,
}

impl TileGenerator {
    // ... existing new() method

    pub fn generate(&self) -> Result<()> {
        // Phase 2: Stream and partition
        let streamer = PbfStreamer::new(&self.input_file, &self.output_dir, self.tile_size_degrees)?;
        let tile_buffers = streamer.partition()?;

        // Phase 3: Proximity computation (NEW)
        let proximity_computer = ProximityComputer::new(&self.input_file, self.tile_size_degrees)?;
        proximity_computer.compute_all(&tile_buffers, &self.output_dir)?;

        // Phase 4: RMDF writing (not implemented yet)

        Ok(())
    }
}
```

**Rationale**: Chains proximity computation after partitioning.

## Success Criteria

### Automated Verification

- [ ] Unit tests pass: `cargo test rmdf::generator::proximity`
- [ ] Bounds computation tests pass
- [ ] Buffer expansion tests pass
- [ ] Parallel computation completes without deadlocks
- [ ] Type checking passes: `cargo check`

### Manual Verification

- [ ] Generate tiles with proximity: `ridi-router generate-tiles --input test.pbf --output ./tiles`
- [ ] Inspect intermediate files - nodes have proximity flags set
- [ ] Nodes near residential areas have `residential_in_proximity = true`
- [ ] Nodes inside military areas have `nogo_area = true`
- [ ] Border nodes get correct flags (not influenced by buffer artifacts)

## Dependencies

- **Depends on**: Phase 2 (needs intermediate tile buffers)
- **Blocks**: Phase 4 (needs proximity flags for RMDF serialization)

## Risks & Mitigations

**Risk**: Buffer size calculation inaccurate at high latitudes
- **Mitigation**: Use Haversine distance for exact calculations if needed

**Risk**: PBF opened twice (residential + military)
- **Mitigation**: Acceptable for Phase 3; could optimize with single pass in future

**Risk**: Memory usage for AreaGrid
- **Mitigation**: AreaGrid is kept in memory but is small (~1GB for global coverage)

**Risk**: Parallel computation race conditions
- **Mitigation**: Each tile processed independently, no shared state except read-only AreaGrids

## Notes

- Buffer overlap strategy eliminates edge artifacts
- Only core tile nodes updated (border duplicates remain with default false)
- AreaGrid extraction matches current pbf_reader.rs behavior exactly
- Constants (500m, 100m thresholds) unchanged from current implementation
- Parallel processing should complete Montenegro in <10 seconds
