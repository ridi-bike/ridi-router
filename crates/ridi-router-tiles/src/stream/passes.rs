//! The three PBF passes: relations -> ways -> nodes.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{bail, Result};
use rayon::prelude::*;
use tracing::info;

use super::buckets::SpillWriter;
use super::nodeidx::{enc_coord, NeededSet, RankedSet, SlotCursor};
use super::pbf::{
    classify_strings, lossy, packed_sint, parse_node, Block, DenseNodes, Packed, PbfFile,
    RelationMsg, WayMsg, KIND_NODE, KIND_RELATION, KIND_WAY,
};
use super::records::*;
use super::store::Store;
use super::util::*;

// ---------------------------------------------------------------------------------------------
// Relations
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
pub struct RelationData {
    /// Ids of all relations the reference keeps (multipolygon, restriction, residential, military).
    pub kept: Vec<u64>,
    /// Kept relations that have relation members: id -> member relation ids.
    pub children: HashMap<u64, Vec<u64>>,
    pub restrictions: Vec<RestrictionRel>,
    pub areas: Vec<AreaRel>,
}

const RK_TYPE: u8 = 1;
const RK_LANDUSE: u8 = 2;
const RK_MILITARY: u8 = 3;
const RK_EXCEPT: u8 = 4;
const RK_RESTRICTION: u8 = 5;
const RK_RESTRICTION_PREFIX: u8 = 6;

fn classify_relation_key(s: &[u8]) -> u8 {
    match s {
        b"type" => RK_TYPE,
        b"landuse" => RK_LANDUSE,
        b"military" => RK_MILITARY,
        b"except" => RK_EXCEPT,
        b"restriction" => RK_RESTRICTION,
        _ if s.starts_with(b"restriction:") => RK_RESTRICTION_PREFIX,
        _ => 0,
    }
}

fn check_order(block: &Block, idx: u64, ranges: &[std::ops::Range<u64>; 3]) -> Result<()> {
    let mut present = [false; 3];
    block.for_each_member(|field, bytes| {
        let k = match field {
            1 => KIND_NODE,
            2 => {
                if DenseNodes::parse(bytes)?.ids.is_empty() {
                    return Ok(());
                }
                KIND_NODE
            }
            3 => KIND_WAY,
            _ => KIND_RELATION,
        };
        present[k as usize] = true;
        Ok(())
    })?;
    for kind in 0..3 {
        if present[kind] && !ranges[kind].contains(&idx) {
            bail!("PBF is not sorted by type then id (blob {idx} has unexpected entity kind {kind}); sort it with `osmium sort` first");
        }
    }
    Ok(())
}

#[hotpath::measure]
pub fn relation_pass(pbf: &PbfFile, ranges: &[std::ops::Range<u64>; 3]) -> Result<RelationData> {
    let parts: Vec<(u64, RelationData)> = pbf
        .par_blocks(ranges[2].clone(), "relations")
        .map(|item| -> Result<(u64, RelationData)> {
            let (bi, raw) = item?;
            let block = Block::parse(&raw)?;
            check_order(&block, bi, ranges)?;
            let codes = classify_strings(&block, classify_relation_key);
            let st = &block.strings;
            let mut out = RelationData::default();
            block.for_each_member(|field, bytes| -> Result<()> {
                if field != 4 {
                    return Ok(());
                }
                let rel = RelationMsg::parse(bytes)?;
                {
                    let id = rel.id as u64;
                    let mut rtype: Option<&[u8]> = None;
                    let mut landuse: Option<&[u8]> = None;
                    let mut military = false;
                    let mut has_restr_tags = false;
                    for (k, v) in Packed::new(rel.keys).zip(Packed::new(rel.vals)) {
                        match codes[k as usize] {
                            RK_TYPE => rtype = Some(st[v as usize]),
                            RK_LANDUSE => landuse = Some(st[v as usize]),
                            RK_MILITARY => military = true,
                            RK_EXCEPT | RK_RESTRICTION | RK_RESTRICTION_PREFIX => {
                                has_restr_tags = true
                            }
                            _ => {}
                        }
                    }
                    let _ = has_restr_tags;
                    let multipolygon = rtype == Some(b"multipolygon");
                    let restriction = rtype == Some(b"restriction");
                    let residential = landuse == Some(b"residential");
                    let mil = landuse == Some(b"military") || military;
                    if !(multipolygon || restriction || residential || mil) {
                        return Ok(());
                    }
                    out.kept.push(id);

                    // decode members
                    let mut m = 0i64;
                    let mut members = Vec::new();
                    for ((dm, t), role) in packed_sint(rel.memids)
                        .zip(Packed::new(rel.types))
                        .zip(Packed::new(rel.roles))
                    {
                        m += dm;
                        let t = match t {
                            0 => 0u8,
                            1 => 1,
                            _ => 2,
                        };
                        members.push((t, role as usize, m as u64));
                    }
                    let child_rels: Vec<u64> = members
                        .iter()
                        .filter(|(t, _, _)| *t == 2)
                        .map(|(_, _, r)| *r)
                        .collect();
                    if !child_rels.is_empty() {
                        out.children.insert(id, child_rels);
                    }

                    if restriction {
                        let mut tags: Vec<(String, String)> = Vec::new();
                        for (k, v) in Packed::new(rel.keys).zip(Packed::new(rel.vals)) {
                            if matches!(
                                codes[k as usize],
                                RK_TYPE | RK_EXCEPT | RK_RESTRICTION | RK_RESTRICTION_PREFIX
                            ) {
                                let key = lossy(st[k as usize]);
                                let val = lossy(st[v as usize]);
                                // last value wins for duplicate keys
                                tags.retain(|(kk, _)| *kk != key);
                                tags.push((key, val));
                            }
                        }
                        let members = members
                            .iter()
                            .map(|&(t, role, r)| {
                                let role = match st[role] {
                                    b"from" => 0u8,
                                    b"to" => 1,
                                    b"via" => 2,
                                    _ => 3,
                                };
                                (t, role, r)
                            })
                            .collect();
                        out.restrictions.push(RestrictionRel { id, tags, members });
                    }
                    if residential || mil {
                        let ways = members
                            .iter()
                            .filter(|(t, _, _)| *t == 1)
                            .map(|&(_, role, r)| {
                                let role = match st[role] {
                                    b"outer" | b"" => ROLE_OUTER,
                                    b"inner" => ROLE_INNER,
                                    _ => ROLE_OTHER,
                                };
                                (r, role)
                            })
                            .collect();
                        out.areas.push(AreaRel {
                            id,
                            residential,
                            military: mil,
                            multipolygon,
                            ways,
                        });
                    }
                }
                Ok(())
            })?;
            Ok((bi, out))
        })
        .collect::<Result<Vec<_>>>()?;

    let mut parts = parts;
    parts.sort_by_key(|(bi, _)| *bi);
    let mut data = RelationData::default();
    for (_, p) in parts {
        data.kept.extend(p.kept);
        data.children.extend(p.children);
        data.restrictions.extend(p.restrictions);
        data.areas.extend(p.areas);
    }
    data.kept.sort_unstable();
    info!(
        "Relations: {} kept, {} with relation members, {} restrictions, {} residential/military areas",
        data.kept.len(),
        data.children.len(),
        data.restrictions.len(),
        data.areas.len()
    );
    Ok(data)
}

/// Reproduce `compute_relation_bboxes_iterative`: a kept relation resolves in round 1 if it has
/// no relation members, otherwise in the round after all its relation members resolved (which
/// must themselves be kept relations); at most 10 rounds.
pub fn resolvable_relations(data: &RelationData) -> HashSet<u64> {
    let is_kept = |id: u64| data.kept.binary_search(&id).is_ok();
    // round in which each relation with children resolved (others resolve in round 1)
    let mut resolved_round: HashMap<u64, u32> = HashMap::new();
    let round_of = |id: u64, rr: &HashMap<u64, u32>| -> Option<u32> {
        if !is_kept(id) {
            None
        } else if data.children.contains_key(&id) {
            rr.get(&id).copied()
        } else {
            Some(1)
        }
    };
    for round in 2..=10u32 {
        let mut newly = Vec::new();
        for (id, children) in &data.children {
            if resolved_round.contains_key(id) {
                continue;
            }
            if children
                .iter()
                .all(|c| round_of(*c, &resolved_round).is_some_and(|r| r < round))
            {
                newly.push(*id);
            }
        }
        if newly.is_empty() {
            break;
        }
        for id in newly {
            resolved_round.insert(id, round);
        }
    }
    let mut unresolved = HashSet::new();
    for id in data.children.keys() {
        if !resolved_round.contains_key(id) {
            unresolved.insert(*id);
        }
    }
    if !unresolved.is_empty() {
        info!(
            "{} kept relations cannot be resolved (missing/circular/too deep relation members)",
            unresolved.len()
        );
    }
    unresolved
}

// ---------------------------------------------------------------------------------------------
// Ways
// ---------------------------------------------------------------------------------------------

const ALLOWED_HIGHWAY_VALUES: [&[u8]; 17] = [
    b"motorway",
    b"trunk",
    b"primary",
    b"secondary",
    b"tertiary",
    b"unclassified",
    b"residential",
    b"motorway_link",
    b"trunk_link",
    b"primary_link",
    b"secondary_link",
    b"tertiary_link",
    b"living_street",
    b"track",
    b"escape",
    b"raceway",
    b"road",
];

const WK_HIGHWAY: u8 = 1;
const WK_MOTORCYCLE: u8 = 2;
const WK_LANDUSE: u8 = 3;
const WK_MILITARY: u8 = 4;
const WK_NAME: u8 = 5;
const WK_REF: u8 = 6;
const WK_SURFACE: u8 = 7;
const WK_SMOOTHNESS: u8 = 8;
const WK_ONEWAY: u8 = 9;
const WK_JUNCTION: u8 = 10;

fn classify_way_key(s: &[u8]) -> u8 {
    match s {
        b"highway" => WK_HIGHWAY,
        b"motorcycle" => WK_MOTORCYCLE,
        b"landuse" => WK_LANDUSE,
        b"military" => WK_MILITARY,
        b"name" => WK_NAME,
        b"ref" => WK_REF,
        b"surface" => WK_SURFACE,
        b"smoothness" => WK_SMOOTHNESS,
        b"oneway" => WK_ONEWAY,
        b"junction" => WK_JUNCTION,
        _ => 0,
    }
}

pub const AREA_RES: u8 = 1;
pub const AREA_MIL: u8 = 2;
pub const AREA_MEMBER: u8 = 4;

pub struct WayPassStats {
    pub routable: u64,
    pub area: u64,
}

/// Routable spill record: id, tags, node ids (delta). Area spill record: id, flags, node ids.
#[hotpath::measure]
pub fn way_pass(
    pbf: &PbfFile,
    ranges: &[std::ops::Range<u64>; 3],
    area_members: &super::util::IdSet,
    needed: &NeededSet,
    routable_out: &SpillWriter,
    area_out: &SpillWriter,
) -> Result<WayPassStats> {
    let n_routable = AtomicU64::new(0);
    let n_area = AtomicU64::new(0);
    pbf.par_blocks(ranges[1].clone(), "ways")
        .try_for_each_init(
            || (Vec::<i64>::new(), Vec::<i64>::new()),
            |(refs, block_ids), item| -> Result<()> {
                let (bi, raw) = item?;
                let block = Block::parse(&raw)?;
                check_order(&block, bi, ranges)?;
                let codes = classify_strings(&block, classify_way_key);
                let st = &block.strings;
                let mut rbuf = Vec::with_capacity(1 << 20);
                let mut abuf = Vec::new();
                block_ids.clear();
                block.for_each_member(|field, bytes| -> Result<()> {
                    if field != 3 {
                        return Ok(());
                    }
                    let way = WayMsg::parse(bytes)?;
                    let id = way.id as u64;
                    let mut vals: [Option<&[u8]>; 11] = [None; 11];
                    for (k, v) in Packed::new(way.keys).zip(Packed::new(way.vals)) {
                        let c = codes[k as usize];
                        if c != 0 {
                            vals[c as usize] = Some(st[v as usize]);
                        }
                    }
                    let highway = vals[WK_HIGHWAY as usize];
                    let routable = highway.is_some_and(|h| {
                        ALLOWED_HIGHWAY_VALUES.contains(&h)
                            || (h == b"path" && vals[WK_MOTORCYCLE as usize] == Some(b"yes"))
                    });
                    let landuse = vals[WK_LANDUSE as usize];
                    let res = landuse == Some(b"residential");
                    let mil = landuse == Some(b"military") || vals[WK_MILITARY as usize].is_some();
                    let member = area_members.contains(&id);
                    if !routable && !res && !mil && !member {
                        return Ok(());
                    }
                    refs.clear();
                    let mut acc = 0i64;
                    for d in packed_sint(way.refs) {
                        acc += d;
                        refs.push(acc);
                    }
                    let closed = refs.len() >= 4 && refs[0] == refs[refs.len() - 1];
                    let area = ((res || mil) && closed) || member;
                    if !routable && !area {
                        return Ok(());
                    }
                    block_ids.extend_from_slice(refs);
                    let put_refs = |buf: &mut Vec<u8>| {
                        put_uvarint(buf, refs.len() as u64);
                        let mut prev = 0i64;
                        for &r in refs.iter() {
                            put_svarint(buf, r - prev);
                            prev = r;
                        }
                    };
                    if routable {
                        n_routable.fetch_add(1, Ordering::Relaxed);
                        put_uvarint(&mut rbuf, id);
                        let tags: [Option<&[u8]>; 7] = [
                            vals[WK_NAME as usize],
                            vals[WK_REF as usize],
                            vals[WK_HIGHWAY as usize],
                            vals[WK_SURFACE as usize],
                            vals[WK_SMOOTHNESS as usize],
                            vals[WK_ONEWAY as usize],
                            vals[WK_JUNCTION as usize],
                        ];
                        put_way_tags(&mut rbuf, &tags);
                        put_refs(&mut rbuf);
                    }
                    if area {
                        n_area.fetch_add(1, Ordering::Relaxed);
                        put_uvarint(&mut abuf, id);
                        let mut flags = 0u8;
                        if res && closed {
                            flags |= AREA_RES;
                        }
                        if mil && closed {
                            flags |= AREA_MIL;
                        }
                        if member {
                            flags |= AREA_MEMBER;
                        }
                        abuf.push(flags);
                        put_refs(&mut abuf);
                    }
                    Ok(())
                })?;
                needed.insert_batch(block_ids)?;
                routable_out.write_batch(&rbuf)?;
                area_out.write_batch(&abuf)?;
                Ok(())
            },
        )?;
    let stats = WayPassStats {
        routable: n_routable.load(Ordering::Relaxed),
        area: n_area.load(Ordering::Relaxed),
    };
    info!(
        "Ways: {} routable, {} area/area-member ways spilled",
        stats.routable, stats.area
    );
    Ok(stats)
}

/// Node ids of one routable-spill frame, in record order.
pub fn routable_frame_refs(frame: &[u8], out: &mut Vec<i64>) {
    let mut pos = 0;
    while pos < frame.len() {
        let _id = get_uvarint(frame, &mut pos);
        let mut sink = Vec::new();
        copy_way_tags(frame, &mut pos, &mut sink);
        let n = get_uvarint(frame, &mut pos) as usize;
        let mut acc = 0i64;
        for _ in 0..n {
            acc += get_svarint(frame, &mut pos);
            out.push(acc);
        }
    }
}

/// Node ids of one area-spill frame, in record order.
pub fn area_frame_refs(frame: &[u8], out: &mut Vec<i64>) {
    let mut pos = 0;
    while pos < frame.len() {
        let _id = get_uvarint(frame, &mut pos);
        pos += 1;
        let n = get_uvarint(frame, &mut pos) as usize;
        let mut acc = 0i64;
        for _ in 0..n {
            acc += get_svarint(frame, &mut pos);
            out.push(acc);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Nodes
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct NodeBounds {
    pub lat_min: i32,
    pub lat_max: i32,
    pub lon_min: i32,
    pub lon_max: i32,
    pub count: u64,
    pub stored: u64,
}

impl NodeBounds {
    fn empty() -> Self {
        Self {
            lat_min: i32::MAX,
            lat_max: i32::MIN,
            lon_min: i32::MAX,
            lon_max: i32::MIN,
            count: 0,
            stored: 0,
        }
    }
    fn merge(self, o: Self) -> Self {
        Self {
            lat_min: self.lat_min.min(o.lat_min),
            lat_max: self.lat_max.max(o.lat_max),
            lon_min: self.lon_min.min(o.lon_min),
            lon_max: self.lon_max.max(o.lon_max),
            count: self.count + o.count,
            stored: self.stored + o.stored,
        }
    }
    #[inline]
    fn add(&mut self, lat: i32, lon: i32) {
        self.lat_min = self.lat_min.min(lat);
        self.lat_max = self.lat_max.max(lat);
        self.lon_min = self.lon_min.min(lon);
        self.lon_max = self.lon_max.max(lon);
        self.count += 1;
    }
}

#[hotpath::measure]
pub fn node_pass(
    pbf: &PbfFile,
    ranges: &[std::ops::Range<u64>; 3],
    set: &RankedSet,
    store: &Store,
) -> Result<NodeBounds> {
    let bounds = pbf
        .par_blocks(ranges[0].clone(), "nodes")
        .map_init(
            || Vec::<u64>::new(),
            |run, item| -> Result<NodeBounds> {
                let (bi, raw) = item?;
                let block = Block::parse(&raw)?;
                check_order(&block, bi, ranges)?;
                let mut b = NodeBounds::empty();
                // contiguous slot runs
                let mut run_start = 0u64;
                run.clear();
                let mut cursor = SlotCursor::default();
                let mut visit = |id: i64, lat: i32, lon: i32, b: &mut NodeBounds| -> Result<()> {
                    b.add(lat, lon);
                    if let Some(slot) = set.slot_seq(id, &mut cursor) {
                        if !run.is_empty() && slot != run_start + run.len() as u64 {
                            store.write_run(run_start, run)?;
                            run.clear();
                        }
                        if run.is_empty() {
                            run_start = slot;
                        }
                        run.push(enc_coord(lat, lon));
                        b.stored += 1;
                    }
                    Ok(())
                };
                block.for_each_member(|field, bytes| -> Result<()> {
                    match field {
                        1 => {
                            let (id, la, lo) = parse_node(bytes)?;
                            visit(id, block.coord(la, true), block.coord(lo, false), &mut b)?;
                        }
                        2 => {
                            let d = DenseNodes::parse(bytes)?;
                            let (mut id, mut la, mut lo) = (0i64, 0i64, 0i64);
                            for ((did, dla), dlo) in packed_sint(d.ids)
                                .zip(packed_sint(d.lats))
                                .zip(packed_sint(d.lons))
                            {
                                id += did;
                                la += dla;
                                lo += dlo;
                                visit(id, block.coord(la, true), block.coord(lo, false), &mut b)?;
                            }
                        }
                        _ => {}
                    }
                    Ok(())
                })?;
                if !run.is_empty() {
                    store.write_run(run_start, run)?;
                }
                Ok(b)
            },
        )
        .try_reduce(NodeBounds::empty, |a, b| Ok(a.merge(b)))?;
    info!(
        "Nodes: {} total, {} stored (of {} needed)",
        bounds.count, bounds.stored, set.count
    );
    Ok(bounds)
}
