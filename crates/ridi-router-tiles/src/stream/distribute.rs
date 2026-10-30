//! Resolve spilled ways against the node store and distribute ways, polygons and restriction
//! relations into per-region bucket files.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use anyhow::Result;
use geo::{Contains, Coord, LineString, MultiPolygon, Point, Polygon};
use rayon::prelude::*;
use tracing::info;

use super::buckets::{RegionBuckets, SpillReader, REC_POLY, REC_RESTRICTION, REC_WAY};
use super::geom::{smallvec_lite::SmallTiles, TileGrid};
use super::nodeidx::dec_coord;
use super::passes::{area_frame_refs, routable_frame_refs, AREA_MEMBER, AREA_MIL, AREA_RES};
use super::records::*;
use super::rings::connect_ways_into_rings;
use super::store::Resolver;
use super::util::*;
use crate::local_stage::proximity::lon_degrees_for;
use crate::proximity::compute_multipolygon_bbox;

/// Margin (degrees of latitude) around a polygon bbox when choosing regions: covers the
/// 500 m * 1.5 prefilter (0.00676 deg), the tile buffer (0.005 deg) and cell-centre offsets.
/// The longitude margin is the same ground distance (see `lon_degrees_for`).
const POLY_REGION_MARGIN_DEG: f64 = 0.02;

pub struct RouteStats {
    pub ways: u64,
    pub way_records: u64,
    pub missing_nodes: u64,
}

/// Distribute routable ways. Returns the regions each restriction-member way went to.
#[hotpath::measure]
pub fn distribute_ways(
    spill: &SpillReader,
    resolver: &Resolver,
    grid: &TileGrid,
    buckets: &RegionBuckets,
    restriction_way_ids: &IdSet,
    region_used: &[AtomicBool],
) -> Result<(HashMap<u64, Vec<u32>>, RouteStats)> {
    let member_regions: Mutex<HashMap<u64, Vec<u32>>> = Mutex::new(HashMap::new());
    let n_ways = AtomicU64::new(0);
    let n_records = AtomicU64::new(0);
    let n_missing = AtomicU64::new(0);

    resolver.for_each_frame(
        spill,
        "route ways",
        routable_frame_refs,
        || buckets.local(),
        |local, _fi, frame, frame_coords| -> Result<()> {
            let mut ci = 0usize;
            let mut pos = 0;
            let mut ids: Vec<i64> = Vec::new();
            let mut coords: Vec<Option<(i32, i32)>> = Vec::new();
            let mut regions: Vec<u32> = Vec::new();
            let mut cols = SmallTiles::default();
            let mut rows = SmallTiles::default();
            let mut rec = Vec::new();
            let mut local_members: Vec<(u64, Vec<u32>)> = Vec::new();
            let (mut nw, mut nr, mut nm) = (0u64, 0u64, 0u64);
            while pos < frame.len() {
                let id = get_uvarint(&frame, &mut pos);
                let tags_start = pos;
                let mut tmp = Vec::new();
                copy_way_tags(&frame, &mut pos, &mut tmp);
                let tags_end = pos;
                let n = get_uvarint(&frame, &mut pos) as usize;
                ids.clear();
                coords.clear();
                regions.clear();
                let mut acc = 0i64;
                for _ in 0..n {
                    acc += get_svarint(&frame, &mut pos);
                    ids.push(acc);
                    let c = dec_coord(frame_coords[ci]);
                    ci += 1;
                    if let Some((la, lo)) = c {
                        grid.cols_for(decimicro_to_deg(lo), &mut cols);
                        grid.rows_for(decimicro_to_deg(la), &mut rows);
                        for &r in rows.as_slice() {
                            for &c in cols.as_slice() {
                                let reg = grid.region_of_tile(c, r);
                                if !regions.contains(&reg) {
                                    regions.push(reg);
                                }
                            }
                        }
                    } else {
                        nm += 1;
                    }
                    coords.push(c);
                }
                nw += 1;
                if regions.is_empty() {
                    continue;
                }
                // encode the region record once
                rec.clear();
                put_uvarint(&mut rec, id);
                rec.extend_from_slice(&frame[tags_start..tags_end]);
                let _ = &tmp;
                put_uvarint(&mut rec, n as u64);
                let (mut pid, mut pla, mut plo) = (0i64, 0i64, 0i64);
                for (nid, c) in ids.iter().zip(coords.iter()) {
                    let d = nid - pid;
                    pid = *nid;
                    let zz = ((d << 1) ^ (d >> 63)) as u64;
                    put_uvarint(&mut rec, (zz << 1) | c.is_some() as u64);
                    if let Some((la, lo)) = c {
                        put_svarint(&mut rec, *la as i64 - pla);
                        put_svarint(&mut rec, *lo as i64 - plo);
                        pla = *la as i64;
                        plo = *lo as i64;
                    }
                }
                for &reg in &regions {
                    local.push(reg, REC_WAY, &rec)?;
                    nr += 1;
                    if !region_used[reg as usize].load(Ordering::Relaxed) {
                        region_used[reg as usize].store(true, Ordering::Relaxed);
                    }
                }
                if restriction_way_ids.contains(&id) {
                    local_members.push((id, regions.clone()));
                }
            }
            n_ways.fetch_add(nw, Ordering::Relaxed);
            n_records.fetch_add(nr, Ordering::Relaxed);
            n_missing.fetch_add(nm, Ordering::Relaxed);
            if !local_members.is_empty() {
                let mut m = member_regions.lock().unwrap();
                for (id, regs) in local_members {
                    m.entry(id).or_default().extend(regs);
                }
            }
            Ok(())
        },
    )?;
    let stats = RouteStats {
        ways: n_ways.load(Ordering::Relaxed),
        way_records: n_records.load(Ordering::Relaxed),
        missing_nodes: n_missing.load(Ordering::Relaxed),
    };
    info!(
        "Distributed {} routable ways as {} region records ({} missing node refs)",
        stats.ways, stats.way_records, stats.missing_nodes
    );
    Ok((member_regions.into_inner().unwrap(), stats))
}

#[hotpath::measure]
pub fn distribute_restrictions(
    restrictions: &[RestrictionRel],
    unresolved: &HashSet<u64>,
    member_regions: &HashMap<u64, Vec<u32>>,
    buckets: &RegionBuckets,
) -> Result<u64> {
    let mut local = buckets.local();
    let mut buf = Vec::new();
    let mut regions: Vec<u32> = Vec::new();
    let mut written = 0;
    for r in restrictions {
        if unresolved.contains(&r.id) {
            continue;
        }
        regions.clear();
        for &(t, _, id) in &r.members {
            if t == 1 {
                if let Some(regs) = member_regions.get(&id) {
                    regions.extend_from_slice(regs);
                }
            }
        }
        if regions.is_empty() {
            continue;
        }
        regions.sort_unstable();
        regions.dedup();
        buf.clear();
        r.encode(&mut buf);
        for &reg in &regions {
            local.push(reg, REC_RESTRICTION, &buf)?;
        }
        written += 1;
    }
    local.flush()?;
    info!("Distributed {} restriction relations", written);
    Ok(written)
}

struct MemberWay {
    /// first == last node id and >= 4 node ids
    ids_closed: bool,
    coords: Vec<(i32, i32)>,
}

fn to_linestring(coords: &[(i32, i32)]) -> LineString<f64> {
    LineString::new(
        coords
            .iter()
            .map(|&(la, lo)| Coord {
                x: decimicro_to_deg(lo),
                y: decimicro_to_deg(la),
            })
            .collect(),
    )
}

fn from_linestring(ls: &LineString<f64>) -> Vec<(i32, i32)> {
    ls.0.iter()
        .map(|c| ((c.y * 1e7).round() as i32, (c.x * 1e7).round() as i32))
        .collect()
}

/// Polygons that would be copied into more than `max_regions` regions are written once to a
/// shared file and loaded by every tile-stage job instead.
pub struct LargePolys {
    out: Mutex<std::io::BufWriter<std::fs::File>>,
    pub max_regions: usize,
    pub count: AtomicU64,
}

impl LargePolys {
    pub fn create(path: &std::path::Path, max_regions: usize) -> Result<Self> {
        Ok(Self {
            out: Mutex::new(std::io::BufWriter::new(std::fs::File::create(path)?)),
            max_regions,
            count: AtomicU64::new(0),
        })
    }
    fn write(&self, payload: &[u8]) -> Result<()> {
        use std::io::Write;
        let mut hdr = Vec::with_capacity(10);
        put_uvarint(&mut hdr, payload.len() as u64);
        let mut o = self.out.lock().unwrap();
        o.write_all(&hdr)?;
        o.write_all(payload)?;
        self.count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    pub fn finish(self) -> Result<()> {
        use std::io::Write;
        self.out.into_inner().unwrap().flush()?;
        Ok(())
    }
}

struct PolyEmitter<'a> {
    large: &'a LargePolys,
    grid: &'a TileGrid,
    region_used: &'a [AtomicBool],
    regions: Vec<u32>,
    buf: Vec<u8>,
    count: [u64; 2],
}

impl PolyEmitter<'_> {
    fn emit(
        &mut self,
        local: &mut super::buckets::LocalBuckets<'_>,
        residential: bool,
        military: bool,
        key: PolyKey,
        rings: Vec<LineString<f64>>,
    ) -> Result<()> {
        let mut it = rings.into_iter();
        let exterior = it.next().unwrap();
        let holes: Vec<LineString<f64>> = it.collect();
        let enc_rings: Vec<Vec<(i32, i32)>> = std::iter::once(&exterior)
            .chain(holes.iter())
            .map(from_linestring)
            .collect();
        let mp = MultiPolygon::new(vec![Polygon::new(exterior, holes)]);
        let bbox = compute_multipolygon_bbox(&mp);
        self.grid.regions_for_bbox(
            bbox.lat_min,
            bbox.lat_max,
            bbox.lon_min,
            bbox.lon_max,
            POLY_REGION_MARGIN_DEG,
            lon_degrees_for(
                POLY_REGION_MARGIN_DEG,
                bbox.lat_min as f64 - POLY_REGION_MARGIN_DEG,
                bbox.lat_max as f64 + POLY_REGION_MARGIN_DEG,
            ),
            &mut self.regions,
        );
        self.regions
            .retain(|r| self.region_used[*r as usize].load(Ordering::Relaxed));
        if self.regions.is_empty() {
            return Ok(());
        }
        let bbox_arr = [bbox.lat_min, bbox.lat_max, bbox.lon_min, bbox.lon_max];
        for (kind, on) in [(POLY_RESIDENTIAL, residential), (POLY_MILITARY, military)] {
            if !on {
                continue;
            }
            // area / centroid are computed by the local stage itself
            let (area, centroid) = (0.0f32, (0.0f64, 0.0f64));
            let meta = PolyMeta {
                kind,
                key,
                area,
                centroid,
                bbox: bbox_arr,
            };
            self.buf.clear();
            encode_polygon(&mut self.buf, &meta, &enc_rings);
            if self.regions.len() > self.large.max_regions {
                self.large.write(&self.buf)?;
            } else {
                for &r in &self.regions {
                    local.push(r, REC_POLY, &self.buf)?;
                }
            }
            self.count[kind as usize] += 1;
        }
        Ok(())
    }
}

/// Build residential / military polygons from closed area ways and area relations and
/// distribute them to the regions that hold routable data near them.
#[hotpath::measure]
pub fn distribute_polygons(
    spill: &SpillReader,
    resolver: &Resolver,
    grid: &TileGrid,
    buckets: &RegionBuckets,
    areas: &[AreaRel],
    unresolved: &HashSet<u64>,
    region_used: &[AtomicBool],
    large: &LargePolys,
) -> Result<[u64; 2]> {
    let members: Mutex<IdMap<MemberWay>> = Mutex::new(IdMap::default());
    let counts = Mutex::new([0u64; 2]);

    resolver.for_each_frame(
        spill,
        "area ways",
        area_frame_refs,
        || buckets.local(),
        |local, _fi, frame, frame_coords| -> Result<()> {
            let mut ci = 0usize;
            let mut em = PolyEmitter {
                grid,
                region_used,
                regions: Vec::new(),
                buf: Vec::new(),
                count: [0; 2],
                large,
            };
            let mut local_members = Vec::new();
            let mut pos = 0;
            while pos < frame.len() {
                let id = get_uvarint(&frame, &mut pos);
                let flags = frame[pos];
                pos += 1;
                let n = get_uvarint(&frame, &mut pos) as usize;
                let mut acc = 0i64;
                let mut first = 0i64;
                let mut coords = Vec::with_capacity(n);
                for i in 0..n {
                    acc += get_svarint(&frame, &mut pos);
                    if i == 0 {
                        first = acc;
                    }
                    if let Some(c) = dec_coord(frame_coords[ci]) {
                        coords.push(c);
                    }
                    ci += 1;
                }
                let ids_closed = n >= 4 && first == acc;
                if flags & (AREA_RES | AREA_MIL) != 0 && coords.len() >= 4 {
                    em.emit(
                        local,
                        flags & AREA_RES != 0,
                        flags & AREA_MIL != 0,
                        (0, id, 0),
                        vec![to_linestring(&coords)],
                    )?;
                }
                if flags & AREA_MEMBER != 0 {
                    local_members.push((id, MemberWay { ids_closed, coords }));
                }
            }
            if !local_members.is_empty() {
                members.lock().unwrap().extend(local_members);
            }
            let mut c = counts.lock().unwrap();
            c[0] += em.count[0];
            c[1] += em.count[1];
            Ok(())
        },
    )?;

    let members = members.into_inner().unwrap();
    let mut areas: Vec<&AreaRel> = areas
        .iter()
        .filter(|a| !unresolved.contains(&a.id))
        .collect();
    areas.sort_by_key(|a| a.id);
    let rel_counts = areas
        .par_iter()
        .try_fold(
            || ([0u64; 2], buckets.local()),
            |(mut acc, mut local), rel| -> Result<([u64; 2], _)> {
                let mut em = PolyEmitter {
                    grid,
                    region_used,
                    regions: Vec::new(),
                    buf: Vec::new(),
                    count: [0; 2],
                    large,
                };
                let mut k = 0u32;
                if !rel.multipolygon {
                    for &(way_id, _) in &rel.ways {
                        if let Some(w) = members.get(&way_id) {
                            if w.ids_closed && w.coords.len() >= 4 {
                                em.emit(
                                    &mut local,
                                    rel.residential,
                                    rel.military,
                                    (1, rel.id, k),
                                    vec![to_linestring(&w.coords)],
                                )?;
                                k += 1;
                            }
                        }
                    }
                } else {
                    let mut outer = Vec::new();
                    let mut inner = Vec::new();
                    for &(way_id, role) in &rel.ways {
                        let Some(w) = members.get(&way_id) else {
                            continue;
                        };
                        if w.coords.len() < 2 {
                            continue;
                        }
                        match role {
                            ROLE_OUTER => outer.push(to_linestring(&w.coords)),
                            ROLE_INNER => inner.push(to_linestring(&w.coords)),
                            _ => {}
                        }
                    }
                    let outer = connect_ways_into_rings(outer);
                    let inner = connect_ways_into_rings(inner);
                    for outer_ring in outer {
                        if outer_ring.0.len() < 4 {
                            continue;
                        }
                        let outer_polygon = Polygon::new(outer_ring.clone(), vec![]);
                        let holes: Vec<LineString<f64>> = inner
                            .iter()
                            .filter(|r| r.0.len() >= 4)
                            .filter(|r| {
                                r.0.first()
                                    .map(|c| outer_polygon.contains(&Point::new(c.x, c.y)))
                                    .unwrap_or(false)
                            })
                            .cloned()
                            .collect();
                        let mut rings = vec![outer_ring];
                        rings.extend(holes);
                        em.emit(
                            &mut local,
                            rel.residential,
                            rel.military,
                            (1, rel.id, k),
                            rings,
                        )?;
                        k += 1;
                    }
                }
                acc[0] += em.count[0];
                acc[1] += em.count[1];
                Ok((acc, local))
            },
        )
        .map(|r| r.map(|(c, _local)| c))
        .try_reduce(|| [0u64; 2], |a, b| Ok([a[0] + b[0], a[1] + b[1]]))?;
    let c = counts.into_inner().unwrap();
    let total = [c[0] + rel_counts[0], c[1] + rel_counts[1]];
    info!(
        "Distributed {} residential and {} military polygons ({} member ways)",
        total[0],
        total[1],
        members.len()
    );
    Ok(total)
}
