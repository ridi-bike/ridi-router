use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

// Magic number for RMDF files - compiler infers the array size from the literal
const MAGIC_BYTES: &[u8; 4] = b"RMDF";
pub const MAGIC_LEN: usize = MAGIC_BYTES.len();

// Number of sections in RMDF format
pub const NUM_SECTIONS: usize = 7;

// Tile coordinates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct TileId {
    pub col: u16, // 0-359 (longitude-based)
    pub row: u16, // 0-179 (latitude-based)
}

impl TileId {
    pub fn from_coords(lat: f32, lon: f32) -> Self {
        let col = ((lon + 180.0).floor() as u16).min(359);
        let row = ((lat + 90.0).floor() as u16).min(179);
        Self { col, row }
    }

    pub fn to_filename(&self) -> String {
        format!("tile_{}_{}.rmdf", self.col, self.row)
    }
}

// Tile geographic bounds
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct TileBounds {
    pub lat_min: f32,
    pub lat_max: f32,
    pub lon_min: f32,
    pub lon_max: f32,
}

// RMDF file header
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct RmdfHeader {
    pub magic: [u8; MAGIC_LEN],  // b"RMDF"
    pub version: u32,            // Format version (1)
    pub tile_bounds: TileBounds, // 16 bytes
    pub point_count: u64,
    pub line_count: u64,
    pub spatial_grid_cell_count: u32,
    pub tag_value_count: u32,
    pub tag_set_count: u32,
    pub rule_count: u32,
    pub section_offsets: [u64; NUM_SECTIONS], // Offsets to each section
                                              // checksum stored separately at end of file (not in header)
}

impl RmdfHeader {
    pub const SIZE: usize = std::mem::size_of::<RmdfHeader>();
    pub const MAGIC: [u8; MAGIC_LEN] = *MAGIC_BYTES;
    pub const VERSION: u32 = 1;
}

// Spatial index grid cell entry (20 bytes - properly aligned)
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct GridCellEntry {
    pub cell_id: u32, // Encoded (lat_rounded << 16 | lon_rounded)
    pub _padding1: u32,
    pub points_offset: u64, // Offset into Points Section
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

// Point record (56 bytes - properly aligned)
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct PointRecord {
    pub osm_id: u64,
    pub lat: f32,
    pub lon: f32,
    pub lines_offset: u64, // Offset into Line References Section
    pub lines_count: u32,
    pub _padding1: u32,
    pub rules_offset: u64, // Offset into Rules Section
    pub rules_count: u32,
    pub flags: u16, // bit 0: residential_in_proximity, bit 1: nogo_area
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

// Line record (48 bytes - properly aligned)
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct LineRecord {
    pub point_a_osm_id: u64,
    pub point_a_lat: f32,
    pub point_a_lon: f32,
    pub point_b_osm_id: u64,
    pub point_b_lat: f32,
    pub point_b_lon: f32,
    pub direction: u8, // 0=BothWays, 1=OneWay, 2=Roundabout
    pub _padding1: u8,
    pub _padding2: u16,
    pub tag_set_index: u32, // Index into Tag Sets Section
}

// String entry in tag values section (16 bytes - padded for alignment)
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct StringEntry {
    pub offset: u64, // Offset into string data pool
    pub length: u32,
    pub _padding: u32,
}

// Tag set record (20 bytes)
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct TagSetRecord {
    pub name_idx: u32, // 0xFFFFFFFF = none
    pub hw_ref_idx: u32,
    pub highway_idx: u32,
    pub surface_idx: u32,
    pub smoothness_idx: u32,
}

impl TagSetRecord {
    pub const NONE: u32 = 0xFFFFFFFF;
}

// Rule record (40 bytes - properly aligned)
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct RuleRecord {
    pub from_lines_offset: u64,
    pub from_lines_count: u32,
    pub _padding1: u32,
    pub to_lines_offset: u64,
    pub to_lines_count: u32,
    pub rule_type: u8, // 0=OnlyAllowed, 1=NotAllowed
    pub _padding2: u8,
    pub _padding3: u16,
}

// Section identifiers (for offsets array)
pub mod section {
    pub const SPATIAL_INDEX: usize = 0;
    pub const POINTS: usize = 1;
    pub const LINES: usize = 2;
    pub const LINE_REFS: usize = 3;
    pub const TAG_VALUES: usize = 4;
    pub const TAG_SETS: usize = 5;
    pub const RULES: usize = 6;

    // Compile-time assertion: ensure NUM_SECTIONS matches the highest section index + 1
    const _: () = assert!(
        super::NUM_SECTIONS == RULES + 1,
        "NUM_SECTIONS must equal the number of section constants"
    );
}
