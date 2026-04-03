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
    pub const VERSION: u32 = 1;
}

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct GridCellEntry {
    pub cell_id: u32,
    pub _padding1: u32,
    pub points_offset: u64,
    pub points_count: u32,
    pub _padding2: u32,
}

impl GridCellEntry {
    pub fn encode_cell_id(lat: f32, lon: f32, precision: u32) -> u32 {
        let lat_rounded = (lat * precision as f32).round() as i16;
        let lon_rounded = (lon * precision as f32).round() as i16;
        ((lat_rounded as u32) << 16) | (lon_rounded as u32 & 0xFFFF)
    }
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

impl PointRecord {
    pub const RESIDENTIAL_IN_PROXIMITY_FLAG: u16 = 0b0000_0000_0000_0001;
    pub const NOGO_AREA_FLAG: u16 = 0b0000_0000_0000_0010;

    pub fn residential_in_proximity(&self) -> bool {
        (self.flags & Self::RESIDENTIAL_IN_PROXIMITY_FLAG) != 0
    }

    pub fn nogo_area(&self) -> bool {
        (self.flags & Self::NOGO_AREA_FLAG) != 0
    }
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
