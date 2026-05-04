use std::{
    collections::{HashMap, HashSet},
    fs::File,
    path::Path,
};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use super::{
    format::{RmdfHeader, TileId},
    io::MappedTile,
};
use ridi_router_common::manifest::TileManifest;

#[allow(dead_code)]
const OVERLAP_MARGIN_DEGREES: f32 = 0.005;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoundaryIntegrityIssueKind {
    MissingOverlapPointCopy,
    MissingLineEndpointPointCopy,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BoundaryIntegrityIssue {
    pub kind: BoundaryIntegrityIssueKind,
    pub source_tile_id: TileId,
    pub missing_tile_id: TileId,
    pub point_osm_id: u64,
    pub line_index: Option<usize>,
}

pub fn validate_header(header: &RmdfHeader) -> Result<()> {
    if &header.magic != b"RMDF" {
        anyhow::bail!(
            "Invalid magic number: expected {:?}, got {:?}",
            b"RMDF",
            header.magic
        );
    }

    if header.version != RmdfHeader::VERSION {
        anyhow::bail!(
            "Incompatible RMDF version: expected {}, got {}",
            RmdfHeader::VERSION,
            header.version
        );
    }

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

    let (data, stored_checksum) = file_bytes.split_at(file_bytes.len() - 32);

    let mut hasher = Sha256::new();
    hasher.update(data);
    let computed_checksum = hasher.finalize();

    if stored_checksum != computed_checksum.as_slice() {
        anyhow::bail!("Checksum mismatch: file may be corrupted");
    }

    Ok(())
}

#[allow(dead_code)]
pub fn scan_boundary_overlap_integrity(tile_dir: &Path) -> Result<Vec<BoundaryIntegrityIssue>> {
    let manifest_path = tile_dir.join("manifest.json");
    let manifest_file =
        File::open(&manifest_path).with_context(|| format!("Failed to open {manifest_path:?}"))?;
    let manifest: TileManifest =
        serde_json::from_reader(manifest_file).context("Failed to parse manifest.json")?;

    scan_boundary_overlap_integrity_from_manifest(tile_dir, &manifest)
}

#[allow(dead_code)]
fn scan_boundary_overlap_integrity_from_manifest(
    tile_dir: &Path,
    manifest: &TileManifest,
) -> Result<Vec<BoundaryIntegrityIssue>> {
    let available_tiles = manifest
        .tiles
        .iter()
        .map(|tile| TileId {
            col: tile.col,
            row: tile.row,
        })
        .collect::<HashSet<_>>();
    let mut tile_point_ids = HashMap::<TileId, HashSet<u64>>::new();

    for tile in &manifest.tiles {
        let tile_id = TileId {
            col: tile.col,
            row: tile.row,
        };
        let mapped_tile = MappedTile::load(tile_dir.join(&tile.filename))?;
        let point_ids = mapped_tile
            .get_points()?
            .iter()
            .map(|point| point.osm_id)
            .collect::<HashSet<_>>();
        tile_point_ids.insert(tile_id, point_ids);
    }

    let mut issues = Vec::new();
    let mut seen_issues = HashSet::new();

    for tile in &manifest.tiles {
        let tile_id = TileId {
            col: tile.col,
            row: tile.row,
        };
        let mapped_tile = MappedTile::load(tile_dir.join(&tile.filename))?;
        let current_tile_points = tile_point_ids
            .get(&tile_id)
            .with_context(|| format!("Point index missing for tile {tile_id:?}"))?;

        for point in mapped_tile.get_points()? {
            for candidate_tile in
                overlap_candidate_tiles_for_coords(manifest.tile_size_degrees, point.lat, point.lon)
            {
                if candidate_tile == tile_id || !available_tiles.contains(&candidate_tile) {
                    continue;
                }

                let has_copy = tile_point_ids
                    .get(&candidate_tile)
                    .is_some_and(|point_ids| point_ids.contains(&point.osm_id));
                if has_copy {
                    continue;
                }

                let issue = BoundaryIntegrityIssue {
                    kind: BoundaryIntegrityIssueKind::MissingOverlapPointCopy,
                    source_tile_id: tile_id,
                    missing_tile_id: candidate_tile,
                    point_osm_id: point.osm_id,
                    line_index: None,
                };
                if seen_issues.insert(issue.clone()) {
                    issues.push(issue);
                }
            }
        }

        for (line_index, line) in mapped_tile.get_lines()?.iter().enumerate() {
            for (point_osm_id, lat, lon) in [
                (line.point_a_osm_id, line.point_a_lat, line.point_a_lon),
                (line.point_b_osm_id, line.point_b_lat, line.point_b_lon),
            ] {
                let candidate_tiles =
                    overlap_candidate_tiles_for_coords(manifest.tile_size_degrees, lat, lon);
                if !candidate_tiles.contains(&tile_id)
                    || current_tile_points.contains(&point_osm_id)
                {
                    continue;
                }

                let issue = BoundaryIntegrityIssue {
                    kind: BoundaryIntegrityIssueKind::MissingLineEndpointPointCopy,
                    source_tile_id: tile_id,
                    missing_tile_id: tile_id,
                    point_osm_id,
                    line_index: Some(line_index),
                };
                if seen_issues.insert(issue.clone()) {
                    issues.push(issue);
                }
            }
        }
    }

    Ok(issues)
}

#[allow(dead_code)]
fn overlap_candidate_tiles_for_coords(tile_size_degrees: f32, lat: f32, lon: f32) -> Vec<TileId> {
    let canonical_tile = TileId::from_coords(lat, lon, tile_size_degrees);
    let lon_min = (canonical_tile.col as f32 * tile_size_degrees) - 180.0;
    let lon_max = lon_min + tile_size_degrees;
    let lat_min = (canonical_tile.row as f32 * tile_size_degrees) - 90.0;
    let lat_max = lat_min + tile_size_degrees;

    let within_west_overlap = canonical_tile.col > 0 && (lon - lon_min) <= OVERLAP_MARGIN_DEGREES;
    let within_east_overlap = (lon_max - lon) <= OVERLAP_MARGIN_DEGREES;
    let within_south_overlap = canonical_tile.row > 0 && (lat - lat_min) <= OVERLAP_MARGIN_DEGREES;
    let within_north_overlap = (lat_max - lat) <= OVERLAP_MARGIN_DEGREES;

    let mut candidates = vec![canonical_tile];
    let mut push_unique = |col: i32, row: i32| {
        if !(0..=u16::MAX as i32).contains(&col) || !(0..=u16::MAX as i32).contains(&row) {
            return;
        }

        let candidate = TileId {
            col: col as u16,
            row: row as u16,
        };
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    };

    if within_north_overlap {
        push_unique(canonical_tile.col as i32, canonical_tile.row as i32 + 1);
    }
    if within_south_overlap {
        push_unique(canonical_tile.col as i32, canonical_tile.row as i32 - 1);
    }
    if within_east_overlap {
        push_unique(canonical_tile.col as i32 + 1, canonical_tile.row as i32);
    }
    if within_west_overlap {
        push_unique(canonical_tile.col as i32 - 1, canonical_tile.row as i32);
    }
    if within_north_overlap && within_east_overlap {
        push_unique(canonical_tile.col as i32 + 1, canonical_tile.row as i32 + 1);
    }
    if within_north_overlap && within_west_overlap {
        push_unique(canonical_tile.col as i32 - 1, canonical_tile.row as i32 + 1);
    }
    if within_south_overlap && within_east_overlap {
        push_unique(canonical_tile.col as i32 + 1, canonical_tile.row as i32 - 1);
    }
    if within_south_overlap && within_west_overlap {
        push_unique(canonical_tile.col as i32 - 1, canonical_tile.row as i32 - 1);
    }

    candidates
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::format::TileBounds;
    use super::*;
    use ridi_router_test_support::rmdf::{
        empty_neighbors, manifest_bounds, unique_test_dir, write_manifest, write_tile, LineRecord,
        PointRecord, TileManifest, TileMetadata, TileSpec,
    };

    fn bounds_for_tile(tile_id: TileId) -> TileBounds {
        TileBounds {
            lat_min: (tile_id.row as f32) - 90.0,
            lat_max: (tile_id.row as f32) - 89.0,
            lon_min: (tile_id.col as f32) - 180.0,
            lon_max: (tile_id.col as f32) - 179.0,
        }
    }

    fn write_overlap_scan_fixture(
        prefix: &str,
        include_boundary_copy_in_tile_a: bool,
    ) -> std::path::PathBuf {
        let dir = unique_test_dir(prefix);
        fs::create_dir_all(&dir).unwrap();

        let tile_a = TileId { col: 200, row: 100 };
        let tile_b = TileId { col: 201, row: 100 };
        let bounds_a = bounds_for_tile(tile_a);
        let bounds_b = bounds_for_tile(tile_b);
        let boundary_osm_id = 50_000;
        let local_osm_id = 50_001;

        let mut tile_a_points = vec![PointRecord {
            osm_id: local_osm_id,
            lat: 10.50,
            lon: 20.90,
            lines_offset: 0,
            lines_count: 1,
            _padding1: 0,
            rules_offset: 0,
            rules_count: 0,
            flags: 0,
            _padding2: 0,
        }];
        if include_boundary_copy_in_tile_a {
            tile_a_points.push(PointRecord {
                osm_id: boundary_osm_id,
                lat: 10.50,
                lon: 20.998,
                lines_offset: 1,
                lines_count: 1,
                _padding1: 0,
                rules_offset: 0,
                rules_count: 0,
                flags: 0,
                _padding2: 0,
            });
        }

        let tile_a_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: tile_a,
                bounds: bounds_a,
                spatial_index: Vec::new(),
                points: tile_a_points,
                lines: vec![LineRecord {
                    point_a_osm_id: local_osm_id,
                    point_a_lat: 10.50,
                    point_a_lon: 20.90,
                    point_b_osm_id: boundary_osm_id,
                    point_b_lat: 10.50,
                    point_b_lon: 20.998,
                    direction: 0,
                    _padding1: 0,
                    _padding2: 0,
                    tag_set_index: 0,
                }],
                line_refs: if include_boundary_copy_in_tile_a {
                    vec![0, 0]
                } else {
                    vec![0]
                },
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        let tile_b_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: tile_b,
                bounds: bounds_b,
                spatial_index: Vec::new(),
                points: vec![PointRecord {
                    osm_id: boundary_osm_id,
                    lat: 10.50,
                    lon: 20.998,
                    lines_offset: 0,
                    lines_count: 0,
                    _padding1: 0,
                    rules_offset: 0,
                    rules_count: 0,
                    flags: 0,
                    _padding2: 0,
                }],
                lines: Vec::new(),
                line_refs: Vec::new(),
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        write_manifest(
            &dir,
            &TileManifest {
                version: "test".to_string(),
                tile_size_degrees: 1.0,
                format_version: 1,
                generated_at: "2026-04-05T00:00:00Z".to_string(),
                source_files: vec!["synthetic".to_string()],
                tiles: vec![
                    TileMetadata {
                        filename: tile_a.to_filename(),
                        col: tile_a.col,
                        row: tile_a.row,
                        bounds: manifest_bounds(bounds_a),
                        neighbors: empty_neighbors(),
                        size_bytes: fs::metadata(&tile_a_path).unwrap().len(),
                        point_count: if include_boundary_copy_in_tile_a {
                            2
                        } else {
                            1
                        },
                        line_count: 1,
                        checksum: "sha256:validation-overlap-a".to_string(),
                        military_geojson_filename: None,
                    },
                    TileMetadata {
                        filename: tile_b.to_filename(),
                        col: tile_b.col,
                        row: tile_b.row,
                        bounds: manifest_bounds(bounds_b),
                        neighbors: empty_neighbors(),
                        size_bytes: fs::metadata(&tile_b_path).unwrap().len(),
                        point_count: 1,
                        line_count: 0,
                        checksum: "sha256:validation-overlap-b".to_string(),
                        military_geojson_filename: None,
                    },
                ],
            },
        );

        dir
    }

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

    #[test]
    fn test_boundary_overlap_scan_reports_missing_point_copy_and_invalid_line_endpoint() {
        let dir = write_overlap_scan_fixture("validation-overlap-missing", false);

        let issues = scan_boundary_overlap_integrity(&dir).unwrap();

        assert!(issues.contains(&BoundaryIntegrityIssue {
            kind: BoundaryIntegrityIssueKind::MissingOverlapPointCopy,
            source_tile_id: TileId { col: 201, row: 100 },
            missing_tile_id: TileId { col: 200, row: 100 },
            point_osm_id: 50_000,
            line_index: None,
        }));
        assert!(issues.contains(&BoundaryIntegrityIssue {
            kind: BoundaryIntegrityIssueKind::MissingLineEndpointPointCopy,
            source_tile_id: TileId { col: 200, row: 100 },
            missing_tile_id: TileId { col: 200, row: 100 },
            point_osm_id: 50_000,
            line_index: Some(0),
        }));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_boundary_overlap_scan_passes_when_overlap_copy_exists() {
        let dir = write_overlap_scan_fixture("validation-overlap-clean", true);

        let issues = scan_boundary_overlap_integrity(&dir).unwrap();

        assert!(issues.is_empty(), "unexpected issues: {issues:?}");

        fs::remove_dir_all(dir).unwrap();
    }
}
