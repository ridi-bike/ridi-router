use anyhow::Result;
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
    use super::super::format::TileBounds;

    #[test]
    fn test_invalid_magic() {
        let header = RmdfHeader {
            magic: *b"XXXX",
            version: 1,
            tile_bounds: TileBounds {
                lat_min: 0.0,
                lat_max: 1.0,
                lon_min: 0.0,
                lon_max: 1.0,
            },
            point_count: 0,
            line_count: 0,
            spatial_grid_cell_count: 0,
            tag_value_count: 0,
            tag_set_count: 0,
            rule_count: 0,
            section_offsets: [0; 7],
        };

        assert!(validate_header(&header).is_err());
    }

    #[test]
    fn test_invalid_version() {
        let header = RmdfHeader {
            magic: *b"RMDF",
            version: 999,
            tile_bounds: TileBounds {
                lat_min: 0.0,
                lat_max: 1.0,
                lon_min: 0.0,
                lon_max: 1.0,
            },
            point_count: 0,
            line_count: 0,
            spatial_grid_cell_count: 0,
            tag_value_count: 0,
            tag_set_count: 0,
            rule_count: 0,
            section_offsets: [0; 7],
        };

        assert!(validate_header(&header).is_err());
    }

    #[test]
    fn test_valid_header() {
        let header = RmdfHeader {
            magic: *b"RMDF",
            version: 1,
            tile_bounds: TileBounds {
                lat_min: 56.0,
                lat_max: 57.0,
                lon_min: 24.0,
                lon_max: 25.0,
            },
            point_count: 100,
            line_count: 200,
            spatial_grid_cell_count: 50,
            tag_value_count: 10,
            tag_set_count: 20,
            rule_count: 5,
            section_offsets: [144, 1000, 2000, 3000, 4000, 5000, 6000],
        };

        assert!(validate_header(&header).is_ok());
    }
}
