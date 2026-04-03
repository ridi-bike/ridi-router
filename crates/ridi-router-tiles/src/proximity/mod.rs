pub mod area_rasterizer;
pub mod flag_computer;
pub mod rasterized_grid;

pub use flag_computer::{
    apply_grid_to_nodes, compute_proximity_flags, compute_proximity_flags_with_grid,
};
pub use rasterized_grid::{MilitaryStatus, RasterizedProximityGrid, GRID_CELL_SIZE_DEG};
