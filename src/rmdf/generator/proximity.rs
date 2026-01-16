use anyhow::{Context, Result};
use geo::{CoordsIter, Distance, GeodesicArea, Haversine, HaversineClosestPoint, Point};
use rayon::prelude::*;
use redb::Database;
use std::path::Path;
use tracing::info;

use crate::map_data::proximity::AreaGrid;
use crate::osm_data::pbf_area_reader::PbfAreaReader;
use crate::rmdf::format::{TileBounds, TileId};

// Constants from src/osm_data/pbf_reader.rs
const RESIDENTIAL_PROXIMITY_THRESHOLD_METERS: f64 = 500.0;
const RESIDENTIAL_PART_COVERED: f64 = 0.10;
const THRESHOLD_AREA: f64 = (RESIDENTIAL_PROXIMITY_THRESHOLD_METERS
    * RESIDENTIAL_PROXIMITY_THRESHOLD_METERS
    * std::f64::consts::PI)
    * RESIDENTIAL_PART_COVERED;
const MILITARY_ENTRY_MAX_M: f64 = 100.0;

/// DEPRECATED: Proximity computation now happens in PbfStreamer::compute_proximity_parallel()
///
/// This struct is kept for reference and potential rollback, but is no longer used in the
/// tile generation pipeline. The new implementation computes proximity flags for all nodes
/// in parallel BEFORE partitioning them into tiles, eliminating database I/O overhead and
/// restoring the original parallel processing pattern.
#[deprecated(
    note = "Proximity computation now happens in PbfStreamer::compute_proximity_parallel()"
)]
pub struct ProximityComputer {
    tile_size_degrees: f32,
    residential_areas: AreaGrid,
    military_areas: AreaGrid,
}

impl ProximityComputer {
    pub fn new(pbf_path: &Path, tile_size_degrees: f32) -> Result<Self> {
        info!("Extracting area grids from PBF");

        let file = std::fs::File::open(pbf_path).context("Failed to open PBF file")?;
        let mut pbf = osmpbfreader::OsmPbfReader::new(file);

        // Extract residential areas
        let mut boundary_reader = PbfAreaReader::new(&mut pbf);
        boundary_reader.read(&|obj| {
            (obj.is_way() || obj.is_relation()) && obj.tags().contains("landuse", "residential")
        })?;
        let residential_areas = boundary_reader.get_area_grid();

        // Extract military areas
        let file =
            std::fs::File::open(pbf_path).context("Failed to open PBF file for second pass")?;
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
    pub fn compute_all(&self, tile_ids: &[TileId], tiles_db: &Database) -> Result<()> {
        info!("Computing proximity flags for {} tiles", tile_ids.len());

        // Process tiles in parallel
        tile_ids
            .par_iter()
            .try_for_each(|tile_id| -> Result<()> { self.compute_tile(*tile_id, tiles_db) })?;

        info!("Proximity computation complete");
        Ok(())
    }

    /// Compute flags for a single tile with 500m overlap
    #[allow(dead_code)]
    fn compute_tile(&self, _tile_id: TileId, _tiles_db: &Database) -> Result<()> {
        // This method is deprecated and no longer functional
        // Proximity computation now happens in PbfStreamer::partition_parallel()
        anyhow::bail!("This method is deprecated - use PbfStreamer::partition_parallel() instead")
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
        lat >= bounds.lat_min
            && lat < bounds.lat_max
            && lon >= bounds.lon_min
            && lon < bounds.lon_max
    }

    /// Compute residential proximity flag (from src/osm_data/pbf_reader.rs:90-126)
    fn compute_residential_proximity(&self, lat: f64, lon: f64, _bounds: TileBounds) -> bool {
        let tot_area = match self.residential_areas.find_closest_areas_refs(
            lat as f32, lon as f32, 1, // Search 1 grid step (~1.1km)
        ) {
            Some(areas) => areas.iter().fold(0., |tot, multi_polygon| {
                let geo_point = Point::new(lon, lat);
                let distance = match multi_polygon.haversine_closest_point(&geo_point) {
                    geo::Closest::Intersection(_) => 0.,
                    geo::Closest::SinglePoint(p) => Haversine.distance(p, geo_point),
                    geo::Closest::Indeterminate => {
                        multi_polygon.coords_iter().fold(10000., |min, coords| {
                            let dist = Haversine.distance(geo_point, Point::from(coords));
                            if dist < min {
                                dist
                            } else {
                                min
                            }
                        })
                    }
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
        match self
            .military_areas
            .find_closest_areas_refs(lat as f32, lon as f32, 1)
        {
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
