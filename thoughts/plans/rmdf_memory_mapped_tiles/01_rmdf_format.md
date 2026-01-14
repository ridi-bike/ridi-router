# Phase 1: RMDF Format & Basic I/O

## Overview

Define the binary RMDF format specification and implement memory-mapped file loading with validation. This phase establishes the foundation for zero-copy data access without any tile generation yet.

**Goals:**
- Define all binary structs with #[repr(C)] for predictable layout
- Implement memory-mapped file reading using memmap2
- Implement validation (magic number, version, checksum)
- Write comprehensive unit tests for format parsing

## Changes Required

### 1. Add Dependencies

**File**: `Cargo.toml`

**Changes**: Add new dependencies for memory mapping and zero-copy casting

```toml
[dependencies]
# Add these new dependencies
memmap2 = "0.9"      # Memory-mapped file I/O
bytemuck = "1.14"    # Safe zero-copy casting (#[derive(Pod, Zeroable)])
```

**Rationale**: memmap2 provides cross-platform memory-mapped file support. bytemuck enables safe zero-copy casting from byte slices to structs.

### 2. Create RMDF Module Structure

**File**: `src/rmdf/mod.rs` (new file)

**Changes**: Create module structure for RMDF implementation

```rust
pub mod format;
pub mod validation;
pub mod io;

pub use format::*;
pub use validation::*;
pub use io::*;
```

**Rationale**: Separates concerns: format definitions, validation logic, and I/O operations.

### 3. Define Binary Format Structures

**File**: `src/rmdf/format.rs` (new file)

**Changes**: Define all RMDF binary structures

```rust
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

// Tile coordinates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileId {
    pub col: u16,  // 0-359 (longitude-based)
    pub row: u16,  // 0-179 (latitude-based)
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

// RMDF file header (64 bytes total)
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct RmdfHeader {
    pub magic: [u8; 4],                // b"RMDF"
    pub version: u32,                  // Format version (1)
    pub tile_bounds: TileBounds,       // 16 bytes
    pub point_count: u64,
    pub line_count: u64,
    pub spatial_grid_cell_count: u32,
    pub tag_value_count: u32,
    pub tag_set_count: u32,
    pub rule_count: u32,
    pub section_offsets: [u64; 7],     // Offsets to each section
    // checksum stored separately at end of file (not in header)
}

impl RmdfHeader {
    pub const SIZE: usize = 144; // Calculate actual size
    pub const MAGIC: [u8; 4] = *b"RMDF";
    pub const VERSION: u32 = 1;
}

// Spatial index grid cell entry (16 bytes)
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct GridCellEntry {
    pub cell_id: u32,           // Encoded (lat_rounded << 16 | lon_rounded)
    pub points_offset: u64,     // Offset into Points Section
    pub points_count: u32,
    pub _padding: u32,
}

impl GridCellEntry {
    pub fn encode_cell_id(lat: f32, lon: f32, precision: u32) -> u32 {
        let lat_rounded = (lat * precision as f32).round() as i16;
        let lon_rounded = (lon * precision as f32).round() as i16;
        ((lat_rounded as u32) << 16) | (lon_rounded as u32 & 0xFFFF)
    }
}

// Point record (48 bytes)
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct PointRecord {
    pub osm_id: u64,
    pub lat: f32,
    pub lon: f32,
    pub lines_offset: u64,      // Offset into Line References Section
    pub lines_count: u32,
    pub rules_offset: u64,      // Offset into Rules Section
    pub rules_count: u32,
    pub flags: u16,             // bit 0: residential_in_proximity, bit 1: nogo_area
    pub _padding: [u8; 6],
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

// Line record (48 bytes - updated for alignment)
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct LineRecord {
    pub point_a_osm_id: u64,
    pub point_a_lat: f32,
    pub point_a_lon: f32,
    pub point_b_osm_id: u64,
    pub point_b_lat: f32,
    pub point_b_lon: f32,
    pub direction: u8,          // 0=BothWays, 1=OneWay, 2=Roundabout
    pub tag_set_index: u32,     // Index into Tag Sets Section
    pub _padding: [u8; 3],
}

// String entry in tag values section (12 bytes)
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct StringEntry {
    pub offset: u64,            // Offset into string data pool
    pub length: u32,
    pub _padding: u32,
}

// Tag set record (20 bytes)
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct TagSetRecord {
    pub name_idx: u32,          // 0xFFFFFFFF = none
    pub hw_ref_idx: u32,
    pub highway_idx: u32,
    pub surface_idx: u32,
    pub smoothness_idx: u32,
}

impl TagSetRecord {
    pub const NONE: u32 = 0xFFFFFFFF;
}

// Rule record (40 bytes)
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct RuleRecord {
    pub from_lines_offset: u64,
    pub from_lines_count: u32,
    pub to_lines_offset: u64,
    pub to_lines_count: u32,
    pub rule_type: u8,          // 0=OnlyAllowed, 1=NotAllowed
    pub _padding: [u8; 7],
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
}
```

**Rationale**:
- #[repr(C)] ensures predictable memory layout for zero-copy access
- Pod + Zeroable traits from bytemuck enable safe casting
- All structs padded to 8-byte alignment for performance
- Denormalized coordinates in LineRecord enable border detection without lookups

### 4. Implement Memory-Mapped I/O

**File**: `src/rmdf/io.rs` (new file)

**Changes**: Implement file loading and section access

```rust
use anyhow::{Context, Result};
use bytemuck::{cast_slice, cast_ref, try_cast_slice};
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;

use super::format::*;

pub struct MappedTile {
    _file: File,        // Keep file handle alive
    mmap: Mmap,
    pub header: &'static RmdfHeader,
    pub tile_id: TileId,
}

impl MappedTile {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path.as_ref())
            .with_context(|| format!("Failed to open RMDF file: {:?}", path.as_ref()))?;

        let mmap = unsafe {
            memmap2::MmapOptions::new()
                .map(&file)
                .context("Failed to memory-map file")?
        };

        // Validate minimum size
        if mmap.len() < RmdfHeader::SIZE {
            anyhow::bail!("File too small to contain header");
        }

        // Cast header (zero-copy)
        let header: &RmdfHeader = cast_ref(&mmap[0..RmdfHeader::SIZE]);

        // Extract tile ID from filename
        let filename = path.as_ref()
            .file_name()
            .and_then(|n| n.to_str())
            .context("Invalid filename")?;
        let tile_id = Self::parse_tile_id(filename)?;

        // SAFETY: We keep the File and Mmap alive, so the reference is valid
        // for the lifetime of MappedTile
        let header_static: &'static RmdfHeader = unsafe {
            &*(header as *const RmdfHeader)
        };

        Ok(Self {
            _file: file,
            mmap,
            header: header_static,
            tile_id,
        })
    }

    fn parse_tile_id(filename: &str) -> Result<TileId> {
        // Parse "tile_204_146.rmdf" -> TileId { col: 204, row: 146 }
        let parts: Vec<&str> = filename.strip_suffix(".rmdf")
            .context("Filename must end with .rmdf")?
            .strip_prefix("tile_")
            .context("Filename must start with tile_")?
            .split('_')
            .collect();

        if parts.len() != 2 {
            anyhow::bail!("Invalid tile filename format");
        }

        let col = parts[0].parse().context("Invalid column number")?;
        let row = parts[1].parse().context("Invalid row number")?;

        Ok(TileId { col, row })
    }

    pub fn get_spatial_index(&self) -> Result<&[GridCellEntry]> {
        let offset = self.header.section_offsets[section::SPATIAL_INDEX] as usize;
        let count = self.header.spatial_grid_cell_count as usize;
        let size = count * std::mem::size_of::<GridCellEntry>();

        let slice = self.mmap.get(offset..offset + size)
            .context("Spatial index section out of bounds")?;

        cast_slice(slice)
    }

    pub fn get_points(&self) -> Result<&[PointRecord]> {
        let offset = self.header.section_offsets[section::POINTS] as usize;
        let count = self.header.point_count as usize;
        let size = count * std::mem::size_of::<PointRecord>();

        let slice = self.mmap.get(offset..offset + size)
            .context("Points section out of bounds")?;

        cast_slice(slice)
    }

    pub fn get_lines(&self) -> Result<&[LineRecord]> {
        let offset = self.header.section_offsets[section::LINES] as usize;
        let count = self.header.line_count as usize;
        let size = count * std::mem::size_of::<LineRecord>();

        let slice = self.mmap.get(offset..offset + size)
            .context("Lines section out of bounds")?;

        cast_slice(slice)
    }

    pub fn get_line_refs(&self) -> Result<&[u64]> {
        let offset = self.header.section_offsets[section::LINE_REFS] as usize;
        // Line refs array continues until Tag Values section
        let next_offset = self.header.section_offsets[section::TAG_VALUES] as usize;

        let slice = self.mmap.get(offset..next_offset)
            .context("Line refs section out of bounds")?;

        cast_slice(slice)
    }

    pub fn get_tag_set(&self, index: u32) -> Result<&TagSetRecord> {
        let offset = self.header.section_offsets[section::TAG_SETS] as usize;
        let record_offset = offset + (index as usize * std::mem::size_of::<TagSetRecord>());

        let slice = self.mmap.get(record_offset..record_offset + std::mem::size_of::<TagSetRecord>())
            .context("Tag set out of bounds")?;

        Ok(cast_ref(slice))
    }

    pub fn get_tag_value(&self, index: u32) -> Result<&str> {
        let offset = self.header.section_offsets[section::TAG_VALUES] as usize;
        let entry_offset = offset + (index as usize * std::mem::size_of::<StringEntry>());

        let entry_slice = self.mmap.get(entry_offset..entry_offset + std::mem::size_of::<StringEntry>())
            .context("Tag value entry out of bounds")?;
        let entry: &StringEntry = cast_ref(entry_slice);

        // String data pool starts after all StringEntry records
        let pool_start = offset + (self.header.tag_value_count as usize * std::mem::size_of::<StringEntry>());
        let string_offset = pool_start + entry.offset as usize;

        let string_slice = self.mmap.get(string_offset..string_offset + entry.length as usize)
            .context("Tag value string out of bounds")?;

        std::str::from_utf8(string_slice).context("Invalid UTF-8 in tag value")
    }
}
```

**Rationale**: Provides zero-copy access to all sections. Lifetime management ensures memory safety.

### 5. Implement Validation Logic

**File**: `src/rmdf/validation.rs` (new file)

**Changes**: Implement magic, version, and checksum validation

```rust
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use super::format::RmdfHeader;

pub fn validate_header(header: &RmdfHeader) -> Result<()> {
    // 1. Magic number check
    if &header.magic != b"RMDF" {
        anyhow::bail!(
            "Invalid magic number: expected {:?}, got {:?}",
            b"RMDF",
            header.magic
        );
    }

    // 2. Version check
    if header.version != RmdfHeader::VERSION {
        anyhow::bail!(
            "Incompatible RMDF version: expected {}, got {}",
            RmdfHeader::VERSION,
            header.version
        );
    }

    // 3. Bounds validation
    if header.tile_bounds.lat_min < -90.0 || header.tile_bounds.lat_max > 90.0 {
        anyhow::bail!("Invalid latitude bounds");
    }
    if header.tile_bounds.lon_min < -180.0 || header.tile_bounds.lon_max > 180.0 {
        anyhow::bail!("Invalid longitude bounds");
    }

    Ok(())
}

pub fn validate_checksum(file_bytes: &[u8]) -> Result<()> {
    if file_bytes.len() < 32 {
        anyhow::bail!("File too small to contain checksum");
    }

    // Checksum is last 32 bytes
    let (data, stored_checksum) = file_bytes.split_at(file_bytes.len() - 32);

    // Compute SHA256 of everything except checksum
    let mut hasher = Sha256::new();
    hasher.update(data);
    let computed_checksum = hasher.finalize();

    if stored_checksum != computed_checksum.as_slice() {
        anyhow::bail!("Checksum mismatch: file may be corrupted");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_magic() {
        let mut header = RmdfHeader {
            magic: *b"XXXX",
            version: 1,
            ..unsafe { std::mem::zeroed() }
        };

        assert!(validate_header(&header).is_err());
    }

    #[test]
    fn test_invalid_version() {
        let mut header = RmdfHeader {
            magic: *b"RMDF",
            version: 999,
            ..unsafe { std::mem::zeroed() }
        };

        assert!(validate_header(&header).is_err());
    }
}
```

**Rationale**: Catches corrupted or incompatible files early. Checksum validation ensures data integrity.

### 6. Update Main Module

**File**: `src/lib.rs`

**Changes**: Add rmdf module

```rust
pub mod rmdf;  // Add this line
pub mod map_data;
pub mod osm_data;
pub mod router;
// ... existing modules
```

**Rationale**: Makes RMDF module accessible throughout the codebase.

## Success Criteria

### Automated Verification

- [ ] Unit tests pass: `cargo test rmdf::format`
- [ ] Unit tests pass: `cargo test rmdf::validation`
- [ ] Unit tests pass: `cargo test rmdf::io`
- [ ] Type checking passes: `cargo check`
- [ ] No compiler warnings: `cargo clippy -- -D warnings`

### Manual Verification

- [ ] Can manually create a minimal RMDF file (header + checksum)
- [ ] MappedTile successfully loads and validates the file
- [ ] Invalid magic/version/checksum correctly rejected
- [ ] Zero-copy casting works (no segfaults)

## Dependencies

- **Depends on**: None - can start immediately
- **Blocks**: Phase 2 (needs format definitions for tile generation)

## Risks & Mitigations

**Risk**: Alignment issues on different platforms
- **Mitigation**: Use #[repr(C)] and explicit padding, test on Linux/macOS/Windows

**Risk**: Endianness differences
- **Mitigation**: Document little-endian requirement, validate on ARM

**Risk**: Unsafe code in lifetime management
- **Mitigation**: Careful SAFETY comments, keep File/Mmap alive with MappedTile

## Notes

- All structs use little-endian byte order (native on target platforms)
- File format is versioned (header.version) for future evolution
- Checksum stored at end of file (not in header) to simplify writing
- Grid cell ID encoding uses 16-bit precision matching existing PointGrid (0.01°)
