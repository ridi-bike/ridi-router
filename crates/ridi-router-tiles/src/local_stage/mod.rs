//! Local stage: everything after "region data is in memory".
//!
//! Input (per region, see `LocalStageInput` and map-data/bench/LOCAL_STAGE.md):
//! routable nodes with coordinates, routable ways with reduced tags (refs as node-table
//! indices), restriction relations, residential/military polygons in reference order, the grid
//! anchor and an optional core box.
//!
//! Work: tile assignment (0.005 deg buffered tiles), sparse proximity flags, per-tile graph build
//! and byte-identical RMDF serialization (parallel), manifest entries.

pub mod manifest;
pub mod proximity;
mod tile;

use anyhow::Result;
use geo::MultiPolygon;
use rayon::prelude::*;
use ridi_router_common::manifest::TileMetadata;
use ridi_router_common::osm::OsmRelation;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use tracing::{info, warn};

pub use proximity::GridAnchor;

use crate::generation::{GenerationRestrictionSkipStats, LineDirection};
use crate::rmdf::format::TileId;

/// Tile buffer used by the reference generator (`add_buffer_to_bounds`).
pub const TILE_BUFFER_DEG: f32 = 0.005;

/// Node table: ascending OSM ids with decimicro-degree coordinates
/// (`lat = lat_e7 as f64 * 1e-7`, exactly as osmpbfreader computes it).
/// `lat_e7 == i32::MIN` marks a node referenced by a way but absent from the input.
pub struct NodeTable<'a> {
    pub ids: &'a [u64],
    pub lat_e7: &'a [i32],
    pub lon_e7: &'a [i32],
}

pub const MISSING_COORD: i32 = i32::MIN;

impl NodeTable<'_> {
    #[inline]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    #[inline]
    pub fn present(&self, idx: usize) -> bool {
        self.lat_e7[idx] != MISSING_COORD
    }

    #[inline]
    pub fn lat_lon(&self, idx: usize) -> (f64, f64) {
        (
            self.lat_e7[idx] as f64 * 1e-7,
            self.lon_e7[idx] as f64 * 1e-7,
        )
    }
}

/// Tag values of a routable way that the tile graph reads (raw OSM values).
#[derive(Default, Clone, PartialEq, Eq, Hash, Debug)]
pub struct WayTags {
    pub name: Option<String>,
    pub hw_ref: Option<String>,
    pub highway: Option<String>,
    pub surface: Option<String>,
    pub smoothness: Option<String>,
    pub oneway: Option<String>,
    pub junction: Option<String>,
}

impl WayTags {
    /// Same rule as `GenerationGraph::insert_way`.
    pub fn direction(&self) -> LineDirection {
        if let Some(oneway) = self.oneway.as_deref() {
            if oneway == "yes" || oneway == "1" || oneway == "true" {
                LineDirection::OneWay
            } else {
                LineDirection::BothWays
            }
        } else if let Some(junction) = self.junction.as_deref() {
            if junction == "roundabout" {
                LineDirection::Roundabout
            } else {
                LineDirection::BothWays
            }
        } else {
            LineDirection::BothWays
        }
    }
}

/// Routable ways (highway in the 17 allowed values, or path + motorcycle=yes), ascending id.
pub struct WayTable<'a> {
    pub ids: &'a [u64],
    /// refs of way i = refs[offsets[i]..offsets[i+1]]
    pub offsets: &'a [usize],
    /// node-table indices; u32::MAX = node not in the node table
    pub refs: &'a [u32],
    /// per way index into `tag_sets` (interned tag tuples)
    pub tag_set: &'a [u32],
    pub tag_sets: &'a [WayTags],
}

impl WayTable<'_> {
    #[inline]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    #[inline]
    pub fn refs_of(&self, idx: usize) -> &[u32] {
        &self.refs[self.offsets[idx]..self.offsets[idx + 1]]
    }

    #[inline]
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.ids.binary_search(&id).ok()
    }
}

pub struct LocalStageInput<'a> {
    pub output_dir: &'a Path,
    pub tile_size_degrees: f32,
    pub anchor: GridAnchor,
    /// Only tiles whose centre lies in [lon_min, lon_max) x [lat_min, lat_max) are written.
    pub core_bbox: Option<[f64; 4]>,
    /// Threads writing tile files (compute threads never block on file IO).
    pub writer_threads: usize,
    pub nodes: NodeTable<'a>,
    pub ways: WayTable<'a>,
    /// Restriction relations: type starts with "restriction", nested relations resolved like
    /// the reference loader (unresolvable ones dropped). Ascending id.
    pub restrictions: &'a [OsmRelation],
    /// Residential polygons in reference order (closed ways in file order, then relations).
    pub residential_polygons: &'a [MultiPolygon<f64>],
    pub military_polygons: &'a [MultiPolygon<f64>],
}

pub struct LocalStageOutput {
    /// Manifest entries of the written tiles (neighbours not filled; see `manifest`).
    pub tiles: Vec<TileMetadata>,
}

#[inline]
fn tile_key(col: u16, row: u16) -> u32 {
    ((col as u32) << 16) | row as u32
}

/// All tiles whose bounds buffered by 0.005 deg contain the point (reference f32 maths,
/// inclusive comparisons in f64 as in the reference R-tree envelope query).
#[inline]
pub(crate) fn tiles_for_point(lat: f64, lon: f64, ts: f32, out: &mut Vec<u32>) {
    out.clear();
    let margin = (TILE_BUFFER_DEG / ts).ceil() as i64 + 1;
    // The reference iterates cols/rows up to ceil((clamped bound + 180|90) / ts) exclusive, so
    // the last column is ceil(360/ts)-1 (3599 at 0.1 deg; its buffered bounds are clamped to
    // +180 inclusive) and the last row ceil(180/ts)-1.
    let max_col = ((360.0 / ts as f64).ceil() as i64).min(u16::MAX as i64 + 1);
    let max_row = ((180.0 / ts as f64).ceil() as i64).min(u16::MAX as i64 + 1);
    let c0 = ((lon + 180.0) / ts as f64).floor() as i64;
    let r0 = ((lat + 90.0) / ts as f64).floor() as i64;
    let mut cols = [0u16; 8];
    let mut ncols = 0;
    for col in (c0 - margin)..=(c0 + margin) {
        if col < 0 || col >= max_col {
            continue;
        }
        let lon_min = (col as u16 as f32 * ts) - 180.0;
        let lon_max = lon_min + ts;
        let bmin = (lon_min - TILE_BUFFER_DEG).max(-180.0);
        let bmax = (lon_max + TILE_BUFFER_DEG).min(180.0);
        if lon >= bmin as f64 && lon <= bmax as f64 && ncols < cols.len() {
            cols[ncols] = col as u16;
            ncols += 1;
        }
    }
    for row in (r0 - margin)..=(r0 + margin) {
        if row < 0 || row >= max_row {
            continue;
        }
        let lat_min = (row as u16 as f32 * ts) - 90.0;
        let lat_max = lat_min + ts;
        let bmin = (lat_min - TILE_BUFFER_DEG).max(-90.0);
        let bmax = (lat_max + TILE_BUFFER_DEG).min(90.0);
        if lat >= bmin as f64 && lat <= bmax as f64 {
            for &col in &cols[..ncols] {
                out.push(tile_key(col, row as u16));
            }
        }
    }
}

fn tile_is_owned(core: Option<[f64; 4]>, ts: f32, col: u16, row: u16) -> bool {
    let Some([lon_min, lat_min, lon_max, lat_max]) = core else {
        return true;
    };
    let ts = ts as f64;
    let c_lon = col as f64 * ts - 180.0 + ts / 2.0;
    let c_lat = row as f64 * ts - 90.0 + ts / 2.0;
    c_lon >= lon_min && c_lon < lon_max && c_lat >= lat_min && c_lat < lat_max
}

/// Run the local stage. Calls `phase(name)` after each sub-phase (for uniform phase logging).
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub fn run(input: &LocalStageInput<'_>, phase: &mut dyn FnMut(&str)) -> Result<LocalStageOutput> {
    let ts = input.tile_size_degrees;
    let nodes = &input.nodes;
    let ways = &input.ways;
    std::fs::create_dir_all(input.output_dir)?;

    // ---- routable nodes + tile assignment
    let mut routable = vec![false; nodes.len()];
    for &r in ways.refs {
        if r != u32::MAX && nodes.present(r as usize) {
            routable[r as usize] = true;
        }
    }
    let routable_nodes: Vec<u32> = (0..nodes.len() as u32)
        .filter(|&i| routable[i as usize])
        .collect();
    drop(routable);

    let mut node_pairs: Vec<(u32, u32)> = routable_nodes
        .par_iter()
        .map_init(Vec::new, |buf, &n| {
            let (lat, lon) = nodes.lat_lon(n as usize);
            tiles_for_point(lat, lon, ts, buf);
            buf.iter().map(|&t| (t, n)).collect::<Vec<_>>()
        })
        .flatten_iter()
        .collect();
    node_pairs.par_sort_unstable();

    let mut way_pairs: Vec<(u32, u32)> = (0..ways.len())
        .into_par_iter()
        .map_init(
            || (Vec::new(), Vec::new()),
            |(buf, acc): &mut (Vec<u32>, Vec<u32>), w| {
                acc.clear();
                for &n in ways.refs_of(w) {
                    if n == u32::MAX || !nodes.present(n as usize) {
                        continue;
                    }
                    let (lat, lon) = nodes.lat_lon(n as usize);
                    tiles_for_point(lat, lon, ts, buf);
                    acc.extend_from_slice(buf);
                }
                acc.sort_unstable();
                acc.dedup();
                acc.iter().map(|&t| (t, w as u32)).collect::<Vec<_>>()
            },
        )
        .flatten_iter()
        .collect();
    way_pairs.par_sort_unstable();
    info!(
        "Tile assignment: {} routable nodes, {} node-tile pairs, {} way-tile pairs",
        routable_nodes.len(),
        node_pairs.len(),
        way_pairs.len()
    );
    phase("tile_assignment");

    // ---- proximity flags (sparse)
    let anchor = input.anchor;
    let cell_of = |n: u32| {
        let (lat, lon) = nodes.lat_lon(n as usize);
        let (lat, lon) = (lat as f32, lon as f32);
        (
            anchor.residential_cell(lat, lon),
            anchor.military_cell(lat, lon),
        )
    };
    let mut res_keys: Vec<u64> = routable_nodes
        .par_iter()
        .map(|&n| cell_of(n).0.unwrap_or(u64::MAX))
        .collect();
    res_keys.par_sort_unstable();
    res_keys.dedup();
    if res_keys.last() == Some(&u64::MAX) {
        res_keys.pop();
    }
    res_keys.shrink_to_fit();
    let mut mil_keys: Vec<u64> = routable_nodes
        .par_iter()
        .map(|&n| cell_of(n).1.unwrap_or(u64::MAX))
        .collect();
    mil_keys.par_sort_unstable();
    mil_keys.dedup();
    if mil_keys.last() == Some(&u64::MAX) {
        mil_keys.pop();
    }
    mil_keys.shrink_to_fit();
    let (res_flags, mil_flags) = rayon::join(
        || proximity::residential_cells(&anchor, &res_keys, input.residential_polygons),
        || proximity::military_cells(&anchor, &mil_keys, input.military_polygons),
    );
    // per node-table index: bit0 residential, bit1 nogo
    let mut node_flags = vec![0u8; nodes.len()];
    let flag_values: Vec<u8> = routable_nodes
        .par_iter()
        .map(|&n| {
            let (res, mil) = cell_of(n);
            let r = res
                .and_then(|k| res_keys.binary_search(&k).ok())
                .is_some_and(|i| res_flags[i]);
            let m = mil
                .and_then(|k| mil_keys.binary_search(&k).ok())
                .is_some_and(|i| mil_flags[i]);
            (r as u8) | ((m as u8) << 1)
        })
        .collect();
    for (&n, f) in routable_nodes.iter().zip(flag_values) {
        node_flags[n as usize] = f;
    }
    drop(res_keys);
    drop(mil_keys);
    drop(routable_nodes);
    phase("proximity");

    // ---- restriction index: way idx -> restriction idx (restrictions ascending by id)
    let mut way_restrictions: Vec<(u32, u32)> = input
        .restrictions
        .iter()
        .enumerate()
        .flat_map(|(i, rel)| {
            rel.members
                .iter()
                .filter(|m| m.member_type == ridi_router_common::osm::OsmRelationMemberType::Way)
                .filter_map(|m| ways.index_of(m.member_ref))
                .map(move |w| (w as u32, i as u32))
                .collect::<Vec<_>>()
        })
        .collect();
    way_restrictions.sort_unstable();
    way_restrictions.dedup();

    // ---- tiles: (key, node range, way range)
    let mut tiles: Vec<(u32, std::ops::Range<usize>, std::ops::Range<usize>)> = Vec::new();
    {
        let mut ni = 0usize;
        let mut wi = 0usize;
        while wi < way_pairs.len() {
            let key = way_pairs[wi].0;
            let w_start = wi;
            while wi < way_pairs.len() && way_pairs[wi].0 == key {
                wi += 1;
            }
            while ni < node_pairs.len() && node_pairs[ni].0 < key {
                ni += 1;
            }
            let n_start = ni;
            while ni < node_pairs.len() && node_pairs[ni].0 == key {
                ni += 1;
            }
            tiles.push((key, n_start..ni, w_start..wi));
        }
    }
    let candidate_tiles = tiles.len();
    tiles.retain(|(key, _, _)| tile_is_owned(input.core_bbox, ts, (key >> 16) as u16, *key as u16));
    info!(
        "Writing {} tiles ({} candidate tiles before core filter)",
        tiles.len(),
        candidate_tiles
    );

    // Big tiles first for better load balance
    tiles.sort_by_key(|(_, n, _)| std::cmp::Reverse(n.len()));
    let written = AtomicUsize::new(0);
    let total_stats = Mutex::new(GenerationRestrictionSkipStats::default());
    // File writes run on dedicated threads so compute threads never stall in the kernel's
    // dirty-page throttling; the bounded channel caps the tile bytes in flight.
    let writer_threads = input.writer_threads.max(1);
    let (file_tx, file_rx) = std::sync::mpsc::sync_channel::<(std::path::PathBuf, Vec<u8>)>(
        rayon::current_num_threads() * 2,
    );
    let file_rx = Mutex::new(file_rx);
    let write_error: Mutex<Option<anyhow::Error>> = Mutex::new(None);
    let results = std::thread::scope(|scope| {
        for _ in 0..writer_threads {
            scope.spawn(|| loop {
                let next = file_rx.lock().expect("writer queue").recv();
                let Ok((path, bytes)) = next else { break };
                let written = hotpath::measure_block!("local_stage.file_write", {
                    std::fs::write(&path, &bytes)
                });
                if let Err(error) = written {
                    write_error
                        .lock()
                        .expect("write error")
                        .get_or_insert(anyhow::anyhow!("Failed to write {path:?}: {error}"));
                    break;
                }
            });
        }
        let file_tx = file_tx;
        let results: Result<Vec<TileMetadata>> = tiles
            .par_iter()
            .map_init(
                || {
                    (
                        Vec::<u32>::new(),
                        Vec::<u32>::new(),
                        Vec::<&OsmRelation>::new(),
                    )
                },
                |(points, tile_ways, rels), (key, node_range, way_range)| -> Result<TileMetadata> {
                    let tile_id = TileId {
                        col: (key >> 16) as u16,
                        row: *key as u16,
                    };
                    points.clear();
                    points.extend(node_pairs[node_range.clone()].iter().map(|p| p.1));
                    tile_ways.clear();
                    tile_ways.extend(way_pairs[way_range.clone()].iter().map(|p| p.1));
                    let mut rel_idxs: Vec<u32> = Vec::new();
                    for &w in tile_ways.iter() {
                        let start = way_restrictions.partition_point(|&(wr, _)| wr < w);
                        rel_idxs.extend(
                            way_restrictions[start..]
                                .iter()
                                .take_while(|&&(wr, _)| wr == w)
                                .map(|&(_, r)| r),
                        );
                    }
                    rel_idxs.sort_unstable();
                    rel_idxs.dedup();
                    rels.clear();
                    rels.extend(rel_idxs.iter().map(|&r| &input.restrictions[r as usize]));

                    let mut stats = GenerationRestrictionSkipStats::default();
                    let result = hotpath::measure_block!("local_stage.tile", {
                        tile::build_tile(
                            tile_id,
                            ts,
                            nodes,
                            &node_flags,
                            ways,
                            points,
                            tile_ways,
                            rels,
                            &mut stats,
                        )
                    })?;
                    if !stats.is_empty() {
                        let mut total = total_stats.lock().expect("stats lock");
                        total.via_way_relations += stats.via_way_relations;
                        total.malformed_or_unresolved_relations +=
                            stats.malformed_or_unresolved_relations;
                    }
                    let n = written.fetch_add(1, Ordering::Relaxed) + 1;
                    if n % 2000 == 0 {
                        info!("Tile writing progress: {}/{}", n, tiles.len());
                    }
                    // Hand the bytes to the writer threads (blocks when they are behind, which
                    // bounds the bytes in flight).
                    let path = input.output_dir.join(&result.metadata.filename);
                    file_tx
                        .send((path, result.bytes))
                        .map_err(|_| anyhow::anyhow!("tile writer threads stopped"))?;
                    Ok(result.metadata)
                },
            )
            .collect::<Result<Vec<_>>>();
        drop(file_tx);
        results
    });
    if let Some(error) = write_error.into_inner().expect("write error") {
        return Err(error);
    }
    let results = results?;
    let stats = total_stats.into_inner().expect("stats lock");
    if !stats.is_empty() {
        warn!(
            "Skipped restriction relations (per tile, summed over {} tiles): {}",
            results.len(),
            stats.summary_parts().join(", ")
        );
    }
    let total_bytes: u64 = results.iter().map(|t| t.size_bytes).sum();
    let largest = results.iter().max_by_key(|t| t.size_bytes);
    info!(
        "Wrote {} tiles, {:.1} MB total, largest {} ({:.1} MB, {} points, {} lines)",
        results.len(),
        total_bytes as f64 / 1e6,
        largest.map(|t| t.filename.as_str()).unwrap_or("-"),
        largest.map(|t| t.size_bytes as f64 / 1e6).unwrap_or(0.0),
        largest.map(|t| t.point_count).unwrap_or(0),
        largest.map(|t| t.line_count).unwrap_or(0)
    );
    phase("write_tiles");

    Ok(LocalStageOutput { tiles: results })
}
