//! Semantic comparison of RMDF tile directories.
//!
//! Tile bytes are not deterministic (line, tag and rule order), so tiles are decoded and compared
//! as models: points (coordinates, flags, incident lines), lines (endpoints, direction, tags) and
//! restriction rules. Decoding also checks each tile's internal consistency (layout, checksum,
//! spatial index, references) and a few generator invariants (points inside the tile's buffered
//! bounds, no line across the antimeridian).
//!
//! * [`diff_dirs`] compares a candidate directory against a reference.
//! * [`digest_dir`] reduces a directory to one semantic hash per non-empty tile (golden tests).

use anyhow::{bail, Context, Result};
use bytemuck::{pod_read_unaligned, Pod};
use rayon::prelude::*;
use ridi_router_common::format::*;
use ridi_router_common::manifest::TileManifest;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

/// Resolved tag strings: name, ref, highway, surface, smoothness.
type Tags = [Option<Box<str>>; 5];
const TAG_NAMES: [&str; 5] = ["name", "ref", "highway", "surface", "smoothness"];

/// Buffer (degrees) around a tile's bounds within which the generator includes points.
const TILE_BUFFER_DEG: f32 = 0.005;

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct LineKey {
    a: u64,
    b: u64,
    dir: u8,
    /// f32 bit patterns of endpoint coordinates (a_lat, a_lon, b_lat, b_lon)
    coords: [u32; 4],
    tags: Arc<Tags>,
}

impl fmt::Display for LineKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}->{} dir={}", self.a, self.b, self.dir)?;
        for (i, t) in self.tags.iter().enumerate() {
            if let Some(v) = t {
                write!(f, " {}={:?}", TAG_NAMES[i], v)?;
            }
        }
        write!(
            f,
            " a=({},{}) b=({},{})",
            f32::from_bits(self.coords[0]),
            f32::from_bits(self.coords[1]),
            f32::from_bits(self.coords[2]),
            f32::from_bits(self.coords[3])
        )
    }
}

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct RuleKey {
    via: u64,
    rule_type: u8,
    from: Vec<LineKey>,
    to: Vec<LineKey>,
}

impl fmt::Display for RuleKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let j = |v: &Vec<LineKey>| {
            v.iter()
                .map(|l| format!("[{}->{}]", l.a, l.b))
                .collect::<Vec<_>>()
                .join(",")
        };
        write!(
            f,
            "via={} type={} from={} to={}",
            self.via,
            self.rule_type,
            j(&self.from),
            j(&self.to)
        )
    }
}

struct PointSem {
    lat: f32,
    lon: f32,
    flags: u16,
    incident: Vec<LineKey>, // sorted multiset
}

struct Model {
    bounds: [u32; 4],
    points: HashMap<u64, PointSem>,
    lines: HashMap<LineKey, u32>,
    rules: HashMap<RuleKey, u32>,
    rule_count: u64,
}

/// Consistency issues found while decoding (counted by category, with a few examples).
#[derive(Default)]
struct Issues {
    counts: BTreeMap<&'static str, u64>,
    examples: BTreeMap<&'static str, Vec<String>>,
    max_examples: usize,
}

impl Issues {
    fn new(max_examples: usize) -> Self {
        Self {
            max_examples,
            ..Default::default()
        }
    }
    fn add(&mut self, cat: &'static str, msg: impl FnOnce() -> String) {
        *self.counts.entry(cat).or_default() += 1;
        let ex = self.examples.entry(cat).or_default();
        if ex.len() < self.max_examples {
            ex.push(msg());
        }
    }
    fn total(&self) -> u64 {
        self.counts.values().sum()
    }
}

fn slice_at<'a>(bytes: &'a [u8], off: usize, len: usize, what: &str) -> Result<&'a [u8]> {
    bytes
        .get(off..off.checked_add(len).context("overflow")?)
        .with_context(|| {
            format!(
                "{what} out of bounds (off={off} len={len} file={})",
                bytes.len()
            )
        })
}

fn read_pods<T: Pod>(bytes: &[u8], off: usize, count: usize, what: &str) -> Result<Vec<T>> {
    let sz = std::mem::size_of::<T>();
    let s = slice_at(bytes, off, count.checked_mul(sz).context("overflow")?, what)?;
    Ok(s.chunks_exact(sz).map(pod_read_unaligned).collect())
}

fn read_u64s(bytes: &[u8], off: usize, end: usize, what: &str) -> Result<Vec<u64>> {
    if end < off {
        bail!("{what}: end {end} < start {off}");
    }
    let s = slice_at(bytes, off, end - off, what)?;
    if s.len() % 8 != 0 {
        bail!("{what}: size {} not a multiple of 8", s.len());
    }
    Ok(s.as_chunks::<8>()
        .0
        .iter()
        .map(|c| u64::from_le_bytes(*c))
        .collect())
}

fn parse_tile_name(name: &str) -> Option<(u16, u16)> {
    let s = name.strip_prefix("tile_")?.strip_suffix(".rmdf")?;
    let (c, r) = s.split_once('_')?;
    Some((c.parse().ok()?, r.parse().ok()?))
}

fn read_header(bytes: &[u8]) -> Result<RmdfHeader> {
    if bytes.len() < RmdfHeader::SIZE {
        bail!("file too small for header ({} bytes)", bytes.len());
    }
    let h: RmdfHeader = pod_read_unaligned(&bytes[..RmdfHeader::SIZE]);
    if h.magic != RmdfHeader::MAGIC {
        bail!("bad magic {:?}", h.magic);
    }
    if h.version != RmdfHeader::VERSION {
        bail!("unsupported version {}", h.version);
    }
    Ok(h)
}

fn is_empty_header(h: &RmdfHeader) -> bool {
    h.point_count == 0 && h.line_count == 0
}

/// Decode a tile into a semantic model, recording any internal inconsistency in `issues`.
fn decode(name: &str, bytes: &[u8], tile_size: f32, issues: &mut Issues) -> Result<Model> {
    let h = read_header(bytes)?;

    // Checksum (trailing SHA-256 of all preceding bytes).
    let mut data_end = bytes.len();
    if bytes.len() >= RmdfHeader::SIZE + 32 {
        let (data, sum) = bytes.split_at(bytes.len() - 32);
        if Sha256::digest(data).as_slice() == sum {
            data_end = bytes.len() - 32;
        } else {
            issues.add("checksum_mismatch", || {
                format!("{name}: trailing sha256 mismatch")
            });
        }
    } else {
        issues.add("checksum_missing", || {
            format!("{name}: no room for checksum")
        });
    }

    // Header bounds vs filename.
    if let Some((col, row)) = parse_tile_name(name) {
        let lon_min = col as f32 * tile_size - 180.0;
        let lat_min = row as f32 * tile_size - 90.0;
        let b = &h.tile_bounds;
        let tol = 1e-3;
        if (b.lon_min - lon_min).abs() > tol
            || (b.lat_min - lat_min).abs() > tol
            || (b.lon_max - (lon_min + tile_size)).abs() > tol
            || (b.lat_max - (lat_min + tile_size)).abs() > tol
        {
            issues.add("header_bounds_vs_filename", || {
                format!("{name}: header bounds {:?} do not match filename", b)
            });
        }
    } else {
        issues.add("bad_filename", || format!("{name}: cannot parse tile id"));
    }

    let so = |i: usize| h.section_offsets[i] as usize;
    let pc = h.point_count as usize;
    let lc = h.line_count as usize;
    let gsz = std::mem::size_of::<GridCellEntry>();
    let psz = std::mem::size_of::<PointRecord>();
    let lsz = std::mem::size_of::<LineRecord>();
    let tsz = std::mem::size_of::<TagSetRecord>();
    let ssz = std::mem::size_of::<StringEntry>();

    // Layout: sections must be contiguous in the order the writer emits them.
    let expect = [
        (section::SPATIAL_INDEX, RmdfHeader::SIZE),
        (
            section::POINTS,
            so(section::SPATIAL_INDEX) + h.spatial_grid_cell_count as usize * gsz,
        ),
        (section::LINES, so(section::POINTS) + pc * psz),
        (section::LINE_REFS, so(section::LINES) + lc * lsz),
    ];
    for (sec, want) in expect {
        if so(sec) != want {
            issues.add("section_layout", || {
                format!("{name}: section {sec} offset {} expected {want}", so(sec))
            });
        }
    }
    for i in 1..NUM_SECTIONS {
        if so(i) < so(i - 1) || so(i) > data_end {
            issues.add("section_layout", || {
                format!("{name}: section {i} offset {} out of order/bounds", so(i))
            });
        }
    }
    if so(section::TAG_SETS) + h.tag_set_count as usize * tsz != so(section::RULES) {
        issues.add("section_layout", || {
            format!("{name}: tag sets do not end at rules section")
        });
    }

    let grid: Vec<GridCellEntry> = read_pods(
        bytes,
        so(section::SPATIAL_INDEX),
        h.spatial_grid_cell_count as usize,
        "spatial index",
    )?;
    let points: Vec<PointRecord> = read_pods(bytes, so(section::POINTS), pc, "points")?;
    let lines: Vec<LineRecord> = read_pods(bytes, so(section::LINES), lc, "lines")?;
    let line_refs = read_u64s(
        bytes,
        so(section::LINE_REFS),
        so(section::TAG_VALUES),
        "line refs",
    )?;

    // Generator invariants: every point lies in the tile bounds grown by the tile buffer (clamped
    // to the world), and no line crosses the antimeridian (OSM splits ways there).
    {
        let b = &h.tile_bounds;
        let buf = TILE_BUFFER_DEG + 1e-5;
        let (la0, la1) = ((b.lat_min - buf).max(-90.0), (b.lat_max + buf).min(90.0));
        let (lo0, lo1) = ((b.lon_min - buf).max(-180.0), (b.lon_max + buf).min(180.0));
        for p in &points {
            if !(p.lat >= la0 && p.lat <= la1 && p.lon >= lo0 && p.lon <= lo1) {
                issues.add("point_outside_buffered_bounds", || {
                    format!("{name}: point {} at ({}, {})", p.osm_id, p.lat, p.lon)
                });
            }
        }
        for l in &lines {
            if (l.point_a_lon - l.point_b_lon).abs() > 180.0 {
                issues.add("line_crosses_antimeridian", || {
                    format!("{name}: line {}->{}", l.point_a_osm_id, l.point_b_osm_id)
                });
            }
        }
    }

    // Tag values.
    let tv_off = so(section::TAG_VALUES);
    let tvc = h.tag_value_count as usize;
    let entries: Vec<StringEntry> = read_pods(bytes, tv_off, tvc, "tag value entries")?;
    let pool_start = tv_off + tvc * ssz;
    let pool_end = so(section::TAG_SETS);
    let pool = slice_at(
        bytes,
        pool_start,
        pool_end.saturating_sub(pool_start),
        "tag string pool",
    )?;
    let mut tag_values: Vec<Option<Box<str>>> = Vec::with_capacity(tvc);
    for (i, e) in entries.iter().enumerate() {
        let s = pool
            .get(e.offset as usize..(e.offset as usize).saturating_add(e.length as usize))
            .and_then(|b| std::str::from_utf8(b).ok());
        if s.is_none() {
            issues.add("tag_value_invalid", || {
                format!("{name}: tag value {i} out of pool / bad utf8")
            });
        }
        tag_values.push(s.map(Into::into));
    }
    let tag_sets_raw: Vec<TagSetRecord> = read_pods(
        bytes,
        so(section::TAG_SETS),
        h.tag_set_count as usize,
        "tag sets",
    )?;
    let resolve = |idx: u32, issues: &mut Issues, ts: usize| -> Option<Box<str>> {
        if idx == TagSetRecord::NONE {
            return None;
        }
        match tag_values.get(idx as usize) {
            Some(v) => v.clone(),
            None => {
                issues.add("tag_value_index_oob", || {
                    format!("{name}: tag set {ts} refers to tag value {idx} >= {tvc}")
                });
                Some(format!("<invalid:{idx}>").into())
            }
        }
    };
    let tag_sets: Vec<Arc<Tags>> = tag_sets_raw
        .iter()
        .enumerate()
        .map(|(i, t)| {
            Arc::new([
                resolve(t.name_idx, issues, i),
                resolve(t.hw_ref_idx, issues, i),
                resolve(t.highway_idx, issues, i),
                resolve(t.surface_idx, issues, i),
                resolve(t.smoothness_idx, issues, i),
            ])
        })
        .collect();

    // Rules.
    let rules_raw: Vec<RuleRecord> =
        read_pods(bytes, so(section::RULES), h.rule_count as usize, "rules")?;
    let rule_refs = read_u64s(
        bytes,
        so(section::RULES) + h.rule_count as usize * std::mem::size_of::<RuleRecord>(),
        data_end,
        "rule line refs",
    )?;

    // Line keys.
    let invalid_tags: Arc<Tags> =
        Arc::new([Some("<invalid tag set>".into()), None, None, None, None]);
    let line_keys: Vec<LineKey> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let tags = match tag_sets.get(l.tag_set_index as usize) {
                Some(t) => t.clone(),
                None => {
                    issues.add("tag_set_index_oob", || {
                        format!(
                            "{name}: line {i} tag_set_index {} >= {}",
                            l.tag_set_index,
                            tag_sets.len()
                        )
                    });
                    invalid_tags.clone()
                }
            };
            if l.direction > 2 {
                issues.add("line_bad_direction", || {
                    format!("{name}: line {i} direction {}", l.direction)
                });
            }
            LineKey {
                a: l.point_a_osm_id,
                b: l.point_b_osm_id,
                dir: l.direction,
                coords: [
                    l.point_a_lat.to_bits(),
                    l.point_a_lon.to_bits(),
                    l.point_b_lat.to_bits(),
                    l.point_b_lon.to_bits(),
                ],
                tags,
            }
        })
        .collect();

    // Spatial index: sorted cells, contiguous ranges covering all points in order, correct cell ids.
    {
        let mut next = 0u64;
        let mut prev_cell: Option<u32> = None;
        for (gi, g) in grid.iter().enumerate() {
            if let Some(p) = prev_cell {
                if g.cell_id <= p {
                    issues.add("spatial_index_unsorted", || {
                        format!("{name}: grid entry {gi} cell {} <= prev {p}", g.cell_id)
                    });
                }
            }
            prev_cell = Some(g.cell_id);
            if g.points_offset != next {
                issues.add("spatial_index_gap", || {
                    format!(
                        "{name}: grid entry {gi} offset {} expected {next}",
                        g.points_offset
                    )
                });
            }
            let end = g.points_offset.saturating_add(g.points_count as u64);
            if end > pc as u64 {
                issues.add("spatial_index_oob", || {
                    format!("{name}: grid entry {gi} range end {end} > {pc}")
                });
            } else {
                for p in &points[g.points_offset as usize..end as usize] {
                    let want = GridCellEntry::encode_cell_id(p.lat, p.lon, 100);
                    if want != g.cell_id {
                        issues.add("spatial_index_wrong_cell", || {
                            format!(
                                "{name}: point {} in cell {} but encodes to {want}",
                                p.osm_id, g.cell_id
                            )
                        });
                    }
                }
            }
            next = end;
        }
        if next != pc as u64 {
            issues.add("spatial_index_incomplete", || {
                format!("{name}: spatial index covers {next} of {pc} points")
            });
        }
    }

    // Points.
    let mut point_map: HashMap<u64, PointSem> = HashMap::with_capacity(pc);
    let mut line_seen_from: Vec<u8> = vec![0; lc]; // bit0: listed at a, bit1: listed at b
    let mut rule_used = vec![false; rules_raw.len()];
    let mut rules: HashMap<RuleKey, u32> = HashMap::new();
    let line_list = |off: u64,
                     cnt: u32,
                     refs: &[u64],
                     what: &str,
                     owner: u64,
                     issues: &mut Issues|
     -> Vec<(usize, LineKey)> {
        let start = off as usize;
        let end = start.saturating_add(cnt as usize);
        let Some(s) = refs.get(start..end) else {
            issues.add("ref_range_oob", || {
                format!(
                    "{name}: {what} of {owner}: range {start}..{end} > {}",
                    refs.len()
                )
            });
            return Vec::new();
        };
        s.iter()
            .filter_map(|&r| match line_keys.get(r as usize) {
                Some(k) => Some((r as usize, k.clone())),
                None => {
                    issues.add("line_index_oob", || {
                        format!("{name}: {what} of {owner}: line index {r} >= {lc}")
                    });
                    None
                }
            })
            .collect()
    };

    for p in &points {
        let incident_idx = line_list(
            p.lines_offset,
            p.lines_count,
            &line_refs,
            "incident lines",
            p.osm_id,
            issues,
        );
        let mut incident = Vec::with_capacity(incident_idx.len());
        for (li, k) in incident_idx {
            if k.a == p.osm_id {
                line_seen_from[li] |= 1;
            }
            if k.b == p.osm_id {
                line_seen_from[li] |= 2;
            }
            if k.a != p.osm_id && k.b != p.osm_id {
                issues.add("incident_line_not_touching", || {
                    format!(
                        "{name}: point {} lists line {li} ({k}) which does not touch it",
                        p.osm_id
                    )
                });
            }
            incident.push(k);
        }
        incident.sort_unstable();

        // Rules with this point as via node.
        let rs = p.rules_offset as usize;
        let re = rs.saturating_add(p.rules_count as usize);
        if p.rules_count > 0 {
            match rules_raw.get(rs..re) {
                None => issues.add("rule_range_oob", || {
                    format!(
                        "{name}: point {} rules {rs}..{re} > {}",
                        p.osm_id,
                        rules_raw.len()
                    )
                }),
                Some(rr) => {
                    for (ri, r) in rr.iter().enumerate() {
                        if std::mem::replace(&mut rule_used[rs + ri], true) {
                            issues.add("rule_shared", || {
                                format!("{name}: rule {} referenced by multiple points", rs + ri)
                            });
                        }
                        let mut from: Vec<LineKey> = line_list(
                            r.from_lines_offset,
                            r.from_lines_count,
                            &rule_refs,
                            "rule from",
                            p.osm_id,
                            issues,
                        )
                        .into_iter()
                        .map(|x| x.1)
                        .collect();
                        let mut to: Vec<LineKey> = line_list(
                            r.to_lines_offset,
                            r.to_lines_count,
                            &rule_refs,
                            "rule to",
                            p.osm_id,
                            issues,
                        )
                        .into_iter()
                        .map(|x| x.1)
                        .collect();
                        from.sort_unstable();
                        to.sort_unstable();
                        if r.rule_type > 1 {
                            issues.add("rule_bad_type", || {
                                format!("{name}: rule type {}", r.rule_type)
                            });
                        }
                        *rules
                            .entry(RuleKey {
                                via: p.osm_id,
                                rule_type: r.rule_type,
                                from,
                                to,
                            })
                            .or_default() += 1;
                    }
                }
            }
        }

        if p.flags & !(PointRecord::RESIDENTIAL_IN_PROXIMITY_FLAG | PointRecord::NOGO_AREA_FLAG)
            != 0
        {
            issues.add("point_unknown_flags", || {
                format!("{name}: point {} flags {:#x}", p.osm_id, p.flags)
            });
        }
        if p.lines_count == 0 {
            issues.add("point_without_lines", || {
                format!("{name}: point {} has no lines", p.osm_id)
            });
        }
        let prev = point_map.insert(
            p.osm_id,
            PointSem {
                lat: p.lat,
                lon: p.lon,
                flags: p.flags,
                incident,
            },
        );
        if prev.is_some() {
            issues.add("duplicate_point", || {
                format!("{name}: point {} appears twice", p.osm_id)
            });
        }
    }
    let unused_rules = rule_used.iter().filter(|u| !**u).count();
    if unused_rules > 0 {
        issues.add("rule_unreferenced", || {
            format!("{name}: {unused_rules} rule records not referenced by any point")
        });
    }

    // Lines: endpoints must be points in this tile, coords must agree, and each line must be
    // listed in the incident sets of both endpoints.
    for (i, k) in line_keys.iter().enumerate() {
        for (end, id, lat, lon, bit) in [
            ("a", k.a, k.coords[0], k.coords[1], 1u8),
            ("b", k.b, k.coords[2], k.coords[3], 2u8),
        ] {
            match point_map.get(&id) {
                None => issues.add("line_endpoint_missing_point", || {
                    format!("{name}: line {i} endpoint {end}={id} has no point record")
                }),
                Some(p) => {
                    if p.lat.to_bits() != lat || p.lon.to_bits() != lon {
                        issues.add("line_endpoint_coord_mismatch", || {
                            format!("{name}: line {i} endpoint {end}={id} coords differ from point record")
                        });
                    }
                    if line_seen_from[i] & bit == 0 {
                        issues.add("line_not_in_endpoint_incident", || {
                            format!(
                                "{name}: line {i} ({k}) not listed in incident lines of {end}={id}"
                            )
                        });
                    }
                }
            }
        }
    }

    let mut lines_ms: HashMap<LineKey, u32> = HashMap::with_capacity(lc);
    for k in line_keys {
        *lines_ms.entry(k).or_default() += 1;
    }

    let b = &h.tile_bounds;
    Ok(Model {
        bounds: [
            b.lat_min.to_bits(),
            b.lat_max.to_bits(),
            b.lon_min.to_bits(),
            b.lon_max.to_bits(),
        ],
        points: point_map,
        lines: lines_ms,
        rules,
        rule_count: h.rule_count as u64,
    })
}

// ---------------------------------------------------------------------------
// Comparison
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Acc {
    counts: BTreeMap<&'static str, u64>,
    examples: BTreeMap<&'static str, Vec<String>>,
    max_examples: usize,
}

impl Acc {
    fn add(&mut self, cat: &'static str, n: u64, msg: impl FnOnce() -> String) {
        if n == 0 {
            return;
        }
        *self.counts.entry(cat).or_default() += n;
        let ex = self.examples.entry(cat).or_default();
        if ex.len() < self.max_examples {
            ex.push(msg());
        }
    }
    fn merge(&mut self, other: Acc) {
        for (k, v) in other.counts {
            *self.counts.entry(k).or_default() += v;
        }
        for (k, v) in other.examples {
            let ex = self.examples.entry(k).or_default();
            for e in v {
                if ex.len() < self.max_examples {
                    ex.push(e);
                }
            }
        }
    }
    fn merge_issues(&mut self, prefix: &str, iss: Issues) {
        // Leak-free static keys: map issue categories to a prefixed static string.
        for (k, v) in iss.counts {
            let key = prefixed(prefix, k);
            *self.counts.entry(key).or_default() += v;
        }
        for (k, v) in iss.examples {
            let key = prefixed(prefix, k);
            let ex = self.examples.entry(key).or_default();
            for e in v {
                if ex.len() < self.max_examples {
                    ex.push(e);
                }
            }
        }
    }
}

fn prefixed(prefix: &str, k: &'static str) -> &'static str {
    use std::sync::Mutex;
    static CACHE: Mutex<Option<HashMap<String, &'static str>>> = Mutex::new(None);
    let full = format!("{prefix}{k}");
    let mut g = CACHE.lock().unwrap();
    let m = g.get_or_insert_with(HashMap::new);
    if let Some(s) = m.get(&full) {
        return s;
    }
    let s: &'static str = Box::leak(full.clone().into_boxed_str());
    m.insert(full, s);
    s
}

fn compare(name: &str, r: &Model, c: &Model, acc: &mut Acc) {
    if r.bounds != c.bounds {
        acc.add("tile_bounds_mismatch", 1, || {
            format!("{name}: header bounds differ")
        });
    }
    for (id, rp) in &r.points {
        match c.points.get(id) {
            None => acc.add("missing_points", 1, || {
                format!(
                    "{name}: point {id} ({},{}) missing in candidate",
                    rp.lat, rp.lon
                )
            }),
            Some(cp) => {
                if rp.lat.to_bits() != cp.lat.to_bits() || rp.lon.to_bits() != cp.lon.to_bits() {
                    acc.add("point_coord_mismatch", 1, || {
                        format!(
                            "{name}: point {id} ref=({},{}) cand=({},{})",
                            rp.lat, rp.lon, cp.lat, cp.lon
                        )
                    });
                }
                let rr = rp.flags & PointRecord::RESIDENTIAL_IN_PROXIMITY_FLAG;
                let cr = cp.flags & PointRecord::RESIDENTIAL_IN_PROXIMITY_FLAG;
                if rr != cr {
                    acc.add("residential_mismatch", 1, || {
                        format!(
                            "{name}: point {id} residential ref={} cand={}",
                            rr != 0,
                            cr != 0
                        )
                    });
                }
                let rn = rp.flags & PointRecord::NOGO_AREA_FLAG;
                let cn = cp.flags & PointRecord::NOGO_AREA_FLAG;
                if rn != cn {
                    acc.add("nogo_mismatch", 1, || {
                        format!("{name}: point {id} nogo ref={} cand={}", rn != 0, cn != 0)
                    });
                }
                let mask =
                    !(PointRecord::RESIDENTIAL_IN_PROXIMITY_FLAG | PointRecord::NOGO_AREA_FLAG);
                if rp.flags & mask != cp.flags & mask {
                    acc.add("other_flag_mismatch", 1, || {
                        format!(
                            "{name}: point {id} flags ref={:#x} cand={:#x}",
                            rp.flags, cp.flags
                        )
                    });
                }
                if rp.incident != cp.incident {
                    acc.add("incident_lines_mismatch", 1, || {
                        let (only_r, only_c) = multiset_sym_diff(&rp.incident, &cp.incident);
                        format!(
                            "{name}: point {id} incident lines ref={} cand={}; only-ref: {} only-cand: {}",
                            rp.incident.len(),
                            cp.incident.len(),
                            fmt_list(&only_r),
                            fmt_list(&only_c)
                        )
                    });
                }
            }
        }
    }
    for (id, cp) in &c.points {
        if !r.points.contains_key(id) {
            acc.add("extra_points", 1, || {
                format!(
                    "{name}: point {id} ({},{}) extra in candidate",
                    cp.lat, cp.lon
                )
            });
        }
    }
    diff_multiset(
        name,
        &r.lines,
        &c.lines,
        "missing_lines",
        "extra_lines",
        acc,
    );
    diff_multiset(
        name,
        &r.rules,
        &c.rules,
        "missing_rules",
        "extra_rules",
        acc,
    );
}

fn diff_multiset<K: std::hash::Hash + Eq + fmt::Display>(
    name: &str,
    r: &HashMap<K, u32>,
    c: &HashMap<K, u32>,
    miss: &'static str,
    extra: &'static str,
    acc: &mut Acc,
) {
    for (k, &rc) in r {
        let cc = c.get(k).copied().unwrap_or(0);
        if rc > cc {
            acc.add(miss, (rc - cc) as u64, || {
                format!("{name}: {k} (ref x{rc}, cand x{cc})")
            });
        }
    }
    for (k, &cc) in c {
        let rc = r.get(k).copied().unwrap_or(0);
        if cc > rc {
            acc.add(extra, (cc - rc) as u64, || {
                format!("{name}: {k} (ref x{rc}, cand x{cc})")
            });
        }
    }
}

fn multiset_sym_diff<'a>(
    a: &'a [LineKey],
    b: &'a [LineKey],
) -> (Vec<&'a LineKey>, Vec<&'a LineKey>) {
    // both sorted
    let (mut i, mut j) = (0, 0);
    let (mut oa, mut ob) = (Vec::new(), Vec::new());
    while i < a.len() || j < b.len() {
        if j >= b.len() || (i < a.len() && a[i] < b[j]) {
            oa.push(&a[i]);
            i += 1;
        } else if i >= a.len() || b[j] < a[i] {
            ob.push(&b[j]);
            j += 1;
        } else {
            i += 1;
            j += 1;
        }
    }
    (oa, ob)
}

fn fmt_list(v: &[&LineKey]) -> String {
    let mut s: Vec<String> = v.iter().take(3).map(|k| format!("[{k}]")).collect();
    if v.len() > 3 {
        s.push(format!("... +{}", v.len() - 3));
    }
    if s.is_empty() {
        "-".into()
    } else {
        s.join(" ")
    }
}

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

fn check_manifest(
    dir: &Path,
    nonempty: &HashMap<String, (u64, u64, u64, String)>,
    prefix: &str,
    acc: &mut Acc,
) -> Option<TileManifest> {
    let path = dir.join("manifest.json");
    let m: TileManifest = match std::fs::read(&path)
        .map_err(anyhow::Error::from)
        .and_then(|b| serde_json::from_slice(&b).map_err(Into::into))
    {
        Ok(m) => m,
        Err(e) => {
            acc.add(prefixed(prefix, "manifest_unreadable"), 1, || {
                format!("{}: {e}", path.display())
            });
            return None;
        }
    };
    let listed: HashMap<&str, &ridi_router_common::manifest::TileMetadata> =
        m.tiles.iter().map(|t| (t.filename.as_str(), t)).collect();
    for (fname, (size, pts, lns, sum)) in nonempty {
        match listed.get(fname.as_str()) {
            None => acc.add(prefixed(prefix, "manifest_missing_tile"), 1, || {
                format!("{fname} not listed in manifest")
            }),
            Some(t) => {
                if t.size_bytes != *size
                    || t.point_count != *pts
                    || t.line_count != *lns
                    || &t.checksum != sum
                {
                    acc.add(prefixed(prefix, "manifest_entry_mismatch"), 1, || {
                        format!(
                            "{fname}: manifest size/points/lines/checksum = {}/{}/{}/{} file = {size}/{pts}/{lns}/{sum}",
                            t.size_bytes, t.point_count, t.line_count, t.checksum
                        )
                    });
                }
            }
        }
    }
    for t in &m.tiles {
        if (t.point_count > 0 || t.line_count > 0) && !nonempty.contains_key(&t.filename) {
            acc.add(prefixed(prefix, "manifest_lists_absent_tile"), 1, || {
                format!(
                    "{} listed as non-empty in manifest but no such non-empty file",
                    t.filename
                )
            });
        }
    }
    Some(m)
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct TileStats {
    pub ref_files: usize,
    pub ref_nonempty: usize,
    pub cand_files: usize,
    pub cand_nonempty: usize,
    pub compared: usize,
    pub identical: usize,
    pub differing: usize,
    pub missing_in_candidate: usize,
    pub extra_in_candidate: usize,
    pub ref_points: u64,
    pub cand_points: u64,
    pub ref_lines: u64,
    pub cand_lines: u64,
    pub ref_rules: u64,
    pub cand_rules: u64,
}

#[derive(Serialize)]
pub struct Summary {
    pub ref_dir: String,
    pub candidate_dir: String,
    pub identical: bool,
    pub tiles: TileStats,
    /// Semantic difference totals (ref vs candidate).
    pub diffs: BTreeMap<&'static str, u64>,
    /// Internal-consistency/manifest issues found in the candidate (non-zero => not identical).
    pub candidate_issues: BTreeMap<&'static str, u64>,
    /// Same checks on the reference (informational).
    pub ref_issues: BTreeMap<&'static str, u64>,
    pub differing_tiles: Vec<String>,
    pub examples: BTreeMap<&'static str, Vec<String>>,
    pub elapsed_ms: u128,
}

struct Side {
    dir: PathBuf,
    files: Vec<String>,
}

fn list_tiles(dir: &Path) -> Result<Side> {
    let mut files = Vec::new();
    for e in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let e = e?;
        let n = e.file_name().to_string_lossy().into_owned();
        if n.starts_with("tile_") && n.ends_with(".rmdf") {
            files.push(n);
        }
    }
    files.sort();
    Ok(Side {
        dir: dir.to_path_buf(),
        files,
    })
}

struct Loaded {
    nonempty: bool,
    model: Option<Model>,          // None if empty or undecodable
    meta: (u64, u64, u64, String), // size, points, lines, stored checksum
    issues: Issues,
    fatal: bool,
}

fn load(dir: &Path, name: &str, tile_size: f32, max_ex: usize) -> Loaded {
    let mut issues = Issues::new(max_ex);
    let bytes = match std::fs::read(dir.join(name)) {
        Ok(b) => b,
        Err(e) => {
            issues.add("decode_error", || format!("{name}: {e}"));
            return Loaded {
                nonempty: true,
                model: None,
                meta: (0, 0, 0, String::new()),
                issues,
                fatal: true,
            };
        }
    };
    let sum = if bytes.len() >= 32 {
        format!("sha256:{}", hex::encode(&bytes[bytes.len() - 32..]))
    } else {
        String::new()
    };
    let h = match read_header(&bytes) {
        Ok(h) => h,
        Err(e) => {
            issues.add("decode_error", || format!("{name}: {e:#}"));
            return Loaded {
                nonempty: true,
                model: None,
                meta: (bytes.len() as u64, 0, 0, sum),
                issues,
                fatal: true,
            };
        }
    };
    let meta = (bytes.len() as u64, h.point_count, h.line_count, sum);
    if is_empty_header(&h) {
        return Loaded {
            nonempty: false,
            model: None,
            meta,
            issues,
            fatal: false,
        };
    }
    match decode(name, &bytes, tile_size, &mut issues) {
        Ok(m) => Loaded {
            nonempty: true,
            model: Some(m),
            meta,
            issues,
            fatal: false,
        },
        Err(e) => {
            issues.add("decode_error", || format!("{name}: {e:#}"));
            Loaded {
                nonempty: true,
                model: None,
                meta,
                issues,
                fatal: true,
            }
        }
    }
}

/// (non-empty, (size, points, lines, checksum), rules) of one side of a compared tile.
type TileSide = (bool, (u64, u64, u64, String), u64);

/// Compare `cand_dir` against `ref_dir`. At most `max_examples` examples are kept per category.
pub fn diff_dirs(ref_dir: &Path, cand_dir: &Path, max_examples: usize) -> Result<Summary> {
    let t0 = Instant::now();
    let max_ex = max_examples;
    let rs = list_tiles(ref_dir)?;
    let cs = list_tiles(cand_dir)?;

    // Tile size from the reference manifest (needed only for header-bounds sanity check).
    let tile_size = std::fs::read(rs.dir.join("manifest.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<TileManifest>(&b).ok())
        .map(|m| m.tile_size_degrees)
        .unwrap_or(0.1);

    let rset: HashSet<&String> = rs.files.iter().collect();
    let cset: HashSet<&String> = cs.files.iter().collect();
    let mut all: Vec<&String> = rset.union(&cset).copied().collect();
    all.sort();

    struct TileOut {
        name: String,
        r: Option<TileSide>,
        c: Option<TileSide>,
        kind: u8, // 0 skip, 1 compared, 2 missing, 3 extra
        identical: bool,
        acc: Acc,
    }

    let outs: Vec<TileOut> = all
        .par_iter()
        .map(|name| {
            let mut acc = Acc {
                max_examples: max_ex,
                ..Default::default()
            };
            let rl = rset
                .contains(name)
                .then(|| load(&rs.dir, name, tile_size, max_ex));
            let cl = cset
                .contains(name)
                .then(|| load(&cs.dir, name, tile_size, max_ex));
            let rne = rl.as_ref().is_some_and(|l| l.nonempty);
            let cne = cl.as_ref().is_some_and(|l| l.nonempty);
            let rules_of = |l: &Loaded| l.model.as_ref().map_or(0, |m| m.rule_count);
            let rmeta = rl
                .as_ref()
                .map(|l| (l.nonempty, l.meta.clone(), rules_of(l)));
            let cmeta = cl
                .as_ref()
                .map(|l| (l.nonempty, l.meta.clone(), rules_of(l)));
            let mut kind = 0;
            let mut identical = true;
            let n = name.as_str();
            match (rne, cne) {
                (false, false) => {}
                (true, false) => {
                    kind = 2;
                    identical = false;
                    let l = rl.as_ref().unwrap();
                    acc.add("missing_tiles", 1, || {
                        format!(
                            "{n}: non-empty in ref ({} points, {} lines), {} in candidate",
                            l.meta.1,
                            l.meta.2,
                            if cl.is_some() { "empty" } else { "absent" }
                        )
                    });
                    acc.add("missing_tile_points", l.meta.1, String::new);
                    acc.add("missing_tile_lines", l.meta.2, String::new);
                }
                (false, true) => {
                    kind = 3;
                    identical = false;
                    let l = cl.as_ref().unwrap();
                    acc.add("extra_tiles", 1, || {
                        format!(
                            "{n}: non-empty in candidate ({} points, {} lines), {} in ref",
                            l.meta.1,
                            l.meta.2,
                            if rl.is_some() { "empty" } else { "absent" }
                        )
                    });
                    acc.add("extra_tile_points", l.meta.1, String::new);
                    acc.add("extra_tile_lines", l.meta.2, String::new);
                }
                (true, true) => {
                    kind = 1;
                    let (r, c) = (rl.as_ref().unwrap(), cl.as_ref().unwrap());
                    match (&r.model, &c.model) {
                        (Some(rm), Some(cm)) => {
                            let before: u64 = acc.counts.values().sum();
                            compare(n, rm, cm, &mut acc);
                            identical = acc.counts.values().sum::<u64>() == before;
                        }
                        _ => {
                            identical = false;
                            acc.add("undecodable_tiles", 1, || {
                                format!("{n}: could not decode one side")
                            });
                        }
                    }
                }
            }
            // Fold in consistency issues (only for non-empty tiles).
            if let Some(l) = rl {
                if l.nonempty {
                    acc.merge_issues("ref:", l.issues);
                }
            }
            if let Some(l) = cl {
                if l.nonempty {
                    if l.issues.total() > 0 || l.fatal {
                        identical = false;
                    }
                    acc.merge_issues("cand:", l.issues);
                }
            }
            TileOut {
                name: name.to_string(),
                r: rmeta,
                c: cmeta,
                kind,
                identical,
                acc,
            }
        })
        .collect();

    let mut acc = Acc {
        max_examples: max_ex,
        ..Default::default()
    };
    let mut st = TileStats {
        ref_files: rs.files.len(),
        ref_nonempty: 0,
        cand_files: cs.files.len(),
        cand_nonempty: 0,
        compared: 0,
        identical: 0,
        differing: 0,
        missing_in_candidate: 0,
        extra_in_candidate: 0,
        ref_points: 0,
        cand_points: 0,
        ref_lines: 0,
        cand_lines: 0,
        ref_rules: 0,
        cand_rules: 0,
    };
    let mut r_nonempty = HashMap::new();
    let mut c_nonempty = HashMap::new();
    let mut differing_tiles = Vec::new();
    for o in outs {
        if let Some((true, meta, rules)) = &o.r {
            st.ref_nonempty += 1;
            st.ref_points += meta.1;
            st.ref_lines += meta.2;
            st.ref_rules += rules;
            r_nonempty.insert(o.name.clone(), meta.clone());
        }
        if let Some((true, meta, rules)) = &o.c {
            st.cand_nonempty += 1;
            st.cand_points += meta.1;
            st.cand_lines += meta.2;
            st.cand_rules += rules;
            c_nonempty.insert(o.name.clone(), meta.clone());
        }
        match o.kind {
            1 => {
                st.compared += 1;
                if o.identical {
                    st.identical += 1;
                } else {
                    st.differing += 1;
                }
            }
            2 => st.missing_in_candidate += 1,
            3 => st.extra_in_candidate += 1,
            _ => {}
        }
        if o.kind != 0 && !o.identical {
            differing_tiles.push(o.name.clone());
        }
        acc.merge(o.acc);
    }

    check_manifest(&rs.dir, &r_nonempty, "ref:", &mut acc);
    let cm = check_manifest(&cs.dir, &c_nonempty, "cand:", &mut acc);
    if let Some(cm) = cm {
        if (cm.tile_size_degrees - tile_size).abs() > 1e-6 {
            acc.add("cand:manifest_tile_size_mismatch", 1, || {
                format!(
                    "tile_size_degrees ref={tile_size} cand={}",
                    cm.tile_size_degrees
                )
            });
        }
    }

    let mut diffs = BTreeMap::new();
    let mut cand_issues = BTreeMap::new();
    let mut ref_issues = BTreeMap::new();
    for (k, v) in &acc.counts {
        if let Some(s) = k.strip_prefix("cand:") {
            cand_issues.insert(leak(s), *v);
        } else if let Some(s) = k.strip_prefix("ref:") {
            ref_issues.insert(leak(s), *v);
        } else {
            diffs.insert(*k, *v);
        }
    }
    let identical = diffs.values().all(|v| *v == 0) && cand_issues.is_empty() && st.differing == 0;
    let total = differing_tiles.len();
    differing_tiles.truncate(max_ex.max(1) * 10);
    if total > differing_tiles.len() {
        differing_tiles.push(format!("... and {} more", total - differing_tiles.len()));
    }

    let summary = Summary {
        ref_dir: rs.dir.display().to_string(),
        candidate_dir: cs.dir.display().to_string(),
        identical,
        tiles: st,
        diffs,
        candidate_issues: cand_issues,
        ref_issues,
        differing_tiles,
        examples: acc
            .examples
            .into_iter()
            .filter(|(_, v)| !v.iter().all(|s| s.is_empty()))
            .map(|(k, v)| (k, v.into_iter().filter(|s| !s.is_empty()).collect()))
            .collect(),
        elapsed_ms: t0.elapsed().as_millis(),
    };

    Ok(summary)
}

fn leak(s: &str) -> &'static str {
    prefixed("", Box::leak(s.to_string().into_boxed_str()))
}

pub fn print_summary(s: &Summary) {
    let t = &s.tiles;
    println!(
        "rmdf-diff  ref={}  candidate={}",
        s.ref_dir, s.candidate_dir
    );
    println!(
        "files: ref {} ({} non-empty), cand {} ({} non-empty)",
        t.ref_files, t.ref_nonempty, t.cand_files, t.cand_nonempty
    );
    println!(
        "totals: points ref {} cand {} | lines ref {} cand {} | rules ref {} cand {}",
        t.ref_points, t.cand_points, t.ref_lines, t.cand_lines, t.ref_rules, t.cand_rules
    );
    println!(
        "tiles: compared {}  identical {}  differing {}  missing {}  extra {}",
        t.compared, t.identical, t.differing, t.missing_in_candidate, t.extra_in_candidate
    );
    let get = |k: &str| s.diffs.get(k).copied().unwrap_or(0);
    println!(
        "points: missing {} extra {} coord-mismatch {} | flags: residential {} nogo {} other {} | incident-set {}",
        get("missing_points"),
        get("extra_points"),
        get("point_coord_mismatch"),
        get("residential_mismatch"),
        get("nogo_mismatch"),
        get("other_flag_mismatch"),
        get("incident_lines_mismatch")
    );
    println!(
        "lines: missing {} extra {} | rules: missing {} extra {} | bounds-mismatch {}",
        get("missing_lines"),
        get("extra_lines"),
        get("missing_rules"),
        get("extra_rules"),
        get("tile_bounds_mismatch")
    );
    if !s.candidate_issues.is_empty() {
        println!(
            "candidate consistency/manifest issues: {:?}",
            s.candidate_issues
        );
    }
    if !s.ref_issues.is_empty() {
        println!(
            "reference consistency/manifest issues (info): {:?}",
            s.ref_issues
        );
    }
    if !s.examples.is_empty() {
        println!("examples:");
        for (k, v) in &s.examples {
            for e in v {
                println!("  [{k}] {e}");
            }
        }
    }
    println!(
        "RESULT: {} ({} ms)",
        if s.identical {
            "IDENTICAL"
        } else {
            "DIFFERENT"
        },
        s.elapsed_ms
    );
}

// ---------------------------------------------------------------------------
// Digest
// ---------------------------------------------------------------------------

/// Semantic digest of a tile directory.
#[derive(Debug, Serialize)]
pub struct DirDigest {
    /// sha256 (hex) of each non-empty tile's decoded model, in canonical order.
    pub tiles: BTreeMap<String, String>,
    /// Consistency issues over all tiles (empty for a correct directory).
    pub issues: BTreeMap<&'static str, u64>,
    pub issue_examples: Vec<String>,
    pub totals: Totals,
}

/// Content totals over all non-empty tiles (points in a tile buffer count once per tile).
#[derive(Debug, Default, Serialize)]
pub struct Totals {
    pub points: u64,
    pub lines: u64,
    pub rules: u64,
    /// points with flag bit i set, i = 0..16
    pub points_with_flag: [u64; 16],
}

fn feed_line(h: &mut Sha256, l: &LineKey) {
    h.update(l.a.to_le_bytes());
    h.update(l.b.to_le_bytes());
    h.update([l.dir]);
    for c in l.coords {
        h.update(c.to_le_bytes());
    }
    for t in l.tags.iter() {
        match t {
            None => h.update([0u8]),
            Some(v) => {
                h.update([1u8]);
                h.update((v.len() as u64).to_le_bytes());
                h.update(v.as_bytes());
            }
        }
    }
}

fn model_digest(m: &Model) -> String {
    let mut h = Sha256::new();
    for b in m.bounds {
        h.update(b.to_le_bytes());
    }
    let mut ids: Vec<&u64> = m.points.keys().collect();
    ids.sort();
    h.update((ids.len() as u64).to_le_bytes());
    for id in ids {
        let p = &m.points[id];
        h.update(id.to_le_bytes());
        h.update(p.lat.to_bits().to_le_bytes());
        h.update(p.lon.to_bits().to_le_bytes());
        h.update(p.flags.to_le_bytes());
        h.update((p.incident.len() as u64).to_le_bytes());
        for l in &p.incident {
            feed_line(&mut h, l);
        }
    }
    let mut lines: Vec<(&LineKey, &u32)> = m.lines.iter().collect();
    lines.sort();
    h.update((lines.len() as u64).to_le_bytes());
    for (l, n) in lines {
        feed_line(&mut h, l);
        h.update(n.to_le_bytes());
    }
    let mut rules: Vec<(&RuleKey, &u32)> = m.rules.iter().collect();
    rules.sort();
    h.update((rules.len() as u64).to_le_bytes());
    for (r, n) in rules {
        h.update(r.via.to_le_bytes());
        h.update([r.rule_type]);
        for side in [&r.from, &r.to] {
            h.update((side.len() as u64).to_le_bytes());
            for l in side {
                feed_line(&mut h, l);
            }
        }
        h.update(n.to_le_bytes());
    }
    hex::encode(h.finalize())
}

/// Digest every tile of `dir` (tile size from its manifest).
pub fn digest_dir(dir: &Path) -> Result<DirDigest> {
    let side = list_tiles(dir)?;
    let tile_size = std::fs::read(dir.join("manifest.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<TileManifest>(&b).ok())
        .map(|m| m.tile_size_degrees)
        .with_context(|| format!("reading manifest in {}", dir.display()))?;
    let loaded: Vec<(String, Loaded)> = side
        .files
        .par_iter()
        .map(|name| (name.clone(), load(dir, name, tile_size, 3)))
        .collect();
    let mut tiles = BTreeMap::new();
    let mut issues: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut issue_examples = Vec::new();
    let mut totals = Totals::default();
    for (name, l) in loaded {
        for (k, v) in &l.issues.counts {
            *issues.entry(*k).or_default() += v;
        }
        for ex in l.issues.examples.values().flatten() {
            if issue_examples.len() < 20 {
                issue_examples.push(ex.clone());
            }
        }
        if let Some(m) = &l.model {
            totals.points += m.points.len() as u64;
            totals.lines += m.lines.values().map(|n| *n as u64).sum::<u64>();
            totals.rules += m.rule_count;
            for p in m.points.values() {
                for (bit, n) in totals.points_with_flag.iter_mut().enumerate() {
                    if p.flags & (1 << bit) != 0 {
                        *n += 1;
                    }
                }
            }
            tiles.insert(name, model_digest(m));
        }
    }
    Ok(DirDigest {
        tiles,
        issues,
        issue_examples,
        totals,
    })
}
