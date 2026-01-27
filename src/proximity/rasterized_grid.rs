//! Rasterized proximity grid data structure.
//!
//! Provides O(1) lookup for residential proximity and nogo area flags
//! by pre-computing values during PBF loading.

use crate::osm_data::in_memory_pbf::PbfBounds;

/// Grid cell size: ~50m (0.0005 degrees at equator)
/// This provides a good balance between memory usage and accuracy.
/// Maximum position error is ~35m (half diagonal of cell).
pub const GRID_CELL_SIZE_DEG: f32 = 0.0005;

/// Search radius for residential proximity check (meters)
pub const RESIDENTIAL_PROXIMITY_THRESHOLD_M: f32 = 500.0;

/// Minimum fraction of search circle that must be covered by residential areas
pub const RESIDENTIAL_PART_COVERED: f32 = 0.10;

/// Computed threshold area (m²) = π * r² * coverage_fraction
/// This is approximately 78,540 m²
pub const THRESHOLD_AREA_M2: f32 = std::f32::consts::PI
    * RESIDENTIAL_PROXIMITY_THRESHOLD_M
    * RESIDENTIAL_PROXIMITY_THRESHOLD_M
    * RESIDENTIAL_PART_COVERED;

/// Military interior threshold - must be >100m from boundary to be marked nogo
pub const MILITARY_INTERIOR_M: f32 = 100.0;

/// Grid cell storing pre-computed proximity values.
///
/// Each cell is 8 bytes (4 bytes f32 + 1 byte bool + 3 bytes padding).
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct GridCell {
    /// Total area of residential polygons within 500m of cell center (m²)
    pub residential_area_m2: f32,
    /// True if cell center is >100m inside a military polygon boundary
    pub is_military_interior: bool,
    /// Padding for alignment
    _padding: [u8; 3],
}

impl GridCell {
    /// Create a new empty grid cell
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if this cell indicates residential proximity
    #[inline]
    pub fn is_residential_proximity(&self) -> bool {
        self.residential_area_m2 > THRESHOLD_AREA_M2
    }

    /// Check if this cell indicates a nogo area
    #[inline]
    pub fn is_nogo_area(&self) -> bool {
        self.is_military_interior
    }
}

/// Rasterized grid for O(1) proximity flag lookups.
///
/// The grid covers the geographic bounds of the PBF file with cells
/// of size `GRID_CELL_SIZE_DEG` degrees (approximately 50m at equator).
#[derive(Debug)]
pub struct RasterizedProximityGrid {
    /// Flat array of grid cells in row-major order
    cells: Vec<GridCell>,
    /// Number of columns (longitude direction)
    cols: u32,
    /// Number of rows (latitude direction)
    rows: u32,
    /// Minimum longitude of grid
    lon_min: f32,
    /// Minimum latitude of grid
    lat_min: f32,
}

impl RasterizedProximityGrid {
    /// Create a new grid covering the given bounds.
    ///
    /// # Arguments
    ///
    /// * `bounds` - Geographic bounds from PBF file
    ///
    /// # Panics
    ///
    /// Panics if bounds are invalid or would create a grid too large for memory.
    pub fn new(bounds: &PbfBounds) -> Self {
        let lat_min = bounds.lat_min.expect("PbfBounds should have lat_min") as f32;
        let lat_max = bounds.lat_max.expect("PbfBounds should have lat_max") as f32;
        let lon_min = bounds.lon_min.expect("PbfBounds should have lon_min") as f32;
        let lon_max = bounds.lon_max.expect("PbfBounds should have lon_max") as f32;

        // Add small buffer to ensure edge points are included
        let buffer = GRID_CELL_SIZE_DEG;
        let lat_min = lat_min - buffer;
        let lat_max = lat_max + buffer;
        let lon_min = lon_min - buffer;
        let lon_max = lon_max + buffer;

        // Calculate grid dimensions
        let cols = ((lon_max - lon_min) / GRID_CELL_SIZE_DEG).ceil() as u32;
        let rows = ((lat_max - lat_min) / GRID_CELL_SIZE_DEG).ceil() as u32;

        let cell_count = (cols as u64) * (rows as u64);

        // Safety check for memory usage (each cell is 8 bytes)
        // Limit to ~4GB of grid data
        const MAX_CELLS: u64 = 500_000_000; // ~4GB
        if cell_count > MAX_CELLS {
            panic!(
                "Grid would require {} cells ({:.1} GB), exceeds maximum of {} cells. \
                Consider using a smaller region or larger cell size.",
                cell_count,
                (cell_count * 8) as f64 / 1e9,
                MAX_CELLS
            );
        }

        tracing::info!(
            "Creating proximity grid: {}x{} cells ({:.1} MB), covering {:.3}°-{:.3}° lat, {:.3}°-{:.3}° lon",
            cols,
            rows,
            (cell_count * 8) as f64 / 1e6,
            lat_min,
            lat_max,
            lon_min,
            lon_max
        );

        let cells = vec![GridCell::default(); cell_count as usize];

        Self {
            cells,
            cols,
            rows,
            lon_min,
            lat_min,
        }
    }

    /// Get the cell index for a given coordinate, or None if out of bounds.
    #[inline]
    pub fn cell_index(&self, lat: f32, lon: f32) -> Option<usize> {
        let col = ((lon - self.lon_min) / GRID_CELL_SIZE_DEG) as i32;
        let row = ((lat - self.lat_min) / GRID_CELL_SIZE_DEG) as i32;

        if col < 0 || col >= self.cols as i32 || row < 0 || row >= self.rows as i32 {
            return None;
        }

        Some((row as usize) * (self.cols as usize) + (col as usize))
    }

    /// Get the center coordinates of a cell by index.
    #[inline]
    pub fn cell_center(&self, index: usize) -> (f32, f32) {
        let row = (index / self.cols as usize) as f32;
        let col = (index % self.cols as usize) as f32;

        let lat = self.lat_min + (row + 0.5) * GRID_CELL_SIZE_DEG;
        let lon = self.lon_min + (col + 0.5) * GRID_CELL_SIZE_DEG;

        (lat, lon)
    }

    /// Get a reference to a cell at the given coordinate.
    #[inline]
    pub fn get_cell(&self, lat: f32, lon: f32) -> Option<&GridCell> {
        self.cell_index(lat, lon).map(|idx| &self.cells[idx])
    }

    /// Get a mutable reference to a cell at the given coordinate.
    #[inline]
    pub fn get_cell_mut(&mut self, lat: f32, lon: f32) -> Option<&mut GridCell> {
        self.cell_index(lat, lon).map(|idx| &mut self.cells[idx])
    }

    /// Get a mutable reference to a cell by index.
    #[inline]
    pub fn get_cell_by_index_mut(&mut self, index: usize) -> Option<&mut GridCell> {
        self.cells.get_mut(index)
    }

    /// O(1) lookup for residential proximity flag.
    #[inline]
    pub fn is_residential_proximity(&self, lat: f32, lon: f32) -> bool {
        self.get_cell(lat, lon)
            .map_or(false, |cell| cell.is_residential_proximity())
    }

    /// O(1) lookup for nogo area flag.
    #[inline]
    pub fn is_nogo_area(&self, lat: f32, lon: f32) -> bool {
        self.get_cell(lat, lon)
            .map_or(false, |cell| cell.is_nogo_area())
    }

    /// Get total number of cells in the grid.
    #[inline]
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Get grid dimensions (cols, rows).
    #[inline]
    pub fn dimensions(&self) -> (u32, u32) {
        (self.cols, self.rows)
    }

    /// Get grid bounds (lon_min, lat_min, lon_max, lat_max).
    #[inline]
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let lon_max = self.lon_min + (self.cols as f32) * GRID_CELL_SIZE_DEG;
        let lat_max = self.lat_min + (self.rows as f32) * GRID_CELL_SIZE_DEG;
        (self.lon_min, self.lat_min, lon_max, lat_max)
    }

    /// Iterate over all cell indices for parallel processing.
    #[inline]
    pub fn cell_indices(&self) -> impl Iterator<Item = usize> {
        0..self.cells.len()
    }

    /// Get a slice of all cells for parallel processing.
    #[inline]
    pub fn cells_mut(&mut self) -> &mut [GridCell] {
        &mut self.cells
    }

    /// Count cells with residential proximity flag set.
    pub fn count_residential(&self) -> usize {
        self.cells
            .iter()
            .filter(|c| c.is_residential_proximity())
            .count()
    }

    /// Count cells with nogo flag set.
    pub fn count_nogo(&self) -> usize {
        self.cells.iter().filter(|c| c.is_nogo_area()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_bounds(lat_min: f64, lat_max: f64, lon_min: f64, lon_max: f64) -> PbfBounds {
        PbfBounds {
            lat_min: Some(lat_min),
            lat_max: Some(lat_max),
            lon_min: Some(lon_min),
            lon_max: Some(lon_max),
        }
    }

    #[test]
    fn test_grid_creation() {
        let bounds = make_test_bounds(50.0, 50.1, 10.0, 10.1);
        let grid = RasterizedProximityGrid::new(&bounds);

        // 0.1 degrees / 0.0005 = 200 cells per dimension (plus buffer)
        assert!(grid.cols > 200);
        assert!(grid.rows > 200);
        assert_eq!(grid.cell_count(), (grid.cols as usize) * (grid.rows as usize));
    }

    #[test]
    fn test_cell_index_in_bounds() {
        let bounds = make_test_bounds(50.0, 50.1, 10.0, 10.1);
        let grid = RasterizedProximityGrid::new(&bounds);

        // Point in center should have valid index
        let idx = grid.cell_index(50.05, 10.05);
        assert!(idx.is_some());
    }

    #[test]
    fn test_cell_index_out_of_bounds() {
        let bounds = make_test_bounds(50.0, 50.1, 10.0, 10.1);
        let grid = RasterizedProximityGrid::new(&bounds);

        // Point far outside should return None
        assert!(grid.cell_index(60.0, 20.0).is_none());
        assert!(grid.cell_index(40.0, 5.0).is_none());
    }

    #[test]
    fn test_cell_center_calculation() {
        let bounds = make_test_bounds(50.0, 50.1, 10.0, 10.1);
        let grid = RasterizedProximityGrid::new(&bounds);

        // Get center of first cell
        let (lat, lon) = grid.cell_center(0);

        // Should be near bottom-left corner (with half cell offset)
        assert!(lat < 50.01);
        assert!(lon < 10.01);
    }

    #[test]
    fn test_get_cell_mut() {
        let bounds = make_test_bounds(50.0, 50.1, 10.0, 10.1);
        let mut grid = RasterizedProximityGrid::new(&bounds);

        // Modify a cell
        if let Some(cell) = grid.get_cell_mut(50.05, 10.05) {
            cell.residential_area_m2 = 100000.0; // Above threshold
            cell.is_military_interior = true;
        }

        // Verify flags
        assert!(grid.is_residential_proximity(50.05, 10.05));
        assert!(grid.is_nogo_area(50.05, 10.05));
    }

    #[test]
    fn test_flag_defaults() {
        let bounds = make_test_bounds(50.0, 50.1, 10.0, 10.1);
        let grid = RasterizedProximityGrid::new(&bounds);

        // Default should be no flags set
        assert!(!grid.is_residential_proximity(50.05, 10.05));
        assert!(!grid.is_nogo_area(50.05, 10.05));
    }

    #[test]
    fn test_residential_threshold() {
        let mut cell = GridCell::new();

        // Below threshold
        cell.residential_area_m2 = THRESHOLD_AREA_M2 - 1.0;
        assert!(!cell.is_residential_proximity());

        // Above threshold
        cell.residential_area_m2 = THRESHOLD_AREA_M2 + 1.0;
        assert!(cell.is_residential_proximity());
    }

    #[test]
    fn test_count_methods() {
        let bounds = make_test_bounds(50.0, 50.01, 10.0, 10.01); // Small grid
        let mut grid = RasterizedProximityGrid::new(&bounds);

        // Initially all zeros
        assert_eq!(grid.count_residential(), 0);
        assert_eq!(grid.count_nogo(), 0);

        // Set some cells
        let cells = grid.cells_mut();
        if cells.len() >= 3 {
            cells[0].residential_area_m2 = 100000.0;
            cells[1].residential_area_m2 = 100000.0;
            cells[2].is_military_interior = true;
        }

        assert_eq!(grid.count_residential(), 2);
        assert_eq!(grid.count_nogo(), 1);
    }

    #[test]
    fn test_bounds_method() {
        let bounds = make_test_bounds(50.0, 50.1, 10.0, 10.1);
        let grid = RasterizedProximityGrid::new(&bounds);

        let (lon_min, lat_min, lon_max, lat_max) = grid.bounds();

        // Should cover original bounds plus buffer
        assert!(lat_min <= 50.0);
        assert!(lat_max >= 50.1);
        assert!(lon_min <= 10.0);
        assert!(lon_max >= 10.1);
    }
}
