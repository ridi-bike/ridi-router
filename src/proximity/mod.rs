//! Pre-computed proximity grid for efficient flag lookups.
//!
//! This module provides a rasterized grid-based approach for computing proximity
//! flags (residential proximity, military/nogo areas) during PBF loading rather
//! than per-tile during generation.
//!
//! Key components:
//! - `RasterizedProximityGrid`: O(1) lookup grid for proximity flags
//! - `AreaRasterizer`: Converts polygons to grid cell values using SIMD
//! - `flag_computer`: Orchestrates the flag computation pipeline

pub mod area_rasterizer;
pub mod flag_computer;
pub mod rasterized_grid;

pub use flag_computer::{compute_proximity_flags, compute_proximity_flags_with_grid, apply_grid_to_nodes};
pub use rasterized_grid::{RasterizedProximityGrid, GRID_CELL_SIZE_DEG};
