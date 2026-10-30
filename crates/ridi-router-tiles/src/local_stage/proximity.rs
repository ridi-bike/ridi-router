//! Sparse, exact proximity flags.
//!
//! Only cells that contain routable nodes are evaluated. Cell geometry and the per-cell maths
//! are identical to `RasterizedProximityGrid` / `AreaRasterizer` (f32 cell centres from the
//! same origin, same f32 prefilter boxes, same polygon evaluation and f32 summation order).
//!
//! Speed-ups that keep the result bit-identical:
//! * candidate polygons come from an R-tree over the exact prefilter boxes, queried once per
//!   run of neighbouring cells in a row and then filtered per cell with the exact prefilter;
//! * "is the polygon within 500 m" is decided with exact shortcuts (see `within_threshold`)
//!   instead of always computing the full haversine closest point;
//! * the residential sum stops as soon as it exceeds the threshold (all terms are >= 0 and f32
//!   rounding is monotone, so the final sum cannot drop back below it).

use geo::{
    Contains, Coord, Distance, GeodesicArea, Haversine, HaversineClosestPoint, Line, MultiPolygon,
    Point,
};
use rayon::prelude::*;

#[cfg(test)]
mod tests;
use rstar::{RTree, RTreeObject, AABB};
use tracing::info;

use crate::proximity::area_rasterizer::{
    calculate_bearing, compute_multipolygon_bbox, compute_multipolygon_centroid,
    is_military_interior, polygon_distance_m, BBox,
};
use crate::proximity::rasterized_grid::{
    bearing_to_sector, GRID_CELL_SIZE_DEG, MILITARY_GRID_CELL_SIZE_DEG,
    RESIDENTIAL_PROXIMITY_THRESHOLD_M, THRESHOLD_AREA_M2,
};

/// Grid origin and extent (extent only enforced in reference mode).
#[derive(Clone, Copy, Debug)]
pub struct GridAnchor {
    pub lon_min: f32,
    pub lat_min: f32,
    /// (cols, rows, military_cols, military_rows) if bounded
    pub limits: Option<(u32, u32, u32, u32)>,
}

impl GridAnchor {
    /// Same origin/extent as `RasterizedProximityGrid::try_new` for the given all-node bounds
    /// (degrees, f64 as read from the PBF).
    pub fn reference(lat_min: f64, lat_max: f64, lon_min: f64, lon_max: f64) -> Self {
        let buffer = GRID_CELL_SIZE_DEG;
        let lat_min = lat_min as f32 - buffer;
        let lat_max = lat_max as f32 + buffer;
        let lon_min = lon_min as f32 - buffer;
        let lon_max = lon_max as f32 + buffer;
        let cols = ((lon_max - lon_min) / GRID_CELL_SIZE_DEG).ceil() as u32;
        let rows = ((lat_max - lat_min) / GRID_CELL_SIZE_DEG).ceil() as u32;
        let military_cols = ((lon_max - lon_min) / MILITARY_GRID_CELL_SIZE_DEG).ceil() as u32;
        let military_rows = ((lat_max - lat_min) / MILITARY_GRID_CELL_SIZE_DEG).ceil() as u32;
        Self {
            lon_min,
            lat_min,
            limits: Some((cols, rows, military_cols, military_rows)),
        }
    }

    /// Planet-wide grid with origin (-180, -90); identical for every region.
    pub fn global() -> Self {
        Self {
            lon_min: -180.0,
            lat_min: -90.0,
            limits: None,
        }
    }

    /// Residential cell key (row << 32 | col), as `RasterizedProximityGrid::cell_index`.
    #[inline]
    pub fn residential_cell(&self, lat: f32, lon: f32) -> Option<u64> {
        let col = ((lon - self.lon_min) / GRID_CELL_SIZE_DEG) as i32;
        let row = ((lat - self.lat_min) / GRID_CELL_SIZE_DEG) as i32;
        if col < 0 || row < 0 {
            return None;
        }
        if let Some((cols, rows, _, _)) = self.limits {
            if col >= cols as i32 || row >= rows as i32 {
                return None;
            }
        }
        Some(((row as u64) << 32) | col as u64)
    }

    #[inline]
    pub fn military_cell(&self, lat: f32, lon: f32) -> Option<u64> {
        let col = ((lon - self.lon_min) / MILITARY_GRID_CELL_SIZE_DEG) as i32;
        let row = ((lat - self.lat_min) / MILITARY_GRID_CELL_SIZE_DEG) as i32;
        if col < 0 || row < 0 {
            return None;
        }
        if let Some((_, _, cols, rows)) = self.limits {
            if col >= cols as i32 || row >= rows as i32 {
                return None;
            }
        }
        Some(((row as u64) << 32) | col as u64)
    }

    #[inline]
    fn residential_center(&self, key: u64) -> (f32, f32) {
        let row = (key >> 32) as u32 as f32;
        let col = key as u32 as f32;
        (
            self.lat_min + (row + 0.5) * GRID_CELL_SIZE_DEG,
            self.lon_min + (col + 0.5) * GRID_CELL_SIZE_DEG,
        )
    }

    #[inline]
    fn military_center(&self, key: u64) -> (f32, f32) {
        let row = (key >> 32) as u32 as f32;
        let col = key as u32 as f32;
        (
            self.lat_min + (row + 0.5) * MILITARY_GRID_CELL_SIZE_DEG,
            self.lon_min + (col + 0.5) * MILITARY_GRID_CELL_SIZE_DEG,
        )
    }
}

struct IndexedBox {
    idx: u32,
    env: AABB<[f64; 2]>,
}

impl RTreeObject for IndexedBox {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        self.env
    }
}

/// Exact f32 prefilter box: `BBox::could_be_within(lat, lon, distance_m)` <=> point inside.
fn prefilter_box(b: &BBox, distance_m: f32) -> Option<[f32; 4]> {
    if !(b.lat_min <= b.lat_max && b.lon_min <= b.lon_max) {
        return None;
    }
    let deg_buffer = distance_m / 111_000.0 * 1.5;
    Some([
        b.lat_min - deg_buffer,
        b.lat_max + deg_buffer,
        b.lon_min - deg_buffer,
        b.lon_max + deg_buffer,
    ])
}

fn prefilter_tree(boxes: &[Option<[f32; 4]>]) -> RTree<IndexedBox> {
    let entries = boxes
        .iter()
        .enumerate()
        .filter_map(|(idx, b)| {
            b.map(|b| IndexedBox {
                idx: idx as u32,
                env: AABB::from_corners([b[2] as f64, b[0] as f64], [b[3] as f64, b[1] as f64]),
            })
        })
        .collect();
    RTree::bulk_load(entries)
}

#[inline]
fn in_box(b: &[f32; 4], lat: f32, lon: f32) -> bool {
    lat >= b[0] && lat <= b[1] && lon >= b[2] && lon <= b[3]
}

/// Group sorted cell keys into runs of cells in the same row with a bounded column span.
fn row_runs(keys: &[u64], max_len: usize, max_span: u32) -> Vec<std::ops::Range<usize>> {
    let mut runs = Vec::new();
    let mut start = 0;
    while start < keys.len() {
        let row = keys[start] >> 32;
        let col0 = keys[start] as u32;
        let mut end = start + 1;
        while end < keys.len()
            && end - start < max_len
            && keys[end] >> 32 == row
            && (keys[end] as u32) - col0 <= max_span
        {
            end += 1;
        }
        runs.push(start..end);
        start = end;
    }
    runs
}

// ---------------------------------------------------------------------------------------------
// Residential

/// Mean earth radius used by geo's Haversine, in metres per degree.
const METERS_PER_DEG: f64 = 6_371_008.8 * std::f64::consts::PI / 180.0;
/// Safety margin (m) for lower-bound rejections: covers float error, the SIMD vertex-distance
/// fallback's approximation error, and the closest point not being exactly on the segment.
const REJECT_MARGIN_M: f64 = 25.0;

/// Segments per chunk of a prepared ring (chunk bboxes let scans skip far/irrelevant edges).
const CHUNK: usize = 16;

/// A ring with per-chunk bounding boxes: chunk i covers segments
/// [i*CHUNK, min((i+1)*CHUNK, n_segments)), bbox = [lat0, lat1, lon0, lon1].
struct PreparedRing {
    chunks: Vec<[f64; 4]>,
}

fn prepare_ring(ring: &geo::LineString<f64>) -> PreparedRing {
    let segs = ring.0.len().saturating_sub(1);
    let chunks = (0..segs.div_ceil(CHUNK))
        .map(|c| {
            let mut b = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
            let end = ((c + 1) * CHUNK).min(segs);
            for p in &ring.0[c * CHUNK..=end] {
                b[0] = b[0].min(p.y);
                b[1] = b[1].max(p.y);
                b[2] = b[2].min(p.x);
                b[3] = b[3].max(p.x);
            }
            b
        })
        .collect();
    PreparedRing { chunks }
}

struct PreparedPolygon<'a> {
    polygon: &'a MultiPolygon<f64>,
    /// geodesic area, computed on first use (only polygons within 500 m of a cell need it)
    area: std::sync::OnceLock<f32>,
    centroid: (f64, f64),
    /// f64 bbox over all rings: lat_min, lat_max, lon_min, lon_max
    bounds: [f64; 4],
    max_abs_lat: f64,
    /// REJECT_MARGIN_M + great-circle bulge of the longest segment (m)
    margin_m: f64,
    /// exterior + interiors (only for single-polygon multipolygons)
    rings: Vec<PreparedRing>,
}

impl PreparedPolygon<'_> {
    fn area(&self) -> f32 {
        *self
            .area
            .get_or_init(|| self.polygon.geodesic_area_signed().abs() as f32)
    }
}

fn prepare_polygon(mp: &MultiPolygon<f64>) -> PreparedPolygon<'_> {
    let mut b = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
    let mut max_seg_rad: f64 = 0.0;
    for polygon in mp.iter() {
        for ring in std::iter::once(polygon.exterior()).chain(polygon.interiors()) {
            for c in ring.coords() {
                b[0] = b[0].min(c.y);
                b[1] = b[1].max(c.y);
                b[2] = b[2].min(c.x);
                b[3] = b[3].max(c.x);
            }
            for w in ring.0.windows(2) {
                let len = ((w[0].y - w[1].y).abs() + (w[0].x - w[1].x).abs()).to_radians();
                max_seg_rad = max_seg_rad.max(len);
            }
        }
    }
    let max_abs_lat = b[0].abs().max(b[1].abs());
    // Max latitude excursion of a great-circle arc above its endpoints ~ L^2/8 * tan(lat);
    // doubled for safety.
    let tan = (max_abs_lat + 1.0).min(89.0).to_radians().tan();
    let bulge_m = max_seg_rad * max_seg_rad / 8.0 * tan * 2.0 * 6_371_008.8;
    let rings = if mp.0.len() == 1 {
        let polygon = &mp.0[0];
        std::iter::once(polygon.exterior())
            .chain(polygon.interiors())
            .map(prepare_ring)
            .collect()
    } else {
        Vec::new()
    };
    PreparedPolygon {
        polygon: mp,
        area: std::sync::OnceLock::new(),
        centroid: compute_multipolygon_centroid(mp),
        bounds: b,
        max_abs_lat,
        margin_m: REJECT_MARGIN_M + bulge_m,
        rings,
    }
}

/// Lower-bound test: true if every point of the box [lat0,lat1]x[lon0,lon1] is surely farther
/// than `limit_m` (great-circle) from (plat, plon). `lon_scale` = METERS_PER_DEG * cos(max |lat|
/// of point and box) * 0.999 (a valid lower bound for the longitude term). The longitude test
/// is not used near the antimeridian (longitudes are compared linearly).
#[inline]
#[allow(clippy::too_many_arguments)]
fn box_surely_beyond(
    plat: f64,
    plon: f64,
    lat0: f64,
    lat1: f64,
    lon0: f64,
    lon1: f64,
    limit_m: f64,
    lon_scale: f64,
) -> bool {
    let dlat = (lat0 - plat).max(plat - lat1).max(0.0);
    if dlat * METERS_PER_DEG > limit_m {
        return true;
    }
    if plon.abs() > 179.0 || lon0 < -179.0 || lon1 > 179.0 {
        return false;
    }
    let dlon = (lon0 - plon).max(plon - lon1).max(0.0);
    dlon * lon_scale > limit_m
}

/// Lower bound on the great-circle distance from (plat, plon) to the arc a-b, via the planar
/// point-segment distance in a local equirectangular frame scaled by 0.999*R (latitude) and
/// `lon_scale` = 0.999*R*cos(max |lat|) (longitude). For small angles
/// hav(d) >= hav(dlat) + cos^2(max lat) hav(dlon) gives d >= 0.999 * that planar distance; the
/// arc deviates from the planar segment by at most the bulge, which is part of `limit_m`.
#[inline]
fn segment_surely_beyond(
    plat: f64,
    plon: f64,
    a: Coord<f64>,
    b: Coord<f64>,
    limit_m: f64,
    lon_scale: f64,
) -> bool {
    if plon.abs() > 179.0 || a.x.abs() > 179.0 || b.x.abs() > 179.0 {
        return false;
    }
    let ky = METERS_PER_DEG * 0.999;
    let (ax, ay) = ((a.x - plon) * lon_scale, (a.y - plat) * ky);
    let (bx, by) = ((b.x - plon) * lon_scale, (b.y - plat) * ky);
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (-(ax * dx + ay * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (cx, cy) = (ax + t * dx, ay + t * dy);
    // 1e-9 relative slack for rounding in the planar maths
    (cx * cx + cy * cy).sqrt() * (1.0 - 1e-9) > limit_m
}

enum RingScan {
    /// some segment's closest point is within the threshold
    Within,
    /// at least one finite distance, none within
    Beyond,
    /// no finite distance computed and nothing skipped (reference: Indeterminate)
    AllNaN,
    /// no finite distance computed but some segments were skipped by the bound
    Unknown,
}

// Per-thread counters for the residential within-test (summed with `rayon::broadcast`):
// [within_threshold calls, bbox rejections, haversine segment evaluations, contains calls,
//  reference fallbacks]
thread_local! {
    static PROX_STATS: std::cell::Cell<[u64; 5]> = const { std::cell::Cell::new([0; 5]) };
}

#[inline]
fn stat(i: usize) {
    PROX_STATS.with(|c| {
        let mut v = c.get();
        v[i] += 1;
        c.set(v);
    });
}

#[allow(clippy::too_many_arguments)]
fn scan_ring(
    ring: &geo::LineString<f64>,
    prepared: &PreparedRing,
    from: Point<f64>,
    threshold: f64,
    limit_m: f64,
    lon_scale: f64,
) -> RingScan {
    let (plat, plon) = (from.y(), from.x());
    let mut any_finite = false;
    let mut skipped = false;
    let segs = ring.0.len().saturating_sub(1);
    for (c, cb) in prepared.chunks.iter().enumerate() {
        if box_surely_beyond(plat, plon, cb[0], cb[1], cb[2], cb[3], limit_m, lon_scale) {
            skipped = true;
            continue;
        }
        for i in c * CHUNK..((c + 1) * CHUNK).min(segs) {
            let (a, b) = (ring.0[i], ring.0[i + 1]);
            if box_surely_beyond(
                plat,
                plon,
                a.y.min(b.y),
                a.y.max(b.y),
                a.x.min(b.x),
                a.x.max(b.x),
                limit_m,
                lon_scale,
            ) || segment_surely_beyond(plat, plon, a, b, limit_m, lon_scale)
            {
                skipped = true;
                continue;
            }
            stat(2);
            match Line::new(a, b).haversine_closest_point(&from) {
                geo::Closest::Intersection(_) => return RingScan::Within,
                geo::Closest::SinglePoint(pt) => {
                    let d = Haversine.distance(from, pt);
                    if d.is_nan() {
                        continue;
                    }
                    any_finite = true;
                    if d <= threshold {
                        return RingScan::Within;
                    }
                }
                geo::Closest::Indeterminate => {}
            }
        }
    }
    match (any_finite, skipped) {
        (true, _) => RingScan::Beyond,
        (false, false) => RingScan::AllNaN,
        (false, true) => RingScan::Unknown,
    }
}

/// True if some segment of the ring has a finite reference closest-point distance (used when a
/// scan evaluated nothing finite but skipped provably-far segments: their distances, if finite,
/// all exceed the threshold, so one finite value is enough to know the reference result is
/// "beyond" rather than indeterminate).
fn ring_has_finite_distance(ring: &geo::LineString<f64>, from: Point<f64>) -> bool {
    ring.0.windows(2).any(|w| {
        stat(2);
        match Line::new(w[0], w[1]).haversine_closest_point(&from) {
            geo::Closest::Intersection(_) => true,
            geo::Closest::SinglePoint(pt) => !Haversine.distance(from, pt).is_nan(),
            geo::Closest::Indeterminate => false,
        }
    })
}

#[derive(PartialEq, Eq)]
enum RingPos {
    Inside,
    Outside,
    OnBoundary,
}

/// Exact replica of geo's `coord_pos_relative_to_ring` (winding number, same edge rules, same
/// robust orientation predicate), visiting only chunks whose latitude range contains the point
/// (edges outside it cannot contribute under those rules).
fn ring_position(
    ring: &geo::LineString<f64>,
    prepared: &PreparedRing,
    coord: Coord<f64>,
) -> RingPos {
    use geo::{GeoNum, Kernel, Orientation};
    type Ker = <f64 as GeoNum>::Ker;
    if ring.0.is_empty() {
        return RingPos::Outside;
    }
    if ring.0.len() == 1 {
        return if coord == ring.0[0] {
            RingPos::OnBoundary
        } else {
            RingPos::Outside
        };
    }
    let in_between = |v: f64, a: f64, b: f64| {
        if a < b {
            v >= a && v <= b
        } else {
            v >= b && v <= a
        }
    };
    let segs = ring.0.len() - 1;
    let mut winding = 0i64;
    for (c, cb) in prepared.chunks.iter().enumerate() {
        if coord.y < cb[0] || coord.y > cb[1] {
            continue;
        }
        for i in c * CHUNK..((c + 1) * CHUNK).min(segs) {
            let (start, end) = (ring.0[i], ring.0[i + 1]);
            if start.y <= coord.y {
                if end.y >= coord.y {
                    let o = Ker::orient2d(start, end, coord);
                    if o == Orientation::CounterClockwise && end.y != coord.y {
                        winding += 1
                    } else if o == Orientation::Collinear && in_between(coord.x, start.x, end.x) {
                        return RingPos::OnBoundary;
                    }
                }
            } else if end.y <= coord.y {
                let o = Ker::orient2d(start, end, coord);
                if o == Orientation::Clockwise {
                    winding -= 1
                } else if o == Orientation::Collinear && in_between(coord.x, start.x, end.x) {
                    return RingPos::OnBoundary;
                }
            }
        }
    }
    if winding == 0 {
        RingPos::Outside
    } else {
        RingPos::Inside
    }
}

/// Exactly geo's `Polygon::contains(Point)` for a single-polygon prepared multipolygon.
fn polygon_contains(p: &PreparedPolygon, coord: Coord<f64>) -> bool {
    let polygon = &p.polygon.0[0];
    if polygon.exterior().0.is_empty() {
        return false;
    }
    if ring_position(polygon.exterior(), &p.rings[0], coord) != RingPos::Inside {
        return false;
    }
    for (hole, prepared) in polygon.interiors().iter().zip(&p.rings[1..]) {
        if ring_position(hole, prepared, coord) != RingPos::Outside {
            return false;
        }
    }
    true
}

/// Exactly `polygon_distance_m(polygon, lat, lon) <= threshold`, computed with shortcuts:
/// * polygon bbox farther than threshold + margin -> false (lower bound);
/// * scan only segments (chunks) not provably beyond the threshold, stop at the first one whose
///   reference closest-point distance is within it -> true;
/// * otherwise the reference result is "inside" (planar contains, exact replica) or the minimum
///   over all segments, which is > threshold. Degenerate cases fall back to the reference.
fn within_threshold(p: &PreparedPolygon, lat: f32, lon: f32, threshold: f64) -> bool {
    let (plat, plon) = (lat as f64, lon as f64);
    let limit_m = threshold + p.margin_m;
    let lon_scale =
        METERS_PER_DEG * plat.abs().max(p.max_abs_lat).min(90.0).to_radians().cos() * 0.999;
    let b = &p.bounds;
    stat(0);
    if box_surely_beyond(plat, plon, b[0], b[1], b[2], b[3], limit_m, lon_scale) {
        stat(1);
        return false;
    }
    if p.polygon.0.len() != 1 {
        stat(4);
        return polygon_distance_m(p.polygon, lat, lon) <= threshold;
    }
    let polygon = &p.polygon.0[0];
    if polygon.exterior().0.len() < 3 {
        stat(4);
        return polygon_distance_m(p.polygon, lat, lon) <= threshold;
    }
    let from = Point::new(plon, plat);
    match scan_ring(
        polygon.exterior(),
        &p.rings[0],
        from,
        threshold,
        limit_m,
        lon_scale,
    ) {
        RingScan::Within => return true,
        RingScan::Beyond => {}
        RingScan::Unknown if ring_has_finite_distance(polygon.exterior(), from) => {}
        RingScan::AllNaN | RingScan::Unknown => {
            stat(4);
            return polygon_distance_m(p.polygon, lat, lon) <= threshold;
        }
    }
    for (interior, prepared) in polygon.interiors().iter().zip(&p.rings[1..]) {
        match scan_ring(interior, prepared, from, threshold, limit_m, lon_scale) {
            RingScan::Within => return true,
            RingScan::Beyond | RingScan::AllNaN | RingScan::Unknown => {}
        }
    }
    stat(3);
    let inside = polygon_contains(p, from.0);
    if verify_contains() {
        assert_eq!(inside, polygon.contains(&from), "contains replica mismatch");
    }
    inside
}

fn verify_contains() -> bool {
    static VERIFY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *VERIFY.get_or_init(|| std::env::var("RIDI_VERIFY_PROXIMITY").as_deref() == Ok("1"))
}

/// Residential flag per cell key (keys sorted & unique).
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub(crate) fn residential_cells(
    anchor: &GridAnchor,
    cell_keys: &[u64],
    polygons: &[MultiPolygon<f64>],
) -> Vec<bool> {
    if polygons.is_empty() {
        return vec![false; cell_keys.len()];
    }
    // Prepared lazily: area (geodesic), centroid and ring chunks are only computed for
    // polygons that are a candidate of at least one cell.
    let prepared: Vec<std::sync::OnceLock<PreparedPolygon>> = (0..polygons.len())
        .map(|_| std::sync::OnceLock::new())
        .collect();
    let prepared_at = |idx: usize| prepared[idx].get_or_init(|| prepare_polygon(&polygons[idx]));
    let boxes: Vec<Option<[f32; 4]>> = polygons
        .iter()
        .map(|mp| {
            prefilter_box(
                &compute_multipolygon_bbox(mp),
                RESIDENTIAL_PROXIMITY_THRESHOLD_M,
            )
        })
        .collect();
    let tree = prefilter_tree(&boxes);
    let threshold = RESIDENTIAL_PROXIMITY_THRESHOLD_M as f64;

    let runs = row_runs(cell_keys, 64, 64);
    let flags: Vec<Vec<bool>> = runs
        .par_iter()
        .map(|run| {
            let keys = &cell_keys[run.clone()];
            let (lat, lon_first) = anchor.residential_center(keys[0]);
            let (_, lon_last) = anchor.residential_center(keys[keys.len() - 1]);
            let env = AABB::from_corners(
                [lon_first as f64, lat as f64],
                [lon_last as f64, lat as f64],
            );
            let mut candidates: Vec<u32> = tree
                .locate_in_envelope_intersecting(&env)
                .map(|e| e.idx)
                .collect();
            // Reference evaluation (summation) order
            candidates.sort_unstable();
            keys.iter()
                .map(|&key| {
                    let (lat, lon) = anchor.residential_center(key);
                    if verify_contains() {
                        // Check every shortcut against the reference maths, and the early exit
                        // against the full sum.
                        let mut sectors = [0.0f32; 8];
                        let mut fast_flag = None;
                        for &idx in &candidates {
                            let idx = idx as usize;
                            let Some(b) = &boxes[idx] else { continue };
                            if !in_box(b, lat, lon) {
                                continue;
                            }
                            let p = prepared_at(idx);
                            let reference = polygon_distance_m(p.polygon, lat, lon) <= threshold;
                            assert_eq!(
                                within_threshold(p, lat, lon, threshold),
                                reference,
                                "within_threshold mismatch: polygon {idx} cell ({lat}, {lon})"
                            );
                            if reference {
                                let bearing =
                                    calculate_bearing(lat, lon, p.centroid.0, p.centroid.1);
                                sectors[bearing_to_sector(bearing)] += p.area();
                                let total: f32 = sectors.iter().sum();
                                if total > THRESHOLD_AREA_M2 && fast_flag.is_none() {
                                    fast_flag = Some(true);
                                }
                            }
                        }
                        let total: f32 = sectors.iter().sum();
                        let full_flag = total > THRESHOLD_AREA_M2;
                        assert_eq!(fast_flag.unwrap_or(false), full_flag, "early exit mismatch");
                        return full_flag;
                    }
                    let mut sectors = [0.0f32; 8];
                    for &idx in &candidates {
                        let idx = idx as usize;
                        let Some(b) = &boxes[idx] else { continue };
                        if !in_box(b, lat, lon) {
                            continue;
                        }
                        let p = prepared_at(idx);
                        if within_threshold(p, lat, lon, threshold) {
                            let bearing = calculate_bearing(lat, lon, p.centroid.0, p.centroid.1);
                            sectors[bearing_to_sector(bearing)] += p.area();
                            let total: f32 = sectors.iter().sum();
                            if total > THRESHOLD_AREA_M2 {
                                return true;
                            }
                        }
                    }
                    false
                })
                .collect()
        })
        .collect();
    let flags: Vec<bool> = flags.into_iter().flatten().collect();
    let stats = rayon::broadcast(|_| PROX_STATS.with(|c| c.replace([0; 5])))
        .into_iter()
        .chain(std::iter::once(PROX_STATS.with(|c| c.replace([0; 5]))))
        .fold([0u64; 5], |mut acc, v| {
            for i in 0..5 {
                acc[i] += v[i];
            }
            acc
        });
    info!(
        "Residential: {} cells evaluated ({} runs), {} flagged ({} polygons); within-tests {}, bbox-rejected {}, segment haversines {}, contains {}, reference fallbacks {}",
        cell_keys.len(),
        runs.len(),
        flags.iter().filter(|f| **f).count(),
        polygons.len(),
        stats[0],
        stats[1],
        stats[2],
        stats[3],
        stats[4]
    );
    flags
}

// ---------------------------------------------------------------------------------------------
// Military

/// Military "Full" flag per military cell key (keys sorted & unique).
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub(crate) fn military_cells(
    anchor: &GridAnchor,
    cell_keys: &[u64],
    polygons: &[MultiPolygon<f64>],
) -> Vec<bool> {
    if polygons.is_empty() {
        return vec![false; cell_keys.len()];
    }
    let polygon_bboxes: Vec<(&MultiPolygon<f64>, BBox)> = polygons
        .iter()
        .map(|mp| (mp, compute_multipolygon_bbox(mp)))
        .collect();
    let boxes: Vec<Option<[f32; 4]>> = polygon_bboxes
        .iter()
        .map(|(_, b)| prefilter_box(b, 0.0))
        .collect();
    let tree = prefilter_tree(&boxes);
    let half_size = MILITARY_GRID_CELL_SIZE_DEG / 2.0;

    let runs = row_runs(cell_keys, 64, 64);
    let flags: Vec<Vec<bool>> = runs
        .par_iter()
        .map(|run| {
            let keys = &cell_keys[run.clone()];
            let (lat, lon_first) = anchor.military_center(keys[0]);
            let (_, lon_last) = anchor.military_center(keys[keys.len() - 1]);
            let env = AABB::from_corners(
                [
                    (lon_first - half_size) as f64 - 1e-6,
                    (lat - half_size) as f64 - 1e-6,
                ],
                [
                    (lon_last + half_size) as f64 + 1e-6,
                    (lat + half_size) as f64 + 1e-6,
                ],
            );
            let mut candidates: Vec<u32> = tree
                .locate_in_envelope_intersecting(&env)
                .map(|e| e.idx)
                .collect();
            candidates.sort_unstable();
            let mut scratch: Vec<(&MultiPolygon<f64>, BBox)> = Vec::new();
            keys.iter()
                .map(|&key| {
                    if candidates.is_empty() {
                        return false;
                    }
                    let (center_lat, center_lon) = anchor.military_center(key);
                    let points = [
                        (center_lat, center_lon),
                        (center_lat - half_size, center_lon - half_size),
                        (center_lat - half_size, center_lon + half_size),
                        (center_lat + half_size, center_lon - half_size),
                        (center_lat + half_size, center_lon + half_size),
                    ];
                    points.iter().all(|&(lat, lon)| {
                        scratch.clear();
                        for &idx in &candidates {
                            if let Some(b) = &boxes[idx as usize] {
                                if in_box(b, lat, lon) {
                                    scratch.push(polygon_bboxes[idx as usize]);
                                }
                            }
                        }
                        !scratch.is_empty() && is_military_interior(lat, lon, &scratch)
                    })
                })
                .collect()
        })
        .collect();
    let flags: Vec<bool> = flags.into_iter().flatten().collect();
    info!(
        "Military: {} cells evaluated, {} full ({} polygons)",
        cell_keys.len(),
        flags.iter().filter(|f| **f).count(),
        polygons.len()
    );
    flags
}
