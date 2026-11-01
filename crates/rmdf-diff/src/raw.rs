//! Version-specific tile reading. Both versions are normalized to the v1 record shapes (absolute
//! offsets, endpoint ids and coordinates on every line, rules addressed from their via point), so
//! the semantic decoding and comparison do not depend on the format version.

use anyhow::{bail, Context, Result};
use bytemuck::{pod_read_unaligned, Pod, Zeroable};
use ridi_router_common::format::{self as v2, RmdfHeader, TagSetRecord, NUM_SECTIONS};
use sha2::{Digest, Sha256};

use crate::Issues;

pub(crate) use ridi_router_common::format::section;

/// The v1 on-disk records, which are also the normalized shapes.
pub(crate) mod v1 {
    use super::{Pod, Zeroable};

    pub const VERSION: u32 = 1;
    pub const CHECKSUM_LEN: usize = 32;

    #[derive(Debug, Clone, Copy, Pod, Zeroable)]
    #[repr(C)]
    pub struct GridCellEntry {
        pub cell_id: u32,
        pub _padding1: u32,
        pub points_offset: u64,
        pub points_count: u32,
        pub _padding2: u32,
    }

    #[derive(Debug, Clone, Copy, Pod, Zeroable)]
    #[repr(C)]
    pub struct PointRecord {
        pub osm_id: u64,
        pub lat: f32,
        pub lon: f32,
        pub lines_offset: u64,
        pub lines_count: u32,
        pub _padding1: u32,
        pub rules_offset: u64,
        pub rules_count: u32,
        pub flags: u16,
        pub _padding2: u16,
    }

    #[derive(Debug, Clone, Copy, Pod, Zeroable)]
    #[repr(C)]
    pub struct LineRecord {
        pub point_a_osm_id: u64,
        pub point_a_lat: f32,
        pub point_a_lon: f32,
        pub point_b_osm_id: u64,
        pub point_b_lat: f32,
        pub point_b_lon: f32,
        pub direction: u8,
        pub _padding1: u8,
        pub _padding2: u16,
        pub tag_set_index: u32,
    }

    #[derive(Debug, Clone, Copy, Pod, Zeroable)]
    #[repr(C)]
    pub struct StringEntry {
        pub offset: u64,
        pub length: u32,
        pub _padding: u32,
    }

    #[derive(Debug, Clone, Copy, Pod, Zeroable)]
    #[repr(C)]
    pub struct RuleRecord {
        pub from_lines_offset: u64,
        pub from_lines_count: u32,
        pub _padding1: u32,
        pub to_lines_offset: u64,
        pub to_lines_count: u32,
        pub rule_type: u8,
        pub _padding2: u8,
        pub _padding3: u16,
    }
}

pub(crate) struct Raw<'a> {
    pub grid: Vec<v1::GridCellEntry>,
    pub points: Vec<v1::PointRecord>,
    pub lines: Vec<v1::LineRecord>,
    pub line_refs: Vec<u64>,
    /// (offset, length) into `pool`
    pub tag_values: Vec<(u64, u32)>,
    pub pool: &'a [u8],
    pub tag_sets: Vec<TagSetRecord>,
    pub rules: Vec<v1::RuleRecord>,
    pub rule_refs: Vec<u64>,
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

/// Little-endian `u64`s (v1) or `u32`s (v2) in `off..end`, widened to `u64`.
fn read_refs(bytes: &[u8], off: usize, end: usize, width: usize, what: &str) -> Result<Vec<u64>> {
    if end < off {
        bail!("{what}: end {end} < start {off}");
    }
    let s = slice_at(bytes, off, end - off, what)?;
    if s.len() % width != 0 {
        bail!("{what}: size {} not a multiple of {width}", s.len());
    }
    Ok(match width {
        8 => s
            .as_chunks::<8>()
            .0
            .iter()
            .map(|c| u64::from_le_bytes(*c))
            .collect(),
        _ => s
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c) as u64)
            .collect(),
    })
}

pub(crate) fn read_header(bytes: &[u8]) -> Result<RmdfHeader> {
    if bytes.len() < RmdfHeader::SIZE {
        bail!("file too small for header ({} bytes)", bytes.len());
    }
    let h: RmdfHeader = pod_read_unaligned(&bytes[..RmdfHeader::SIZE]);
    if h.magic != RmdfHeader::MAGIC {
        bail!("bad magic {:?}", h.magic);
    }
    if h.version != v1::VERSION && h.version != RmdfHeader::VERSION {
        bail!("unsupported version {}", h.version);
    }
    Ok(h)
}

/// Checksum length of a tile of version `version`.
pub(crate) fn checksum_len(version: u32) -> usize {
    if version == v1::VERSION {
        v1::CHECKSUM_LEN
    } else {
        v2::CHECKSUM_LEN
    }
}

/// The manifest form of the checksum stored at the end of `bytes`.
pub(crate) fn stored_checksum_string(version: u32, bytes: &[u8]) -> String {
    let len = checksum_len(version);
    let Some(start) = bytes.len().checked_sub(len) else {
        return String::new();
    };
    let sum = &bytes[start..];
    if version == v1::VERSION {
        format!("sha256:{}", hex::encode(sum))
    } else {
        v2::checksum_string(sum.try_into().unwrap())
    }
}

/// Verify the trailing checksum; returns where the data ends.
fn check_checksum(name: &str, h: &RmdfHeader, bytes: &[u8], issues: &mut Issues) -> usize {
    let len = checksum_len(h.version);
    if bytes.len() < RmdfHeader::SIZE + len {
        issues.add("checksum_missing", || {
            format!("{name}: no room for checksum")
        });
        return bytes.len();
    }
    let (data, sum) = bytes.split_at(bytes.len() - len);
    let ok = if h.version == v1::VERSION {
        Sha256::digest(data).as_slice() == sum
    } else {
        v2::checksum(data) == sum
    };
    if ok {
        data.len()
    } else {
        issues.add("checksum_mismatch", || {
            format!("{name}: trailing checksum mismatch")
        });
        // v1 files may lack the checksum entirely; v2 always ends in one
        if h.version == v1::VERSION {
            bytes.len()
        } else {
            data.len()
        }
    }
}

pub(crate) fn read_raw<'a>(
    name: &str,
    h: &RmdfHeader,
    bytes: &'a [u8],
    issues: &mut Issues,
) -> Result<Raw<'a>> {
    let data_end = check_checksum(name, h, bytes, issues);
    let so = |i: usize| h.section_offsets[i] as usize;
    for i in 1..NUM_SECTIONS {
        if so(i) < so(i - 1) || so(i) > data_end {
            issues.add("section_layout", || {
                format!("{name}: section {i} offset {} out of order/bounds", so(i))
            });
        }
    }
    if h.version == v1::VERSION {
        read_v1(name, h, bytes, data_end, issues)
    } else {
        read_v2(name, h, bytes, data_end, issues)
    }
}

fn read_v1<'a>(
    name: &str,
    h: &RmdfHeader,
    bytes: &'a [u8],
    data_end: usize,
    issues: &mut Issues,
) -> Result<Raw<'a>> {
    use std::mem::size_of;
    let so = |i: usize| h.section_offsets[i] as usize;
    let pc = h.point_count as usize;
    let lc = h.line_count as usize;

    // Layout: sections must be contiguous in the order the writer emits them.
    let expect = [
        (section::SPATIAL_INDEX, RmdfHeader::SIZE),
        (
            section::POINTS,
            so(section::SPATIAL_INDEX)
                + h.spatial_grid_cell_count as usize * size_of::<v1::GridCellEntry>(),
        ),
        (
            section::LINES,
            so(section::POINTS) + pc * size_of::<v1::PointRecord>(),
        ),
        (
            section::LINE_REFS,
            so(section::LINES) + lc * size_of::<v1::LineRecord>(),
        ),
    ];
    for (sec, want) in expect {
        if so(sec) != want {
            issues.add("section_layout", || {
                format!("{name}: section {sec} offset {} expected {want}", so(sec))
            });
        }
    }
    if so(section::TAG_SETS) + h.tag_set_count as usize * size_of::<TagSetRecord>()
        != so(section::RULES)
    {
        issues.add("section_layout", || {
            format!("{name}: tag sets do not end at rules section")
        });
    }

    let tvc = h.tag_value_count as usize;
    let entries: Vec<v1::StringEntry> =
        read_pods(bytes, so(section::TAG_VALUES), tvc, "tag value entries")?;
    let pool_start = so(section::TAG_VALUES) + tvc * size_of::<v1::StringEntry>();
    let pool_end = so(section::TAG_SETS);
    Ok(Raw {
        grid: read_pods(
            bytes,
            so(section::SPATIAL_INDEX),
            h.spatial_grid_cell_count as usize,
            "spatial index",
        )?,
        points: read_pods(bytes, so(section::POINTS), pc, "points")?,
        lines: read_pods(bytes, so(section::LINES), lc, "lines")?,
        line_refs: read_refs(
            bytes,
            so(section::LINE_REFS),
            so(section::TAG_VALUES),
            8,
            "line refs",
        )?,
        tag_values: entries.iter().map(|e| (e.offset, e.length)).collect(),
        pool: slice_at(
            bytes,
            pool_start,
            pool_end.saturating_sub(pool_start),
            "tag string pool",
        )?,
        tag_sets: read_pods(
            bytes,
            so(section::TAG_SETS),
            h.tag_set_count as usize,
            "tag sets",
        )?,
        rules: read_pods(bytes, so(section::RULES), h.rule_count as usize, "rules")?,
        rule_refs: read_refs(
            bytes,
            so(section::RULES) + h.rule_count as usize * size_of::<v1::RuleRecord>(),
            data_end,
            8,
            "rule line refs",
        )?,
    })
}

fn read_v2<'a>(
    name: &str,
    h: &RmdfHeader,
    bytes: &'a [u8],
    data_end: usize,
    issues: &mut Issues,
) -> Result<Raw<'a>> {
    use std::mem::size_of;
    let so = |i: usize| h.section_offsets[i] as usize;
    let pc = h.point_count as usize;
    let lc = h.line_count as usize;
    let tvc = h.tag_value_count as usize;
    let rc = h.rule_count as usize;

    // Layout: sections in writer order, each at the first aligned offset after the previous one.
    // The tag string pool's length is not stored, so the tag sets offset is only checked for
    // alignment and order; the rule line refs run to the checksum.
    let sizes = [
        h.spatial_grid_cell_count as usize * size_of::<v2::GridCellEntry>(),
        pc * size_of::<v2::PointRecord>(),
        lc * size_of::<v2::LineRecord>(),
        lc * 2 * size_of::<u32>(),
    ];
    let mut want = RmdfHeader::SIZE;
    for (sec, size) in sizes.iter().enumerate() {
        let aligned = v2::align_section(want);
        if so(sec) != aligned {
            issues.add("section_layout", || {
                format!(
                    "{name}: section {sec} offset {} expected {aligned}",
                    so(sec)
                )
            });
        }
        want = so(sec) + size;
    }
    if so(section::TAG_VALUES) != v2::align_section(want) {
        issues.add("section_layout", || {
            format!(
                "{name}: tag values offset {} expected {}",
                so(section::TAG_VALUES),
                v2::align_section(want)
            )
        });
    }
    let tag_sets_end = so(section::TAG_SETS) + h.tag_set_count as usize * size_of::<TagSetRecord>();
    if so(section::RULES) != v2::align_section(tag_sets_end) {
        issues.add("section_layout", || {
            format!("{name}: tag sets do not end at rules section")
        });
    }
    for i in 0..NUM_SECTIONS {
        if so(i) % v2::SECTION_ALIGN != 0 {
            issues.add("section_unaligned", || {
                format!("{name}: section {i} offset {} not aligned", so(i))
            });
        }
    }

    let grid: Vec<v2::GridCellEntry> = read_pods(
        bytes,
        so(section::SPATIAL_INDEX),
        h.spatial_grid_cell_count as usize,
        "spatial index",
    )?;
    let points: Vec<v2::PointRecord> = read_pods(bytes, so(section::POINTS), pc, "points")?;
    let lines: Vec<v2::LineRecord> = read_pods(bytes, so(section::LINES), lc, "lines")?;
    let line_refs = read_refs(
        bytes,
        so(section::LINE_REFS),
        so(section::LINE_REFS) + sizes[section::LINE_REFS],
        4,
        "line refs",
    )?;
    let entries: Vec<v2::StringEntry> =
        read_pods(bytes, so(section::TAG_VALUES), tvc, "tag value entries")?;
    let pool_start = so(section::TAG_VALUES) + tvc * size_of::<v2::StringEntry>();
    let pool_end = so(section::TAG_SETS);
    let rules: Vec<v2::RuleRecord> = read_pods(bytes, so(section::RULES), rc, "rules")?;
    let rule_refs = read_refs(
        bytes,
        so(section::RULES) + rc * size_of::<v2::RuleRecord>(),
        data_end,
        4,
        "rule line refs",
    )?;

    // Rules: sorted by via node; each via is a point of this tile, flagged as having rules.
    if rules.windows(2).any(|w| w[0].via_osm_id > w[1].via_osm_id) {
        issues.add("rules_unsorted", || {
            format!("{name}: rules not sorted by via node")
        });
    }
    let mut point_index: std::collections::HashMap<u64, usize> = points
        .iter()
        .enumerate()
        .map(|(i, p)| (p.osm_id, i))
        .collect();
    point_index.shrink_to_fit();
    let mut rules_of = vec![(0usize, 0usize); pc]; // (first rule, count) per point
    for (ri, r) in rules.iter().enumerate() {
        match point_index.get(&r.via_osm_id) {
            None => issues.add("rule_via_missing_point", || {
                format!("{name}: rule {ri} via {} has no point record", r.via_osm_id)
            }),
            Some(&pi) => {
                let e = &mut rules_of[pi];
                if e.1 == 0 {
                    e.0 = ri;
                }
                e.1 += 1;
            }
        }
    }
    for (p, &(_, count)) in points.iter().zip(&rules_of) {
        if p.has_rules() != (count > 0) {
            issues.add("point_has_rules_flag", || {
                format!(
                    "{name}: point {} has_rules flag {} but {count} rules",
                    p.osm_id,
                    p.has_rules()
                )
            });
        }
    }

    let invalid = v1::LineRecord::zeroed();
    Ok(Raw {
        grid: grid
            .iter()
            .map(|g| v1::GridCellEntry {
                cell_id: g.cell_id,
                _padding1: 0,
                points_offset: g.points_offset as u64,
                points_count: g.points_count,
                _padding2: 0,
            })
            .collect(),
        points: points
            .iter()
            .zip(&rules_of)
            .map(|(p, &(first, count))| v1::PointRecord {
                osm_id: p.osm_id,
                lat: p.lat,
                lon: p.lon,
                lines_offset: p.lines_offset as u64,
                lines_count: p.lines_count as u32,
                _padding1: 0,
                rules_offset: first as u64,
                rules_count: count as u32,
                flags: p.flags & !v2::PointRecord::HAS_RULES_FLAG,
                _padding2: 0,
            })
            .collect(),
        lines: lines
            .iter()
            .enumerate()
            .map(|(i, l)| match v2::ResolvedLine::resolve(l, &points) {
                Some(r) => v1::LineRecord {
                    point_a_osm_id: r.point_a_osm_id,
                    point_a_lat: r.point_a_lat,
                    point_a_lon: r.point_a_lon,
                    point_b_osm_id: r.point_b_osm_id,
                    point_b_lat: r.point_b_lat,
                    point_b_lon: r.point_b_lon,
                    direction: r.direction,
                    _padding1: 0,
                    _padding2: 0,
                    tag_set_index: r.tag_set_index,
                },
                None => {
                    issues.add("line_point_index_oob", || {
                        format!(
                            "{name}: line {i} points {}/{} >= {pc}",
                            l.point_a, l.point_b
                        )
                    });
                    invalid
                }
            })
            .collect(),
        line_refs,
        tag_values: entries
            .iter()
            .map(|e| (e.offset as u64, e.length))
            .collect(),
        pool: slice_at(
            bytes,
            pool_start,
            pool_end.saturating_sub(pool_start),
            "tag string pool",
        )?,
        tag_sets: read_pods(
            bytes,
            so(section::TAG_SETS),
            h.tag_set_count as usize,
            "tag sets",
        )?,
        rules: rules
            .iter()
            .map(|r| v1::RuleRecord {
                from_lines_offset: r.from_lines_offset as u64,
                from_lines_count: r.from_lines_count as u32,
                _padding1: 0,
                to_lines_offset: r.to_lines_offset as u64,
                to_lines_count: r.to_lines_count as u32,
                rule_type: r.rule_type,
                _padding2: 0,
                _padding3: 0,
            })
            .collect(),
        rule_refs,
    })
}
