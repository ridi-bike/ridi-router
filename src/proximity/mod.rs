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

pub use area_rasterizer::AreaRasterizer;
pub use flag_computer::compute_proximity_flags;
pub use rasterized_grid::{GridCell, RasterizedProximityGrid};
