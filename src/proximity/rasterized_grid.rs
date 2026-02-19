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

/// Grid cell storing pre-computed proximity values with directional sectors.
///
/// Each cell stores residential area in 8 directional sectors (N, NE, E, SE, S, SW, W, NW)
/// to enable correct merging of overlapping grids from multiple PBF files.
///
/// Sector layout:
/// ```text
///         N (0)
/// NW (7)  NE (1)
/// W (6)   •   E (2)
/// SW (5)  SE (3)
///         S (4)
/// ```
///
/// Each sector spans 45°:
/// - N:  337.5° - 22.5°
/// - NE: 22.5° - 67.5°
/// - E:  67.5° - 112.5°
/// - SE: 112.5° - 157.5°
/// - S:  157.5° - 202.5°
/// - SW: 202.5° - 247.5°
/// - W:  247.5° - 292.5°
/// - NW: 292.5° - 337.5°
///
/// Total size: 36 bytes (8 × 4 bytes for sectors + 1 byte bool + 3 bytes padding)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GridCell {
    /// Residential area (m²) in each directional sector within 500m
    /// Index: 0=N, 1=NE, 2=E, 3=SE, 4=S, 5=SW, 6=W, 7=NW
    pub residential_sectors: [f32; 8],
    /// True if cell center is >100m inside a military polygon boundary
    pub is_military_interior: bool,
    /// Padding for alignment
    _padding: [u8; 3],
}

impl Default for GridCell {
    fn default() -> Self {
        Self::new()
    }
}

impl GridCell {
    /// Create a new empty grid cell
    #[inline]
    pub fn new() -> Self {
        Self {
            residential_sectors: [0.0; 8],
            is_military_interior: false,
            _padding: [0; 3],
        }
    }

    /// Check if this cell indicates residential proximity
    /// Sums all sectors and compares to threshold
    #[inline]
    pub fn is_residential_proximity(&self) -> bool {
        let total: f32 = self.residential_sectors.iter().sum();
        total > THRESHOLD_AREA_M2
    }

    /// Check if this cell indicates a nogo area
    #[inline]
    pub fn is_nogo_area(&self) -> bool {
        self.is_military_interior
    }

    /// Merge another cell into this one (for combining overlapping grids)
    ///
    /// Uses MAX for sectors to avoid double-counting when overlapping PBFs
    /// contain the same polygons, and OR for military flag.
    pub fn merge(&mut self, other: &GridCell) {
        for i in 0..8 {
            self.residential_sectors[i] = self.residential_sectors[i].max(other.residential_sectors[i]);
        }
        self.is_military_interior |= other.is_military_interior;
    }

    /// Add residential area to a specific sector
    #[inline]
    pub fn add_to_sector(&mut self, sector_idx: usize, area: f32) {
        if sector_idx < 8 {
            self.residential_sectors[sector_idx] += area;
        }
    }

    /// Get total residential area across all sectors
    #[inline]
    pub fn total_residential_area(&self) -> f32 {
        self.residential_sectors.iter().sum()
    }
}

/// Convert a bearing angle (degrees) to sector index (0-7)
///
/// Bearing 0° = North, 90° = East, 180° = South, 270° = West
/// Sectors are centered on cardinal/ordinal directions:
/// - N (0): 337.5° to 22.5°
/// - NE (1): 22.5° to 67.5°
/// - etc.
#[inline]
pub fn bearing_to_sector(bearing_deg: f32) -> usize {
    // Normalize bearing to 0-360
    let b = ((bearing_deg % 360.0) + 360.0) % 360.0;
    // Add 22.5 to center sectors on cardinal directions, then divide by 45
    (((b + 22.5) / 45.0) as usize) % 8
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

        // Safety check for memory usage (each cell is 36 bytes)
        // Limit to ~4GB of grid data
        const MAX_CELLS: u64 = 110_000_000; // ~4GB at 36 bytes/cell
        if cell_count > MAX_CELLS {
            panic!(
                "Grid would require {} cells ({:.1} GB), exceeds maximum of {} cells. \
                Consider using a smaller region or larger cell size.",
                cell_count,
                (cell_count * 36) as f64 / 1e9,
                MAX_CELLS
            );
        }

        tracing::info!(
            "Creating proximity grid: {}x{} cells ({:.1} MB), covering {:.3}°-{:.3}° lat, {:.3}°-{:.3}° lon",
            cols,
            rows,
            (cell_count * 36) as f64 / 1e6,
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

    /// Get a slice of all cells (read-only)
    #[inline]
    pub fn cells(&self) -> &[GridCell] {
        &self.cells
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

    /// Serialize grid to bytes for storage
    ///
    /// Format:
    /// - Header (16 bytes): cols (u32), rows (u32), lon_min (f32), lat_min (f32)
    /// - Cells (36 bytes each): 8 × f32 sectors + 1 byte bool + 3 bytes padding
    pub fn to_bytes(&self) -> Vec<u8> {
        let header_size = 16;
        let cell_size = 36;
        let mut bytes = Vec::with_capacity(header_size + self.cells.len() * cell_size);

        // Header
        bytes.extend_from_slice(&self.cols.to_le_bytes());
        bytes.extend_from_slice(&self.rows.to_le_bytes());
        bytes.extend_from_slice(&self.lon_min.to_le_bytes());
        bytes.extend_from_slice(&self.lat_min.to_le_bytes());

        // Cells
        for cell in &self.cells {
            for sector in &cell.residential_sectors {
                bytes.extend_from_slice(&sector.to_le_bytes());
            }
            bytes.push(cell.is_military_interior as u8);
            bytes.extend_from_slice(&[0, 0, 0]); // padding
        }

        bytes
    }

    /// Deserialize grid from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < 16 {
            return Err("Buffer too small for header");
        }

        // Parse header
        let cols = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let rows = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let lon_min = f32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let lat_min = f32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);

        let cell_count = (cols as u64) * (rows as u64);
        let expected_size = 16 + (cell_count as usize) * 36;
        if bytes.len() < expected_size {
            return Err("Buffer too small for cells");
        }

        // Parse cells
        let mut cells = Vec::with_capacity(cell_count as usize);
        let mut offset = 16;

        for _ in 0..cell_count {
            let mut residential_sectors = [0.0f32; 8];
            for i in 0..8 {
                residential_sectors[i] = f32::from_le_bytes([
                    bytes[offset + i * 4],
                    bytes[offset + i * 4 + 1],
                    bytes[offset + i * 4 + 2],
                    bytes[offset + i * 4 + 3],
                ]);
            }
            offset += 32;

            let is_military_interior = bytes[offset] != 0;
            offset += 4; // 1 byte bool + 3 bytes padding

            cells.push(GridCell {
                residential_sectors,
                is_military_interior,
                _padding: [0; 3],
            });
        }

        Ok(Self {
            cells,
            cols,
            rows,
            lon_min,
            lat_min,
        })
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

        // Modify a cell using sector API
        if let Some(cell) = grid.get_cell_mut(50.05, 10.05) {
            cell.residential_sectors[0] = 100000.0; // North sector above threshold
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

        // Below threshold in single sector
        cell.residential_sectors[0] = THRESHOLD_AREA_M2 - 1.0;
        assert!(!cell.is_residential_proximity());

        // Above threshold in single sector
        cell.residential_sectors[0] = THRESHOLD_AREA_M2 + 1.0;
        assert!(cell.is_residential_proximity());

        // Reset and test with multiple sectors
        cell.residential_sectors = [0.0; 8];
        cell.residential_sectors[0] = THRESHOLD_AREA_M2 / 2.0;
        cell.residential_sectors[1] = THRESHOLD_AREA_M2 / 2.0 + 1.0;
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
            cells[0].residential_sectors[0] = 100000.0;
            cells[1].residential_sectors[0] = 100000.0;
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

    #[test]
    fn test_bearing_to_sector() {
        // Test cardinal directions
        assert_eq!(bearing_to_sector(0.0), 0);   // N
        assert_eq!(bearing_to_sector(45.0), 1);  // NE
        assert_eq!(bearing_to_sector(90.0), 2);  // E
        assert_eq!(bearing_to_sector(135.0), 3); // SE
        assert_eq!(bearing_to_sector(180.0), 4); // S
        assert_eq!(bearing_to_sector(225.0), 5); // SW
        assert_eq!(bearing_to_sector(270.0), 6); // W
        assert_eq!(bearing_to_sector(315.0), 7); // NW

        // Test boundary cases
        assert_eq!(bearing_to_sector(22.4), 0);  // Just before N/NE boundary
        assert_eq!(bearing_to_sector(22.6), 1);  // Just after N/NE boundary
        assert_eq!(bearing_to_sector(337.4), 7); // Just before NW/N boundary
        assert_eq!(bearing_to_sector(337.6), 0); // Just after NW/N boundary

        // Test negative and >360 values normalize correctly
        assert_eq!(bearing_to_sector(-45.0), 7); // -45° = 315° = NW
        assert_eq!(bearing_to_sector(405.0), 1); // 405° = 45° = NE
    }

    #[test]
    fn test_cell_merge() {
        let mut cell1 = GridCell::new();
        let mut cell2 = GridCell::new();

        // Set different sectors
        cell1.residential_sectors[0] = 1000.0; // N
        cell1.residential_sectors[1] = 500.0;  // NE
        cell1.is_military_interior = true;

        cell2.residential_sectors[1] = 800.0;  // NE (larger)
        cell2.residential_sectors[2] = 600.0;  // E
        cell2.is_military_interior = false;

        cell1.merge(&cell2);

        // N should stay same
        assert_eq!(cell1.residential_sectors[0], 1000.0);
        // NE should take max
        assert_eq!(cell1.residential_sectors[1], 800.0);
        // E should be from cell2
        assert_eq!(cell1.residential_sectors[2], 600.0);
        // Military should be OR'd
        assert!(cell1.is_military_interior);
    }

    #[test]
    fn test_grid_serialization() {
        let bounds = make_test_bounds(50.0, 50.01, 10.0, 10.01);
        let mut grid = RasterizedProximityGrid::new(&bounds);

        // Set some test data
        if let Some(cell) = grid.get_cell_mut(50.005, 10.005) {
            cell.residential_sectors[0] = 50000.0;
            cell.residential_sectors[2] = 30000.0;
            cell.is_military_interior = true;
        }

        // Serialize
        let bytes = grid.to_bytes();

        // Deserialize
        let restored = RasterizedProximityGrid::from_bytes(&bytes).expect("Deserialization should succeed");

        // Verify dimensions match
        assert_eq!(grid.cols, restored.cols);
        assert_eq!(grid.rows, restored.rows);
        assert_eq!(grid.lon_min, restored.lon_min);
        assert_eq!(grid.lat_min, restored.lat_min);

        // Verify cell data matches
        if let (Some(original), Some(restored_cell)) = (
            grid.get_cell(50.005, 10.005),
            restored.get_cell(50.005, 10.005),
        ) {
            assert_eq!(original.residential_sectors, restored_cell.residential_sectors);
            assert_eq!(original.is_military_interior, restored_cell.is_military_interior);
        }
    }

    #[test]
    fn test_gridcell_add_to_sector() {
        let mut cell = GridCell::new();
        
        // Add to valid sectors
        cell.add_to_sector(0, 100.0);
        assert_eq!(cell.residential_sectors[0], 100.0);
        
        cell.add_to_sector(0, 50.0); // Add more to same sector
        assert_eq!(cell.residential_sectors[0], 150.0);
        
        cell.add_to_sector(7, 200.0);
        assert_eq!(cell.residential_sectors[7], 200.0);
        
        // Invalid sector index should be ignored
        cell.add_to_sector(8, 999.0);
        cell.add_to_sector(100, 999.0);
        assert_eq!(cell.residential_sectors[0], 150.0); // Unchanged
    }

    #[test]
    fn test_gridcell_total_residential_area() {
        let mut cell = GridCell::new();
        assert_eq!(cell.total_residential_area(), 0.0);
        
        cell.residential_sectors[0] = 100.0;
        cell.residential_sectors[2] = 200.0;
        cell.residential_sectors[5] = 300.0;
        
        assert_eq!(cell.total_residential_area(), 600.0);
    }

    #[test]
    fn test_gridcell_new_vs_default() {
        let cell_new = GridCell::new();
        let cell_default = GridCell::default();
        
        assert_eq!(cell_new.residential_sectors, cell_default.residential_sectors);
        assert_eq!(cell_new.is_military_interior, cell_default.is_military_interior);
        
        // Both should have all zeros
        assert_eq!(cell_new.residential_sectors, [0.0f32; 8]);
        assert!(!cell_new.is_military_interior);
    }

    #[test]
    fn test_grid_serialization_buffer_too_small() {
        // Empty buffer
        let result = RasterizedProximityGrid::from_bytes(&[]);
        assert!(result.is_err());
        
        // Buffer with only partial header
        let result = RasterizedProximityGrid::from_bytes(&[0, 0, 0]);
        assert!(result.is_err());
        
        // Buffer with header but no cells
        let bounds = make_test_bounds(50.0, 50.01, 10.0, 10.01);
        let grid = RasterizedProximityGrid::new(&bounds);
        let bytes = grid.to_bytes();
        
        // Truncate to just header
        let result = RasterizedProximityGrid::from_bytes(&bytes[..16]);
        assert!(result.is_err());
    }

    #[test]
    fn test_grid_serialization_empty_grid() {
        let bounds = make_test_bounds(50.0, 50.001, 10.0, 10.001);
        let grid = RasterizedProximityGrid::new(&bounds);
        
        let bytes = grid.to_bytes();
        let restored = RasterizedProximityGrid::from_bytes(&bytes).expect("Should deserialize");
        
        // All cells should be default
        for cell in restored.cells().iter() {
            assert_eq!(cell.residential_sectors, [0.0f32; 8]);
            assert!(!cell.is_military_interior);
        }
    }

    #[test]
    fn test_bearing_to_sector_all_ordinals() {
        // Test ordinal directions (NE, SE, SW, NW)
        assert_eq!(bearing_to_sector(45.0), 1);   // NE
        assert_eq!(bearing_to_sector(135.0), 3);  // SE
        assert_eq!(bearing_to_sector(225.0), 5);  // SW
        assert_eq!(bearing_to_sector(315.0), 7);  // NW
        
        // Test boundaries between all sectors
        // N/NE boundary at 22.5
        assert_eq!(bearing_to_sector(22.49), 0);
        assert_eq!(bearing_to_sector(22.51), 1);
        
        // NE/E boundary at 67.5
        assert_eq!(bearing_to_sector(67.49), 1);
        assert_eq!(bearing_to_sector(67.51), 2);
        
        // E/SE boundary at 112.5
        assert_eq!(bearing_to_sector(112.49), 2);
        assert_eq!(bearing_to_sector(112.51), 3);
        
        // SE/S boundary at 157.5
        assert_eq!(bearing_to_sector(157.49), 3);
        assert_eq!(bearing_to_sector(157.51), 4);
        
        // S/SW boundary at 202.5
        assert_eq!(bearing_to_sector(202.49), 4);
        assert_eq!(bearing_to_sector(202.51), 5);
        
        // SW/W boundary at 247.5
        assert_eq!(bearing_to_sector(247.49), 5);
        assert_eq!(bearing_to_sector(247.51), 6);
        
        // W/NW boundary at 292.5
        assert_eq!(bearing_to_sector(292.49), 6);
        assert_eq!(bearing_to_sector(292.51), 7);
        
        // NW/N boundary at 337.5
        assert_eq!(bearing_to_sector(337.49), 7);
        assert_eq!(bearing_to_sector(337.51), 0);
    }

    #[test]
    fn test_bearing_to_sector_large_values() {
        // Values > 720 degrees
        assert_eq!(bearing_to_sector(720.0), 0);   // 720 = 2 * 360 = 0 = N
        assert_eq!(bearing_to_sector(765.0), 1);   // 765 = 720 + 45 = 45 = NE
        assert_eq!(bearing_to_sector(900.0), 2);   // 900 = 2.5 * 360 = 180 = S... wait
        // Actually 900 % 360 = 180, which is S (sector 4)
        assert_eq!(bearing_to_sector(900.0), 4);
        
        // Very negative values
        assert_eq!(bearing_to_sector(-360.0), 0);  // -360 = 0 = N
        assert_eq!(bearing_to_sector(-405.0), 7);  // -405 = -360 - 45 = -45 -> 315 = NW
    }

    #[test]
    fn test_cell_merge_multiple_times() {
        let mut cell = GridCell::new();
        
        // Merge cell with data in sector 0
        let mut other1 = GridCell::new();
        other1.residential_sectors[0] = 100.0;
        cell.merge(&other1);
        assert_eq!(cell.residential_sectors[0], 100.0);
        
        // Merge again with larger value
        let mut other2 = GridCell::new();
        other2.residential_sectors[0] = 200.0;
        cell.merge(&other2);
        assert_eq!(cell.residential_sectors[0], 200.0); // MAX, not sum
        
        // Merge again with smaller value
        let mut other3 = GridCell::new();
        other3.residential_sectors[0] = 50.0;
        cell.merge(&other3);
        assert_eq!(cell.residential_sectors[0], 200.0); // Still 200
    }

    #[test]
    fn test_cell_merge_military_accumulation() {
        let mut cell = GridCell::new();
        
        // Start with military false
        assert!(!cell.is_military_interior);
        
        // Merge with military true
        let mut other1 = GridCell::new();
        other1.is_military_interior = true;
        cell.merge(&other1);
        assert!(cell.is_military_interior);
        
        // Merge with military false - should stay true
        let mut other2 = GridCell::new();
        other2.is_military_interior = false;
        cell.merge(&other2);
        assert!(cell.is_military_interior);
    }
}
