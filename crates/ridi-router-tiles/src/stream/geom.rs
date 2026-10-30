//! Tile / region / proximity-grid geometry, reproducing the reference f32 formulas exactly.

use smallvec_lite::SmallTiles;

/// Buffer added around tile bounds when selecting nodes (MultiPbfGenerator::add_buffer_to_bounds).
const TILE_BUFFER_DEG: f32 = 0.005;
/// Distance from a tile edge beyond which a coordinate is certainly in exactly one tile column
/// (row): the 0.005 deg buffer plus generous slack for the f32 tile-bound rounding (~1e-5 deg).
const FAST_SLACK: f64 = 0.0052;

#[derive(Clone, Copy, Debug)]
pub struct TileGrid {
    pub tile_size: f32,
    /// Tiles per region side.
    pub region_tiles: u32,
    pub region_cols: u32,
    pub region_rows: u32,
    max_col: i64,
    max_row: i64,
    reach: i64,
}

impl TileGrid {
    pub fn new(tile_size: f32, region_tiles: u32) -> Self {
        // Valid tiles: cols 0..ceil(360/ts), rows 0..ceil(180/ts) (exclusive), as the reference
        // `calculate_tiles_from_bounds`; the last col/row's bounds are clamped to 180/90
        // (inclusive), so nodes at exactly +180 belong to col 3599 only. Jobs' core boxes never
        // extend past these, so the vendored local stage never writes col 3600 / row 1800.
        let max_col = (360.0 / tile_size as f64).ceil() as i64;
        let max_row = (180.0 / tile_size as f64).ceil() as i64;
        let region_cols = (max_col as u32).div_ceil(region_tiles);
        let region_rows = (max_row as u32).div_ceil(region_tiles);
        let reach = (0.005 / tile_size as f64).ceil() as i64 + 1;
        Self {
            tile_size,
            region_tiles,
            region_cols,
            region_rows,
            max_col,
            max_row,
            reach,
        }
    }

    /// Buffered bounds (lo, hi) along one axis for tile index `i`, exactly as the reference
    /// computes them (f32 maths, then compared as f64).
    #[inline]
    fn lon_bounds(&self, col: i64) -> (f64, f64) {
        let lon_min = (col as u16 as f32 * self.tile_size) - 180.0;
        let lon_max = lon_min + self.tile_size;
        (
            (lon_min - TILE_BUFFER_DEG).max(-180.0) as f64,
            (lon_max + TILE_BUFFER_DEG).min(180.0) as f64,
        )
    }

    #[inline]
    fn lat_bounds(&self, row: i64) -> (f64, f64) {
        let lat_min = (row as u16 as f32 * self.tile_size) - 90.0;
        let lat_max = lat_min + self.tile_size;
        (
            (lat_min - TILE_BUFFER_DEG).max(-90.0) as f64,
            (lat_max + TILE_BUFFER_DEG).min(90.0) as f64,
        )
    }

    /// Tile columns whose buffered bounds contain `lon` (inclusive).
    #[inline]
    pub fn cols_for(&self, lon: f64, out: &mut SmallTiles) {
        out.clear();
        let x = (lon + 180.0) / self.tile_size as f64;
        let c0 = x.floor() as i64;
        // fast path: far (> buffer + f32 rounding slack) from both tile edges -> one column
        let d = (x - c0 as f64) * self.tile_size as f64;
        if d > FAST_SLACK && self.tile_size as f64 - d > FAST_SLACK && c0 >= 0 && c0 < self.max_col {
            out.push(c0 as u32);
            return;
        }
        for c in (c0 - self.reach)..=(c0 + self.reach) {
            if c < 0 || c >= self.max_col {
                continue;
            }
            let (lo, hi) = self.lon_bounds(c);
            if lo <= lon && lon <= hi {
                out.push(c as u32);
            }
        }
    }

    #[inline]
    pub fn rows_for(&self, lat: f64, out: &mut SmallTiles) {
        out.clear();
        let y = (lat + 90.0) / self.tile_size as f64;
        let r0 = y.floor() as i64;
        let d = (y - r0 as f64) * self.tile_size as f64;
        if d > FAST_SLACK && self.tile_size as f64 - d > FAST_SLACK && r0 >= 0 && r0 < self.max_row {
            out.push(r0 as u32);
            return;
        }
        for r in (r0 - self.reach)..=(r0 + self.reach) {
            if r < 0 || r >= self.max_row {
                continue;
            }
            let (lo, hi) = self.lat_bounds(r);
            if lo <= lat && lat <= hi {
                out.push(r as u32);
            }
        }
    }

    pub fn max_col(&self) -> i64 {
        self.max_col
    }

    pub fn max_row(&self) -> i64 {
        self.max_row
    }

    #[inline]
    pub fn region_of_tile(&self, col: u32, row: u32) -> u32 {
        (row / self.region_tiles) * self.region_cols + col / self.region_tiles
    }


    pub fn all_regions(&self) -> impl Iterator<Item = u32> {
        0..(self.region_cols * self.region_rows)
    }

    /// Regions whose tiles could need a polygon with this (f32) bbox: any region whose area,
    /// grown by `margin` degrees, intersects the bbox.
    pub fn regions_for_bbox(
        &self,
        lat_min: f32,
        lat_max: f32,
        lon_min: f32,
        lon_max: f32,
        margin: f64,
        out: &mut Vec<u32>,
    ) {
        out.clear();
        let rdeg = self.region_tiles as f64 * self.tile_size as f64;
        let c0 = (((lon_min as f64 - margin) + 180.0) / rdeg).floor().max(0.0) as i64;
        let c1 = (((lon_max as f64 + margin) + 180.0) / rdeg)
            .floor()
            .min(self.region_cols as f64 - 1.0) as i64;
        let r0 = (((lat_min as f64 - margin) + 90.0) / rdeg).floor().max(0.0) as i64;
        let r1 = (((lat_max as f64 + margin) + 90.0) / rdeg)
            .floor()
            .min(self.region_rows as f64 - 1.0) as i64;
        for r in r0..=r1 {
            for c in c0..=c1 {
                out.push(r as u32 * self.region_cols + c as u32);
            }
        }
    }
}

/// Tiny fixed-capacity vector for tile indices along one axis.
pub mod smallvec_lite {
    #[derive(Default)]
    pub struct SmallTiles {
        buf: [u32; 8],
        len: usize,
    }

    impl SmallTiles {
        #[inline]
        pub fn clear(&mut self) {
            self.len = 0;
        }
        #[inline]
        pub fn push(&mut self, v: u32) {
            if self.len < self.buf.len() {
                self.buf[self.len] = v;
                self.len += 1;
            }
        }
        #[inline]
        pub fn as_slice(&self) -> &[u32] {
            &self.buf[..self.len]
        }
    }
}
