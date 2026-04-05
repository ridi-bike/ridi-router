pub mod rmdf {
    use std::{
        fs,
        path::{Path, PathBuf},
        process,
        time::{SystemTime, UNIX_EPOCH},
    };

    pub use ridi_router_common::format::{
        GridCellEntry, LineRecord, PointRecord, RuleRecord, TagSetRecord, TileBounds, TileId,
    };
    use ridi_router_common::format::{RmdfHeader, StringEntry};
    pub use ridi_router_common::manifest::{
        TileBounds as ManifestTileBounds, TileManifest, TileMetadata, TileNeighbors,
    };

    pub const SYNTHETIC_TILE_ID: TileId = TileId { col: 200, row: 100 };
    pub const SYNTHETIC_TILE_BOUNDS: TileBounds = TileBounds {
        lat_min: 10.0,
        lat_max: 11.0,
        lon_min: 20.0,
        lon_max: 21.0,
    };
    pub const SYNTHETIC_TILE_SIZE_DEGREES: f32 = 1.0;
    pub const SYNTHETIC_START_LAT: f32 = 10.0;
    pub const SYNTHETIC_START_LON: f32 = 20.0;
    pub const SYNTHETIC_FINISH_LAT: f32 = 10.12;
    pub const SYNTHETIC_FINISH_LON: f32 = 20.0;

    #[derive(Debug, Clone)]
    pub struct TileSpec {
        pub tile_id: TileId,
        pub bounds: TileBounds,
        pub spatial_index: Vec<GridCellEntry>,
        pub points: Vec<PointRecord>,
        pub lines: Vec<LineRecord>,
        pub line_refs: Vec<u64>,
        pub tag_values: Vec<String>,
        pub tag_sets: Vec<TagSetRecord>,
        pub rules: Vec<RuleRecord>,
        pub rule_line_refs: Vec<u64>,
    }

    impl Default for TileSpec {
        fn default() -> Self {
            Self {
                tile_id: SYNTHETIC_TILE_ID,
                bounds: SYNTHETIC_TILE_BOUNDS,
                spatial_index: Vec::new(),
                points: Vec::new(),
                lines: Vec::new(),
                line_refs: Vec::new(),
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            }
        }
    }

    #[derive(Debug, Clone)]
    pub struct LinearSingleTileFixture {
        pub dir: PathBuf,
        pub rule_file: PathBuf,
        pub tile_id: TileId,
        pub start_osm_id: u64,
        pub finish_osm_id: u64,
        pub start_lat: f32,
        pub start_lon: f32,
        pub finish_lat: f32,
        pub finish_lon: f32,
    }

    #[derive(Debug, Clone)]
    pub struct MissingNeighborFixture {
        pub dir: PathBuf,
        pub tile_a: TileId,
        pub missing_tile: TileId,
        pub center_osm_id: u64,
        pub in_tile_neighbor_osm_id: u64,
        pub missing_neighbor_osm_id: u64,
    }

    pub fn unique_test_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ridi-router-{prefix}-{}-{}",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    pub fn write_manifest(dir: &Path, manifest: &TileManifest) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let manifest_path = dir.join("manifest.json");
        fs::write(&manifest_path, serde_json::to_vec(manifest).unwrap()).unwrap();
        manifest_path
    }

    pub fn write_tile(dir: &Path, spec: &TileSpec) -> PathBuf {
        fs::create_dir_all(dir).unwrap();

        let spatial_index_offset = RmdfHeader::SIZE as u64;
        let points_offset = spatial_index_offset
            + std::mem::size_of::<GridCellEntry>() as u64 * spec.spatial_index.len() as u64;
        let lines_offset =
            points_offset + std::mem::size_of::<PointRecord>() as u64 * spec.points.len() as u64;
        let line_refs_offset =
            lines_offset + std::mem::size_of::<LineRecord>() as u64 * spec.lines.len() as u64;
        let tag_values_offset =
            line_refs_offset + std::mem::size_of::<u64>() as u64 * spec.line_refs.len() as u64;
        let tag_sets_offset = tag_values_offset + tag_values_section_size(&spec.tag_values);
        let rules_offset = tag_sets_offset
            + std::mem::size_of::<TagSetRecord>() as u64 * spec.tag_sets.len() as u64;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RMDF");
        push_u32(&mut bytes, RmdfHeader::VERSION);
        write_tile_bounds(&mut bytes, spec.bounds);
        push_u64(&mut bytes, spec.points.len() as u64);
        push_u64(&mut bytes, spec.lines.len() as u64);
        push_u32(&mut bytes, spec.spatial_index.len() as u32);
        push_u32(&mut bytes, spec.tag_values.len() as u32);
        push_u32(&mut bytes, spec.tag_sets.len() as u32);
        push_u32(&mut bytes, spec.rules.len() as u32);
        for offset in [
            spatial_index_offset,
            points_offset,
            lines_offset,
            line_refs_offset,
            tag_values_offset,
            tag_sets_offset,
            rules_offset,
        ] {
            push_u64(&mut bytes, offset);
        }

        assert_eq!(bytes.len() as u64, RmdfHeader::SIZE as u64);

        for grid_cell in &spec.spatial_index {
            write_grid_cell_entry(&mut bytes, *grid_cell);
        }
        for point in &spec.points {
            write_point_record(&mut bytes, *point);
        }
        for line in &spec.lines {
            write_line_record(&mut bytes, *line);
        }
        for line_ref in &spec.line_refs {
            push_u64(&mut bytes, *line_ref);
        }
        write_tag_values_section(&mut bytes, &spec.tag_values);
        for tag_set in &spec.tag_sets {
            write_tag_set_record(&mut bytes, *tag_set);
        }
        for rule in &spec.rules {
            write_rule_record(&mut bytes, *rule);
        }
        for line_ref in &spec.rule_line_refs {
            push_u64(&mut bytes, *line_ref);
        }

        let tile_path = dir.join(spec.tile_id.to_filename());
        fs::write(&tile_path, bytes).unwrap();
        tile_path
    }

    pub fn write_default_rules_file(dir: &Path) -> PathBuf {
        fs::create_dir_all(dir).unwrap();

        let rules = serde_json::json!({
            "basic": { "step_limit": 50 },
            "generation": {
                "waypoint_generation": {
                    "start_finish": {
                        "variation_distances_m": [],
                        "variation_bearing_deg": []
                    }
                },
                "route_generation_retry": {
                    "trigger_min_route_count": 1,
                    "round_trip_adjustment_bearing_deg": [],
                    "avoid_residential": [false]
                }
            },
            "highway": null,
            "surface": null,
            "smoothness": null
        });

        let rule_file = dir.join("rules.json");
        fs::write(&rule_file, serde_json::to_vec(&rules).unwrap()).unwrap();
        rule_file
    }

    pub fn create_linear_single_tile_fixture(prefix: &str) -> LinearSingleTileFixture {
        let dir = unique_test_dir(prefix);
        fs::create_dir_all(&dir).unwrap();

        let mut points = Vec::new();
        let mut line_refs = Vec::new();
        let mut lines = Vec::new();

        for idx in 0..=12_u64 {
            let lat = 10.0 + idx as f32 * 0.01;
            let mut refs = Vec::new();
            if idx > 0 {
                refs.push(idx - 1);
            }
            if idx < 12 {
                refs.push(idx);
            }

            points.push(PointRecord {
                osm_id: 1000 + idx,
                lat,
                lon: 20.0,
                lines_offset: line_refs.len() as u64,
                lines_count: refs.len() as u32,
                _padding1: 0,
                rules_offset: 0,
                rules_count: 0,
                flags: 0,
                _padding2: 0,
            });
            line_refs.extend(refs);
        }

        for idx in 0..12_u64 {
            let point_a_lat = 10.0 + idx as f32 * 0.01;
            let point_b_lat = 10.0 + (idx + 1) as f32 * 0.01;
            lines.push(LineRecord {
                point_a_osm_id: 1000 + idx,
                point_a_lat,
                point_a_lon: 20.0,
                point_b_osm_id: 1001 + idx,
                point_b_lat,
                point_b_lon: 20.0,
                direction: 0,
                _padding1: 0,
                _padding2: 0,
                tag_set_index: 0,
            });
        }

        let point_count = points.len() as u64;
        let line_count = lines.len() as u64;
        let tile_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: SYNTHETIC_TILE_ID,
                bounds: SYNTHETIC_TILE_BOUNDS,
                spatial_index: Vec::new(),
                points,
                lines,
                line_refs,
                tag_values: vec!["secondary".to_string()],
                tag_sets: vec![TagSetRecord {
                    name_idx: TagSetRecord::NONE,
                    hw_ref_idx: TagSetRecord::NONE,
                    highway_idx: 0,
                    surface_idx: TagSetRecord::NONE,
                    smoothness_idx: TagSetRecord::NONE,
                }],
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        write_manifest(
            &dir,
            &TileManifest {
                version: "test".to_string(),
                tile_size_degrees: SYNTHETIC_TILE_SIZE_DEGREES,
                format_version: 1,
                generated_at: "2026-04-03T00:00:00Z".to_string(),
                source_files: vec!["synthetic".to_string()],
                tiles: vec![TileMetadata {
                    filename: SYNTHETIC_TILE_ID.to_filename(),
                    col: SYNTHETIC_TILE_ID.col,
                    row: SYNTHETIC_TILE_ID.row,
                    bounds: manifest_bounds(SYNTHETIC_TILE_BOUNDS),
                    neighbors: empty_neighbors(),
                    size_bytes: fs::metadata(&tile_path).unwrap().len(),
                    point_count,
                    line_count,
                    checksum: "sha256:test".to_string(),
                    military_geojson_filename: None,
                }],
            },
        );

        let rule_file = write_default_rules_file(&dir);

        LinearSingleTileFixture {
            dir,
            rule_file,
            tile_id: SYNTHETIC_TILE_ID,
            start_osm_id: 1000,
            finish_osm_id: 1012,
            start_lat: SYNTHETIC_START_LAT,
            start_lon: SYNTHETIC_START_LON,
            finish_lat: SYNTHETIC_FINISH_LAT,
            finish_lon: SYNTHETIC_FINISH_LON,
        }
    }

    pub fn create_missing_neighbor_fixture(prefix: &str) -> MissingNeighborFixture {
        let dir = unique_test_dir(prefix);
        fs::create_dir_all(&dir).unwrap();

        let tile_a = SYNTHETIC_TILE_ID;
        let missing_tile = TileId { col: 201, row: 100 };
        let center_osm_id = 2000;
        let in_tile_neighbor_osm_id = 2001;
        let missing_neighbor_osm_id = 2999;

        let points = vec![
            PointRecord {
                osm_id: center_osm_id,
                lat: 10.5,
                lon: 20.5,
                lines_offset: 0,
                lines_count: 2,
                _padding1: 0,
                rules_offset: 0,
                rules_count: 0,
                flags: 0,
                _padding2: 0,
            },
            PointRecord {
                osm_id: in_tile_neighbor_osm_id,
                lat: 10.6,
                lon: 20.6,
                lines_offset: 2,
                lines_count: 1,
                _padding1: 0,
                rules_offset: 0,
                rules_count: 0,
                flags: 0,
                _padding2: 0,
            },
        ];

        let lines = vec![
            LineRecord {
                point_a_osm_id: center_osm_id,
                point_a_lat: 10.5,
                point_a_lon: 20.5,
                point_b_osm_id: in_tile_neighbor_osm_id,
                point_b_lat: 10.6,
                point_b_lon: 20.6,
                direction: 0,
                _padding1: 0,
                _padding2: 0,
                tag_set_index: 0,
            },
            LineRecord {
                point_a_osm_id: center_osm_id,
                point_a_lat: 10.5,
                point_a_lon: 20.5,
                point_b_osm_id: missing_neighbor_osm_id,
                point_b_lat: 10.5,
                point_b_lon: 21.1,
                direction: 0,
                _padding1: 0,
                _padding2: 0,
                tag_set_index: 0,
            },
        ];

        let line_refs = vec![0, 1, 0];
        let tile_a_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: tile_a,
                bounds: SYNTHETIC_TILE_BOUNDS,
                spatial_index: Vec::new(),
                points,
                lines,
                line_refs,
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        let east_filename = missing_tile.to_filename();
        let west_filename = tile_a.to_filename();

        write_manifest(
            &dir,
            &TileManifest {
                version: "test".to_string(),
                tile_size_degrees: SYNTHETIC_TILE_SIZE_DEGREES,
                format_version: 1,
                generated_at: "2026-04-03T00:00:00Z".to_string(),
                source_files: vec!["synthetic".to_string()],
                tiles: vec![
                    TileMetadata {
                        filename: west_filename.clone(),
                        col: tile_a.col,
                        row: tile_a.row,
                        bounds: manifest_bounds(SYNTHETIC_TILE_BOUNDS),
                        neighbors: TileNeighbors {
                            north: None,
                            south: None,
                            east: Some(east_filename.clone()),
                            west: None,
                            northeast: None,
                            northwest: None,
                            southeast: None,
                            southwest: None,
                        },
                        size_bytes: fs::metadata(&tile_a_path).unwrap().len(),
                        point_count: 2,
                        line_count: 2,
                        checksum: "sha256:test-tile-a".to_string(),
                        military_geojson_filename: None,
                    },
                    TileMetadata {
                        filename: east_filename,
                        col: missing_tile.col,
                        row: missing_tile.row,
                        bounds: manifest_bounds(TileBounds {
                            lat_min: 10.0,
                            lat_max: 11.0,
                            lon_min: 21.0,
                            lon_max: 22.0,
                        }),
                        neighbors: TileNeighbors {
                            north: None,
                            south: None,
                            east: None,
                            west: Some(west_filename),
                            northeast: None,
                            northwest: None,
                            southeast: None,
                            southwest: None,
                        },
                        size_bytes: 0,
                        point_count: 0,
                        line_count: 0,
                        checksum: "sha256:missing".to_string(),
                        military_geojson_filename: None,
                    },
                ],
            },
        );

        MissingNeighborFixture {
            dir,
            tile_a,
            missing_tile,
            center_osm_id,
            in_tile_neighbor_osm_id,
            missing_neighbor_osm_id,
        }
    }

    pub fn empty_neighbors() -> TileNeighbors {
        TileNeighbors {
            north: None,
            south: None,
            east: None,
            west: None,
            northeast: None,
            northwest: None,
            southeast: None,
            southwest: None,
        }
    }

    pub fn manifest_bounds(bounds: TileBounds) -> ManifestTileBounds {
        ManifestTileBounds {
            lat_min: bounds.lat_min,
            lat_max: bounds.lat_max,
            lon_min: bounds.lon_min,
            lon_max: bounds.lon_max,
        }
    }

    fn push_u16(buf: &mut Vec<u8>, value: u16) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(buf: &mut Vec<u8>, value: u32) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u64(buf: &mut Vec<u8>, value: u64) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn push_f32(buf: &mut Vec<u8>, value: f32) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn align_up(value: u64, alignment: usize) -> u64 {
        let alignment = alignment as u64;
        if alignment <= 1 {
            return value;
        }

        let remainder = value % alignment;
        if remainder == 0 {
            value
        } else {
            value + (alignment - remainder)
        }
    }

    fn pad_to_alignment(buf: &mut Vec<u8>, alignment: usize) {
        let target_len = align_up(buf.len() as u64, alignment) as usize;
        buf.resize(target_len, 0);
    }

    fn write_tile_bounds(buf: &mut Vec<u8>, bounds: TileBounds) {
        push_f32(buf, bounds.lat_min);
        push_f32(buf, bounds.lat_max);
        push_f32(buf, bounds.lon_min);
        push_f32(buf, bounds.lon_max);
    }

    fn write_point_record(buf: &mut Vec<u8>, point: PointRecord) {
        push_u64(buf, point.osm_id);
        push_f32(buf, point.lat);
        push_f32(buf, point.lon);
        push_u64(buf, point.lines_offset);
        push_u32(buf, point.lines_count);
        push_u32(buf, point._padding1);
        push_u64(buf, point.rules_offset);
        push_u32(buf, point.rules_count);
        push_u16(buf, point.flags);
        push_u16(buf, point._padding2);
    }

    fn write_line_record(buf: &mut Vec<u8>, line: LineRecord) {
        push_u64(buf, line.point_a_osm_id);
        push_f32(buf, line.point_a_lat);
        push_f32(buf, line.point_a_lon);
        push_u64(buf, line.point_b_osm_id);
        push_f32(buf, line.point_b_lat);
        push_f32(buf, line.point_b_lon);
        buf.push(line.direction);
        buf.push(line._padding1);
        push_u16(buf, line._padding2);
        push_u32(buf, line.tag_set_index);
    }

    fn tag_values_section_size(tag_values: &[String]) -> u64 {
        let entry_bytes = std::mem::size_of::<StringEntry>() * tag_values.len();
        let string_bytes: usize = tag_values.iter().map(|value| value.len()).sum();
        align_up(
            (entry_bytes + string_bytes) as u64,
            std::mem::align_of::<TagSetRecord>(),
        )
    }

    fn write_grid_cell_entry(buf: &mut Vec<u8>, grid_cell: GridCellEntry) {
        push_u32(buf, grid_cell.cell_id);
        push_u32(buf, grid_cell._padding1);
        push_u64(buf, grid_cell.points_offset);
        push_u32(buf, grid_cell.points_count);
        push_u32(buf, grid_cell._padding2);
    }

    fn write_string_entry(buf: &mut Vec<u8>, entry: StringEntry) {
        push_u64(buf, entry.offset);
        push_u32(buf, entry.length);
        push_u32(buf, entry._padding);
    }

    fn write_tag_values_section(buf: &mut Vec<u8>, tag_values: &[String]) {
        let mut string_offset = 0_u64;
        for tag_value in tag_values {
            write_string_entry(
                buf,
                StringEntry {
                    offset: string_offset,
                    length: u32::try_from(tag_value.len()).unwrap(),
                    _padding: 0,
                },
            );
            string_offset += tag_value.len() as u64;
        }

        for tag_value in tag_values {
            buf.extend_from_slice(tag_value.as_bytes());
        }

        pad_to_alignment(buf, std::mem::align_of::<TagSetRecord>());
    }

    fn write_tag_set_record(buf: &mut Vec<u8>, tag_set: TagSetRecord) {
        push_u32(buf, tag_set.name_idx);
        push_u32(buf, tag_set.hw_ref_idx);
        push_u32(buf, tag_set.highway_idx);
        push_u32(buf, tag_set.surface_idx);
        push_u32(buf, tag_set.smoothness_idx);
    }

    fn write_rule_record(buf: &mut Vec<u8>, rule: RuleRecord) {
        push_u64(buf, rule.from_lines_offset);
        push_u32(buf, rule.from_lines_count);
        push_u32(buf, rule._padding1);
        push_u64(buf, rule.to_lines_offset);
        push_u32(buf, rule.to_lines_count);
        buf.push(rule.rule_type);
        buf.push(rule._padding2);
        push_u16(buf, rule._padding3);
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use bytemuck::{pod_read_unaligned, Pod};
        use ridi_router_common::format::RmdfHeader;

        fn read_pod_vec<T: Pod>(bytes: &[u8]) -> Vec<T> {
            bytes
                .chunks_exact(std::mem::size_of::<T>())
                .map(pod_read_unaligned::<T>)
                .collect()
        }

        fn decode_string(entry: StringEntry, pool: &[u8]) -> &str {
            let start = entry.offset as usize;
            let end = start + entry.length as usize;
            std::str::from_utf8(&pool[start..end]).unwrap()
        }

        #[test]
        fn tile_spec_default_keeps_optional_sections_empty() {
            let spec = TileSpec::default();

            assert_eq!(spec.tile_id, SYNTHETIC_TILE_ID);
            assert_eq!(spec.bounds.lat_min, SYNTHETIC_TILE_BOUNDS.lat_min);
            assert_eq!(spec.bounds.lat_max, SYNTHETIC_TILE_BOUNDS.lat_max);
            assert_eq!(spec.bounds.lon_min, SYNTHETIC_TILE_BOUNDS.lon_min);
            assert_eq!(spec.bounds.lon_max, SYNTHETIC_TILE_BOUNDS.lon_max);
            assert!(spec.spatial_index.is_empty());
            assert!(spec.points.is_empty());
            assert!(spec.lines.is_empty());
            assert!(spec.line_refs.is_empty());
            assert!(spec.tag_values.is_empty());
            assert!(spec.tag_sets.is_empty());
            assert!(spec.rules.is_empty());
            assert!(spec.rule_line_refs.is_empty());
        }

        #[test]
        fn write_tile_serializes_spatial_index_and_tag_sections() {
            let dir = unique_test_dir("rmdf-test-support-tag-smoke");
            let lat = 10.5;
            let lon = 20.5;
            let highway_idx = 0;
            let surface_idx = 1;
            let smoothness_idx = 2;

            let spec = TileSpec {
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
                ..TileSpec::default()
            };

            let tile_path = write_tile(&dir, &spec);
            let bytes = fs::read(&tile_path).unwrap();
            let header = pod_read_unaligned::<RmdfHeader>(&bytes[..RmdfHeader::SIZE]);

            let expected_points_offset =
                RmdfHeader::SIZE as u64 + std::mem::size_of::<GridCellEntry>() as u64;
            let expected_lines_offset =
                expected_points_offset + std::mem::size_of::<PointRecord>() as u64;
            let expected_line_refs_offset =
                expected_lines_offset + std::mem::size_of::<LineRecord>() as u64;
            let expected_tag_values_offset =
                expected_line_refs_offset + std::mem::size_of::<u64>() as u64;
            let expected_tag_sets_offset =
                expected_tag_values_offset + tag_values_section_size(&spec.tag_values);
            let expected_rules_offset =
                expected_tag_sets_offset + std::mem::size_of::<TagSetRecord>() as u64;

            assert_eq!(header.spatial_grid_cell_count, 1);
            assert_eq!(header.tag_value_count, 3);
            assert_eq!(header.tag_set_count, 1);
            assert_eq!(header.section_offsets[0], RmdfHeader::SIZE as u64);
            assert_eq!(header.section_offsets[1], expected_points_offset);
            assert_eq!(header.section_offsets[2], expected_lines_offset);
            assert_eq!(header.section_offsets[3], expected_line_refs_offset);
            assert_eq!(header.section_offsets[4], expected_tag_values_offset);
            assert_eq!(header.section_offsets[5], expected_tag_sets_offset);
            assert_eq!(header.section_offsets[6], expected_rules_offset);

            let tag_values_start = header.section_offsets[4] as usize;
            let tag_sets_start = header.section_offsets[5] as usize;
            let rules_start = header.section_offsets[6] as usize;
            let entry_bytes = std::mem::size_of::<StringEntry>() * spec.tag_values.len();
            let tag_values_section = &bytes[tag_values_start..tag_sets_start];
            let string_entries = read_pod_vec::<StringEntry>(&tag_values_section[..entry_bytes]);
            let string_pool = &tag_values_section[entry_bytes..];

            assert_eq!(decode_string(string_entries[0], string_pool), "secondary");
            assert_eq!(decode_string(string_entries[1], string_pool), "gravel");
            assert_eq!(decode_string(string_entries[2], string_pool), "bad");

            let tag_sets = read_pod_vec::<TagSetRecord>(&bytes[tag_sets_start..rules_start]);
            assert_eq!(tag_sets.len(), 1);
            assert_eq!(tag_sets[0].highway_idx, highway_idx);
            assert_eq!(tag_sets[0].surface_idx, surface_idx);
            assert_eq!(tag_sets[0].smoothness_idx, smoothness_idx);

            fs::remove_dir_all(dir).unwrap();
        }
    }
}
