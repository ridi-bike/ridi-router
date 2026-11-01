use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

const MAGIC_BYTES: &[u8; 4] = b"RMDF";
pub const MAGIC_LEN: usize = MAGIC_BYTES.len();

pub const NUM_SECTIONS: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct TileId {
    pub col: u16,
    pub row: u16,
}

impl TileId {
    pub fn from_coords(lat: f32, lon: f32, tile_size_degrees: f32) -> Self {
        let col = ((lon + 180.0) / tile_size_degrees).floor() as u16;
        let row = ((lat + 90.0) / tile_size_degrees).floor() as u16;
        Self { col, row }
    }

    pub fn to_filename(&self) -> String {
        format!("tile_{}_{}.rmdf", self.col, self.row)
    }
}

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct TileBounds {
    pub lat_min: f32,
    pub lat_max: f32,
    pub lon_min: f32,
    pub lon_max: f32,
}

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct RmdfHeader {
    pub magic: [u8; MAGIC_LEN],
    pub version: u32,
    pub tile_bounds: TileBounds,
    pub point_count: u64,
    pub line_count: u64,
    pub spatial_grid_cell_count: u32,
    pub tag_value_count: u32,
    pub tag_set_count: u32,
    pub rule_count: u32,
    pub section_offsets: [u64; NUM_SECTIONS],
}

impl RmdfHeader {
    pub const SIZE: usize = std::mem::size_of::<RmdfHeader>();
    pub const MAGIC: [u8; MAGIC_LEN] = *MAGIC_BYTES;
    /// v2: compact records (tile-local point indices, u32 refs), 8-byte aligned sections and a
    /// trailing xxh3-128 checksum.
    pub const VERSION: u32 = 2;
}

/// Every section starts at a multiple of this, so all of them can be cast in place.
pub const SECTION_ALIGN: usize = 8;

pub const fn align_section(offset: usize) -> usize {
    offset.div_ceil(SECTION_ALIGN) * SECTION_ALIGN
}

/// Trailing checksum: xxh3-128 of all preceding bytes, little endian.
pub const CHECKSUM_LEN: usize = 16;
pub const CHECKSUM_ALGORITHM: &str = "xxh3-128";

pub fn checksum(data: &[u8]) -> [u8; CHECKSUM_LEN] {
    xxhash_rust::xxh3::xxh3_128(data).to_le_bytes()
}

/// Manifest form of a tile checksum: `xxh3-128:<hex>`.
pub fn checksum_string(sum: &[u8; CHECKSUM_LEN]) -> String {
    format!("{CHECKSUM_ALGORITHM}:{}", hex::encode(sum))
}

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct GridCellEntry {
    pub cell_id: u32,
    pub points_offset: u32,
    pub points_count: u32,
}

impl GridCellEntry {
    pub fn encode_cell_id(lat: f32, lon: f32, precision: u32) -> u32 {
        let lat_rounded = (lat * precision as f32).round() as i16;
        let lon_rounded = (lon * precision as f32).round() as i16;
        ((lat_rounded as u32) << 16) | (lon_rounded as u32 & 0xFFFF)
    }
}

/// A routable node. Its incident lines are `line_refs[lines_offset..][..lines_count]`; restrictions
/// with it as via node are the rule records with `via_osm_id == osm_id` (see `HAS_RULES_FLAG`).
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct PointRecord {
    pub osm_id: u64,
    pub lat: f32,
    pub lon: f32,
    pub lines_offset: u32,
    pub lines_count: u16,
    pub flags: u16,
}

impl PointRecord {
    pub const RESIDENTIAL_IN_PROXIMITY_FLAG: u16 = 0b0000_0000_0000_0001;
    pub const NOGO_AREA_FLAG: u16 = 0b0000_0000_0000_0010;
    pub const HAS_RULES_FLAG: u16 = 0b0000_0000_0000_0100;

    pub fn residential_in_proximity(&self) -> bool {
        (self.flags & Self::RESIDENTIAL_IN_PROXIMITY_FLAG) != 0
    }

    pub fn nogo_area(&self) -> bool {
        (self.flags & Self::NOGO_AREA_FLAG) != 0
    }

    pub fn has_rules(&self) -> bool {
        (self.flags & Self::HAS_RULES_FLAG) != 0
    }
}

/// A line between two points of the same tile, by index into the points section.
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct LineRecord {
    pub point_a: u32,
    pub point_b: u32,
    /// tag set index in the low 30 bits, direction in the top 2
    pub tag_set_direction: u32,
}

impl LineRecord {
    const DIRECTION_SHIFT: u32 = 30;
    pub const MAX_TAG_SET_INDEX: u32 = (1 << Self::DIRECTION_SHIFT) - 1;

    pub fn new(point_a: u32, point_b: u32, tag_set_index: u32, direction: u8) -> Self {
        assert!(tag_set_index <= Self::MAX_TAG_SET_INDEX && direction < 4);
        Self {
            point_a,
            point_b,
            tag_set_direction: tag_set_index | (direction as u32) << Self::DIRECTION_SHIFT,
        }
    }

    pub fn tag_set_index(&self) -> u32 {
        self.tag_set_direction & Self::MAX_TAG_SET_INDEX
    }

    pub fn direction(&self) -> u8 {
        (self.tag_set_direction >> Self::DIRECTION_SHIFT) as u8
    }
}

/// A line with its endpoints looked up in the tile's points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedLine {
    pub point_a_osm_id: u64,
    pub point_a_lat: f32,
    pub point_a_lon: f32,
    pub point_b_osm_id: u64,
    pub point_b_lat: f32,
    pub point_b_lon: f32,
    pub direction: u8,
    pub tag_set_index: u32,
}

impl ResolvedLine {
    /// `None` when an endpoint index is outside `points`.
    pub fn resolve(line: &LineRecord, points: &[PointRecord]) -> Option<Self> {
        let a = points.get(line.point_a as usize)?;
        let b = points.get(line.point_b as usize)?;
        Some(Self {
            point_a_osm_id: a.osm_id,
            point_a_lat: a.lat,
            point_a_lon: a.lon,
            point_b_osm_id: b.osm_id,
            point_b_lat: b.lat,
            point_b_lon: b.lon,
            direction: line.direction(),
            tag_set_index: line.tag_set_index(),
        })
    }
}

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct StringEntry {
    pub offset: u32,
    pub length: u32,
}

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct TagSetRecord {
    pub name_idx: u32,
    pub hw_ref_idx: u32,
    pub highway_idx: u32,
    pub surface_idx: u32,
    pub smoothness_idx: u32,
}

impl TagSetRecord {
    pub const NONE: u32 = 0xFFFFFFFF;
}

/// A turn restriction. Records are sorted by `via_osm_id` (stable, so a via node's rules keep
/// their order); from/to lines are `rule_line_refs[offset..][..count]`.
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct RuleRecord {
    pub via_osm_id: u64,
    pub from_lines_offset: u32,
    pub to_lines_offset: u32,
    pub from_lines_count: u16,
    pub to_lines_count: u16,
    pub rule_type: u8,
    pub _padding: [u8; 3],
}

impl RuleRecord {
    /// The rules whose via node is `osm_id`.
    pub fn for_via(rules: &[RuleRecord], osm_id: u64) -> &[RuleRecord] {
        let start = rules.partition_point(|r| r.via_osm_id < osm_id);
        let end = start + rules[start..].partition_point(|r| r.via_osm_id == osm_id);
        &rules[start..end]
    }
}

/// Section layout, in file order, each starting at a `SECTION_ALIGN` boundary:
/// spatial index (`GridCellEntry`), points, lines, line refs (`u32` line indices), tag values
/// (`StringEntry` table then the string pool), tag sets, rules (`RuleRecord`s then their `u32`
/// line refs), then the `CHECKSUM_LEN`-byte checksum.
pub mod section {
    pub const SPATIAL_INDEX: usize = 0;
    pub const POINTS: usize = 1;
    pub const LINES: usize = 2;
    pub const LINE_REFS: usize = 3;
    pub const TAG_VALUES: usize = 4;
    pub const TAG_SETS: usize = 5;
    pub const RULES: usize = 6;

    const _: () = assert!(
        super::NUM_SECTIONS == RULES + 1,
        "NUM_SECTIONS must equal the number of section constants"
    );
}

const _: () = {
    assert!(RmdfHeader::SIZE.is_multiple_of(SECTION_ALIGN));
    assert!(std::mem::size_of::<GridCellEntry>() == 12);
    assert!(std::mem::size_of::<PointRecord>() == 24);
    assert!(std::mem::size_of::<LineRecord>() == 12);
    assert!(std::mem::size_of::<StringEntry>() == 8);
    assert!(std::mem::size_of::<RuleRecord>() == 24);
};
