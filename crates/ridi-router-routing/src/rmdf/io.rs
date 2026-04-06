use anyhow::{Context, Result};
use bytemuck::{cast_slice, pod_read_unaligned, try_from_bytes};
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;

use super::format::*;

pub struct MappedTile {
    _file: File,
    mmap: Mmap,
    data_end: usize,
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

        if mmap.len() < RmdfHeader::SIZE {
            anyhow::bail!("File too small to contain header");
        }

        let header: &RmdfHeader = try_from_bytes(&mmap[0..RmdfHeader::SIZE])
            .map_err(|_| anyhow::anyhow!("Failed to cast header bytes"))?;

        let filename = path
            .as_ref()
            .file_name()
            .and_then(|n| n.to_str())
            .context("Invalid filename")?;
        let tile_id = Self::parse_tile_id(filename)?;

        // SAFETY: We keep the File and Mmap alive, so the reference is valid
        // for the lifetime of MappedTile
        let header_static: &'static RmdfHeader = unsafe { &*(header as *const RmdfHeader) };

        let data_end = if super::validation::validate_checksum(&mmap).is_ok() {
            mmap.len() - 32
        } else {
            mmap.len()
        };

        Ok(Self {
            _file: file,
            mmap,
            data_end,
            header: header_static,
            tile_id,
        })
    }

    fn parse_tile_id(filename: &str) -> Result<TileId> {
        // Parse "tile_204_146.rmdf" -> TileId { col: 204, row: 146 }
        let parts: Vec<&str> = filename
            .strip_suffix(".rmdf")
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

        let slice = self
            .mmap
            .get(offset..offset + size)
            .context("Spatial index section out of bounds")?;

        Ok(cast_slice(slice))
    }

    pub fn get_points(&self) -> Result<&[PointRecord]> {
        let offset = self.header.section_offsets[section::POINTS] as usize;
        let count = self.header.point_count as usize;
        let size = count * std::mem::size_of::<PointRecord>();

        let slice = self
            .mmap
            .get(offset..offset + size)
            .context("Points section out of bounds")?;

        Ok(cast_slice(slice))
    }

    pub fn get_lines(&self) -> Result<&[LineRecord]> {
        let offset = self.header.section_offsets[section::LINES] as usize;
        let count = self.header.line_count as usize;
        let size = count * std::mem::size_of::<LineRecord>();

        let slice = self
            .mmap
            .get(offset..offset + size)
            .context("Lines section out of bounds")?;

        Ok(cast_slice(slice))
    }

    pub fn get_line_refs(&self) -> Result<&[u64]> {
        let offset = self.header.section_offsets[section::LINE_REFS] as usize;
        // Line refs array continues until Tag Values section
        let next_offset = self.header.section_offsets[section::TAG_VALUES] as usize;

        let slice = self
            .mmap
            .get(offset..next_offset)
            .context("Line refs section out of bounds")?;

        Ok(cast_slice(slice))
    }

    pub fn get_tag_set(&self, index: u32) -> Result<TagSetRecord> {
        let offset = self.header.section_offsets[section::TAG_SETS] as usize;
        let record_offset = offset + (index as usize * std::mem::size_of::<TagSetRecord>());

        let slice = self
            .mmap
            .get(record_offset..record_offset + std::mem::size_of::<TagSetRecord>())
            .context("Tag set out of bounds")?;

        Ok(pod_read_unaligned(slice))
    }

    pub fn get_tag_value(&self, index: u32) -> Result<&str> {
        let offset = self.header.section_offsets[section::TAG_VALUES] as usize;
        let entry_offset = offset + (index as usize * std::mem::size_of::<StringEntry>());

        let entry_slice = self
            .mmap
            .get(entry_offset..entry_offset + std::mem::size_of::<StringEntry>())
            .context("Tag value entry out of bounds")?;
        let entry: &StringEntry = try_from_bytes(entry_slice)
            .map_err(|_| anyhow::anyhow!("Failed to cast string entry"))?;

        // String data pool starts after all StringEntry records
        let pool_start =
            offset + (self.header.tag_value_count as usize * std::mem::size_of::<StringEntry>());
        let string_offset = pool_start + entry.offset as usize;

        let string_slice = self
            .mmap
            .get(string_offset..string_offset + entry.length as usize)
            .context("Tag value string out of bounds")?;

        std::str::from_utf8(string_slice).context("Invalid UTF-8 in tag value")
    }

    pub fn get_rules(&self) -> Result<Vec<RuleRecord>> {
        let offset = self.header.section_offsets[section::RULES] as usize;
        let count = self.header.rule_count as usize;
        let size = count * std::mem::size_of::<RuleRecord>();

        let slice = self
            .mmap
            .get(offset..offset + size)
            .context("Rules section out of bounds")?;

        Ok(slice
            .chunks_exact(std::mem::size_of::<RuleRecord>())
            .map(pod_read_unaligned)
            .collect())
    }

    pub fn get_rule_line_refs_payload(&self) -> Result<Vec<u64>> {
        let rules_offset = self.header.section_offsets[section::RULES] as usize;
        let rule_record_bytes = self.header.rule_count as usize * std::mem::size_of::<RuleRecord>();
        let payload_offset = rules_offset + rule_record_bytes;

        let slice = self
            .mmap
            .get(payload_offset..self.data_end)
            .context("Rule line refs payload out of bounds")?;

        if slice.len() % std::mem::size_of::<u64>() != 0 {
            anyhow::bail!(
                "Rule line refs payload size {} is not a multiple of {}",
                slice.len(),
                std::mem::size_of::<u64>()
            );
        }

        Ok(slice
            .chunks_exact(std::mem::size_of::<u64>())
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::MappedTile;
    use ridi_router_common::format::{GridCellEntry, LineRecord, PointRecord, TagSetRecord};
    use ridi_router_test_support::rmdf::{
        unique_test_dir, write_tile, TileSpec, SYNTHETIC_TILE_BOUNDS, SYNTHETIC_TILE_ID,
    };

    #[test]
    fn loads_synthetic_tile_with_spatial_index_and_tags() {
        let dir = unique_test_dir("rmdf-io-tag-smoke");
        let lat = 10.5;
        let lon = 20.5;
        let highway_idx = 0;
        let surface_idx = 1;
        let smoothness_idx = 2;

        let tile_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: SYNTHETIC_TILE_ID,
                bounds: SYNTHETIC_TILE_BOUNDS,
                spatial_index: vec![GridCellEntry {
                    cell_id: GridCellEntry::encode_cell_id(lat, lon, 100),
                    _padding1: 0,
                    points_offset: 0,
                    points_count: 1,
                    _padding2: 0,
                }],
                points: vec![PointRecord {
                    osm_id: 1,
                    lat,
                    lon,
                    lines_offset: 0,
                    lines_count: 1,
                    _padding1: 0,
                    rules_offset: 0,
                    rules_count: 0,
                    flags: 0,
                    _padding2: 0,
                }],
                lines: vec![LineRecord {
                    point_a_osm_id: 1,
                    point_a_lat: lat,
                    point_a_lon: lon,
                    point_b_osm_id: 2,
                    point_b_lat: lat + 0.01,
                    point_b_lon: lon + 0.01,
                    direction: 0,
                    _padding1: 0,
                    _padding2: 0,
                    tag_set_index: 0,
                }],
                line_refs: vec![0],
                tag_values: vec![
                    "secondary".to_string(),
                    "gravel".to_string(),
                    "bad".to_string(),
                ],
                tag_sets: vec![TagSetRecord {
                    name_idx: TagSetRecord::NONE,
                    hw_ref_idx: TagSetRecord::NONE,
                    highway_idx,
                    surface_idx,
                    smoothness_idx,
                }],
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        let tile = MappedTile::load(&tile_path).unwrap();

        assert_eq!(tile.header.spatial_grid_cell_count, 1);
        assert_eq!(tile.header.tag_value_count, 3);
        assert_eq!(tile.header.tag_set_count, 1);

        let spatial_index = tile.get_spatial_index().unwrap();
        assert_eq!(spatial_index.len(), 1);
        assert_eq!(
            spatial_index[0].cell_id,
            GridCellEntry::encode_cell_id(lat, lon, 100)
        );
        assert_eq!(spatial_index[0].points_offset, 0);
        assert_eq!(spatial_index[0].points_count, 1);

        assert_eq!(tile.get_tag_value(highway_idx).unwrap(), "secondary");
        assert_eq!(tile.get_tag_value(surface_idx).unwrap(), "gravel");
        assert_eq!(tile.get_tag_value(smoothness_idx).unwrap(), "bad");

        let tag_set = tile.get_tag_set(0).unwrap();
        assert_eq!(tag_set.name_idx, TagSetRecord::NONE);
        assert_eq!(tag_set.hw_ref_idx, TagSetRecord::NONE);
        assert_eq!(tag_set.highway_idx, highway_idx);
        assert_eq!(tag_set.surface_idx, surface_idx);
        assert_eq!(tag_set.smoothness_idx, smoothness_idx);
    }
}
