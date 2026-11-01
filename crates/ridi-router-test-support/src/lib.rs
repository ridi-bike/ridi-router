pub mod rmdf {
    use std::{
        fs,
        path::{Path, PathBuf},
        process,
        time::{SystemTime, UNIX_EPOCH},
    };

    use std::collections::HashMap;

    use bytemuck::{bytes_of, cast_slice};
    use ridi_router_common::format as rmdf;
    use ridi_router_common::format::{RmdfHeader, StringEntry, NUM_SECTIONS};
    pub use ridi_router_common::format::{TagSetRecord, TileBounds, TileId};
    pub use ridi_router_common::manifest::{
        TileBounds as ManifestTileBounds, TileManifest, TileMetadata, TileNeighbors,
    };

    // Fixtures describe tiles in the v1 record shapes (absolute offsets, endpoint ids and
    // coordinates on each line, rules addressed from their via point); `write_tile` encodes them
    // in the current format.

    #[derive(Debug, Clone, Copy)]
    pub struct GridCellEntry {
        pub cell_id: u32,
        pub _padding1: u32,
        pub points_offset: u64,
        pub points_count: u32,
        pub _padding2: u32,
    }

    impl GridCellEntry {
        pub fn encode_cell_id(lat: f32, lon: f32, precision: u32) -> u32 {
            rmdf::GridCellEntry::encode_cell_id(lat, lon, precision)
        }
    }

    #[derive(Debug, Clone, Copy)]
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
        pub const RESIDENTIAL_IN_PROXIMITY_FLAG: u16 =
            rmdf::PointRecord::RESIDENTIAL_IN_PROXIMITY_FLAG;
        pub const NOGO_AREA_FLAG: u16 = rmdf::PointRecord::NOGO_AREA_FLAG;
    }

    #[derive(Debug, Clone, Copy)]
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

    #[derive(Debug, Clone, Copy)]
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
    pub struct RoundTripSingleTileFixture {
        pub dir: PathBuf,
        pub rule_file: PathBuf,
        pub tile_id: TileId,
        pub start_finish_osm_id: u64,
        pub start_finish_lat: f32,
        pub start_finish_lon: f32,
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
        let tile_path = dir.join(spec.tile_id.to_filename());
        fs::write(&tile_path, encode_tile(spec)).unwrap();
        tile_path
    }

    /// Encode `spec` as an RMDF file. A line endpoint that is not one of the spec's points (a
    /// fixture shortcut the generator never produces) is added as a point without lines.
    pub fn encode_tile(spec: &TileSpec) -> Vec<u8> {
        let mut points: Vec<rmdf::PointRecord> = spec
            .points
            .iter()
            .map(|p| rmdf::PointRecord {
                osm_id: p.osm_id,
                lat: p.lat,
                lon: p.lon,
                lines_offset: u32::try_from(p.lines_offset).unwrap(),
                lines_count: u16::try_from(p.lines_count).unwrap(),
                flags: if p.rules_count > 0 {
                    p.flags | rmdf::PointRecord::HAS_RULES_FLAG
                } else {
                    p.flags
                },
            })
            .collect();
        let mut point_index = HashMap::<u64, u32>::new();
        for (i, p) in points.iter().enumerate() {
            point_index.entry(p.osm_id).or_insert(i as u32);
        }
        let mut endpoint = |osm_id: u64, lat: f32, lon: f32| -> u32 {
            *point_index.entry(osm_id).or_insert_with(|| {
                points.push(rmdf::PointRecord {
                    osm_id,
                    lat,
                    lon,
                    lines_offset: 0,
                    lines_count: 0,
                    flags: 0,
                });
                (points.len() - 1) as u32
            })
        };
        let lines: Vec<rmdf::LineRecord> = spec
            .lines
            .iter()
            .map(|l| {
                rmdf::LineRecord::new(
                    endpoint(l.point_a_osm_id, l.point_a_lat, l.point_a_lon),
                    endpoint(l.point_b_osm_id, l.point_b_lat, l.point_b_lon),
                    l.tag_set_index,
                    l.direction,
                )
            })
            .collect();

        let mut rules: Vec<rmdf::RuleRecord> = spec
            .points
            .iter()
            .flat_map(|p| {
                let start = p.rules_offset as usize;
                spec.rules[start..start + p.rules_count as usize]
                    .iter()
                    .map(move |r| rmdf::RuleRecord {
                        via_osm_id: p.osm_id,
                        from_lines_offset: u32::try_from(r.from_lines_offset).unwrap(),
                        to_lines_offset: u32::try_from(r.to_lines_offset).unwrap(),
                        from_lines_count: u16::try_from(r.from_lines_count).unwrap(),
                        to_lines_count: u16::try_from(r.to_lines_count).unwrap(),
                        rule_type: r.rule_type,
                        _padding: [0; 3],
                    })
            })
            .collect();
        rules.sort_by_key(|r| r.via_osm_id);

        let spatial_index: Vec<rmdf::GridCellEntry> = spec
            .spatial_index
            .iter()
            .map(|g| rmdf::GridCellEntry {
                cell_id: g.cell_id,
                points_offset: u32::try_from(g.points_offset).unwrap(),
                points_count: g.points_count,
            })
            .collect();
        let refs = |refs: &[u64]| -> Vec<u32> {
            refs.iter().map(|&r| u32::try_from(r).unwrap()).collect()
        };
        let line_refs = refs(&spec.line_refs);
        let rule_line_refs = refs(&spec.rule_line_refs);

        let mut string_entries = Vec::with_capacity(spec.tag_values.len());
        let mut string_pool = Vec::new();
        for value in &spec.tag_values {
            string_entries.push(StringEntry {
                offset: string_pool.len() as u32,
                length: value.len() as u32,
            });
            string_pool.extend_from_slice(value.as_bytes());
        }

        let sections: [Vec<&[u8]>; NUM_SECTIONS] = [
            vec![cast_slice(&spatial_index)],
            vec![cast_slice(&points)],
            vec![cast_slice(&lines)],
            vec![cast_slice(&line_refs)],
            vec![cast_slice(&string_entries), &string_pool],
            vec![cast_slice(&spec.tag_sets)],
            vec![cast_slice(&rules), cast_slice(&rule_line_refs)],
        ];
        let mut section_offsets = [0u64; NUM_SECTIONS];
        let mut body = Vec::new();
        for (i, parts) in sections.iter().enumerate() {
            body.resize(
                rmdf::align_section(RmdfHeader::SIZE + body.len()) - RmdfHeader::SIZE,
                0,
            );
            section_offsets[i] = (RmdfHeader::SIZE + body.len()) as u64;
            for part in parts {
                body.extend_from_slice(part);
            }
        }

        let header = RmdfHeader {
            magic: RmdfHeader::MAGIC,
            version: RmdfHeader::VERSION,
            tile_bounds: spec.bounds,
            point_count: points.len() as u64,
            line_count: lines.len() as u64,
            spatial_grid_cell_count: spatial_index.len() as u32,
            tag_value_count: spec.tag_values.len() as u32,
            tag_set_count: spec.tag_sets.len() as u32,
            rule_count: rules.len() as u32,
            section_offsets,
        };
        let mut bytes = bytes_of(&header).to_vec();
        bytes.extend_from_slice(&body);
        let checksum = rmdf::checksum(&bytes);
        bytes.extend_from_slice(&checksum);
        bytes
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

    fn write_round_trip_rules_file(dir: &Path) -> PathBuf {
        fs::create_dir_all(dir).unwrap();

        let rules = serde_json::json!({
            "basic": {
                "step_limit": 100,
                "progression_direction": {
                    "enabled": false,
                    "check_junctions_back": 100
                },
                "progression_speed": {
                    "enabled": false,
                    "check_steps_back": 1000,
                    "last_step_distance_below_avg_with_ratio": 1.3
                },
                "no_short_detours": {
                    "enabled": false,
                    "min_detour_len_m": 5000.0
                }
            },
            "generation": {
                "waypoint_generation": {
                    "start_finish": {
                        "variation_distances_m": [],
                        "variation_bearing_deg": []
                    },
                    "round_trip": {
                        "variation_distance_ratios": [1.0],
                        "variation_bearing_deg": [0.0]
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

        let rule_file = dir.join("round-trip-rules.json");
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

    pub fn create_round_trip_single_tile_fixture(prefix: &str) -> RoundTripSingleTileFixture {
        let dir = unique_test_dir(prefix);
        fs::create_dir_all(&dir).unwrap();

        let start_finish_osm_id = 2000;
        let north_west_osm_id = 2001;
        let north_east_osm_id = 2002;
        let south_east_osm_id = 2003;

        let points = vec![
            PointRecord {
                osm_id: start_finish_osm_id,
                lat: 10.0,
                lon: 20.0,
                lines_offset: 0,
                lines_count: 3,
                _padding1: 0,
                rules_offset: 0,
                rules_count: 0,
                flags: 0,
                _padding2: 0,
            },
            PointRecord {
                osm_id: north_west_osm_id,
                lat: 10.01,
                lon: 20.0,
                lines_offset: 3,
                lines_count: 3,
                _padding1: 0,
                rules_offset: 0,
                rules_count: 0,
                flags: 0,
                _padding2: 0,
            },
            PointRecord {
                osm_id: north_east_osm_id,
                lat: 10.01,
                lon: 20.01,
                lines_offset: 6,
                lines_count: 3,
                _padding1: 0,
                rules_offset: 0,
                rules_count: 0,
                flags: 0,
                _padding2: 0,
            },
            PointRecord {
                osm_id: south_east_osm_id,
                lat: 10.0,
                lon: 20.01,
                lines_offset: 9,
                lines_count: 3,
                _padding1: 0,
                rules_offset: 0,
                rules_count: 0,
                flags: 0,
                _padding2: 0,
            },
        ];

        let lines = vec![
            LineRecord {
                point_a_osm_id: start_finish_osm_id,
                point_a_lat: 10.0,
                point_a_lon: 20.0,
                point_b_osm_id: north_west_osm_id,
                point_b_lat: 10.01,
                point_b_lon: 20.0,
                direction: 0,
                _padding1: 0,
                _padding2: 0,
                tag_set_index: 0,
            },
            LineRecord {
                point_a_osm_id: north_west_osm_id,
                point_a_lat: 10.01,
                point_a_lon: 20.0,
                point_b_osm_id: north_east_osm_id,
                point_b_lat: 10.01,
                point_b_lon: 20.01,
                direction: 0,
                _padding1: 0,
                _padding2: 0,
                tag_set_index: 0,
            },
            LineRecord {
                point_a_osm_id: north_east_osm_id,
                point_a_lat: 10.01,
                point_a_lon: 20.01,
                point_b_osm_id: south_east_osm_id,
                point_b_lat: 10.0,
                point_b_lon: 20.01,
                direction: 0,
                _padding1: 0,
                _padding2: 0,
                tag_set_index: 0,
            },
            LineRecord {
                point_a_osm_id: south_east_osm_id,
                point_a_lat: 10.0,
                point_a_lon: 20.01,
                point_b_osm_id: start_finish_osm_id,
                point_b_lat: 10.0,
                point_b_lon: 20.0,
                direction: 0,
                _padding1: 0,
                _padding2: 0,
                tag_set_index: 0,
            },
            LineRecord {
                point_a_osm_id: start_finish_osm_id,
                point_a_lat: 10.0,
                point_a_lon: 20.0,
                point_b_osm_id: north_east_osm_id,
                point_b_lat: 10.01,
                point_b_lon: 20.01,
                direction: 0,
                _padding1: 0,
                _padding2: 0,
                tag_set_index: 0,
            },
            LineRecord {
                point_a_osm_id: north_west_osm_id,
                point_a_lat: 10.01,
                point_a_lon: 20.0,
                point_b_osm_id: south_east_osm_id,
                point_b_lat: 10.0,
                point_b_lon: 20.01,
                direction: 0,
                _padding1: 0,
                _padding2: 0,
                tag_set_index: 0,
            },
        ];

        let line_refs = vec![0, 3, 4, 0, 1, 5, 1, 2, 4, 2, 3, 5];
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
                    checksum: "sha256:test-round-trip".to_string(),
                    military_geojson_filename: None,
                }],
            },
        );

        let rule_file = write_round_trip_rules_file(&dir);

        RoundTripSingleTileFixture {
            dir,
            rule_file,
            tile_id: SYNTHETIC_TILE_ID,
            start_finish_osm_id,
            start_finish_lat: 10.0,
            start_finish_lon: 20.0,
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

            // the line's far endpoint is added as a second point
            let expected_points_offset = RmdfHeader::SIZE as u64 + 16;
            let expected_lines_offset = expected_points_offset + 2 * 24;
            let expected_line_refs_offset = expected_lines_offset + 16;
            let expected_tag_values_offset = expected_line_refs_offset + 8;
            let expected_tag_sets_offset = expected_tag_values_offset
                + (3 * 8 + "secondarygravelbad".len() as u64).next_multiple_of(8);
            let expected_rules_offset = expected_tag_sets_offset + 24;

            assert_eq!(header.version, RmdfHeader::VERSION);
            assert_eq!(header.point_count, 2);
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
            assert_eq!(bytes.len(), expected_rules_offset as usize + 16);

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
