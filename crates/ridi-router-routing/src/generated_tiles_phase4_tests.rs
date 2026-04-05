use std::{
    fs,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use osm_io::osm::{
    model::{
        coordinate::Coordinate,
        element::Element,
        node::Node,
        relation::{Member, MemberData, Relation},
        tag::Tag,
        way::Way,
    },
    pbf::{compression_type::CompressionType, file_info::FileInfo, writer::Writer},
};
use ridi_router_tiles::{generate_tiles, TileGenerationRequest, TileInputSource};

use crate::{
    map_data::{
        graph::{MapDataGraph, MapDataLineRef, MapDataPointRef},
        rule::MapDataRuleType,
    },
    router::walker::{Walker, WalkerError, WalkerMoveResult},
    RoutingContext,
};

const TILE_SIZE_DEGREES: f32 = 1.0;
const START_OSM_ID: u64 = 1;
const VIA_OSM_ID: u64 = 2;
const FORBIDDEN_EXIT_OSM_ID: u64 = 3;
const ALLOWED_EXIT_OSM_ID: u64 = 4;
const EXTRA_ALLOWED_EXIT_OSM_ID: u64 = 5;
const FINISH_OSM_ID: u64 = 6;
const VIA_LAT: f32 = 10.11;
const VIA_LON: f32 = 20.10;

#[test]
fn generated_tiles_hydrate_turn_rules_and_change_walker_behavior() {
    let fixture = GeneratedRestrictionFixture::new("routing-phase4-generated-restriction");

    let tile_manager = crate::rmdf::TileManager::new(fixture.tiles_dir.clone()).unwrap();
    let graph = MapDataGraph::new(tile_manager);

    let via_point = graph.get_point_from_tiles(fixture.tile_id, VIA_OSM_ID);
    assert_eq!(via_point.id, VIA_OSM_ID);
    assert_eq!(via_point.lines.len(), 4);
    assert_eq!(via_point.rules.len(), 1);

    let rule = &via_point.rules[0];
    assert_eq!(rule.rule_type, MapDataRuleType::NotAllowed);
    assert_eq!(
        line_endpoint_ids(&graph, &rule.from_lines),
        vec![(START_OSM_ID, VIA_OSM_ID)]
    );
    assert_eq!(
        line_endpoint_ids(&graph, &rule.to_lines),
        vec![(VIA_OSM_ID, FORBIDDEN_EXIT_OSM_ID)]
    );

    let ctx = RoutingContext::new(&graph);
    let start = MapDataPointRef::new(fixture.tile_id, START_OSM_ID);
    let finish = MapDataPointRef::new(fixture.tile_id, FINISH_OSM_ID);
    let forbidden_exit = MapDataPointRef::new(fixture.tile_id, FORBIDDEN_EXIT_OSM_ID);
    let allowed_exit = MapDataPointRef::new(fixture.tile_id, ALLOWED_EXIT_OSM_ID);
    let extra_allowed_exit = MapDataPointRef::new(fixture.tile_id, EXTRA_ALLOWED_EXIT_OSM_ID);

    let mut walker = Walker::new(start);
    let choices = match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
        Ok(WalkerMoveResult::Fork(choices)) => choices,
        other => panic!("expected a fork at the via point, got {other:?}"),
    };

    let choice_points = choices.get_all_segment_points();
    assert!(!choice_points.contains(&forbidden_exit));
    assert!(choice_points.contains(&allowed_exit));
    assert!(choice_points.contains(&extra_allowed_exit));

    walker.set_fork_choice_point_ref(forbidden_exit);
    match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
        Err(WalkerError::WrongForkChoice {
            id,
            available_fork_ids,
        }) => {
            assert_eq!(id, FORBIDDEN_EXIT_OSM_ID);
            assert!(!available_fork_ids.contains(&FORBIDDEN_EXIT_OSM_ID));
            assert!(available_fork_ids.contains(&ALLOWED_EXIT_OSM_ID));
            assert!(available_fork_ids.contains(&EXTRA_ALLOWED_EXIT_OSM_ID));
        }
        other => panic!("expected forbidden move to be rejected, got {other:?}"),
    }

    walker.set_fork_choice_point_ref(allowed_exit);
    assert_eq!(
        walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish),
        Ok(WalkerMoveResult::Finish)
    );
}

fn line_endpoint_ids(graph: &MapDataGraph, line_refs: &[MapDataLineRef]) -> Vec<(u64, u64)> {
    let mut endpoints = line_refs
        .iter()
        .map(|line_ref| {
            let line = graph
                .get_line_from_tiles(line_ref.get_tile_id(), line_ref.get_element_id() as usize);
            (
                line.points.0.get_element_id(),
                line.points.1.get_element_id(),
            )
        })
        .collect::<Vec<_>>();
    endpoints.sort_unstable();
    endpoints
}

struct GeneratedRestrictionFixture {
    _dir: TempDir,
    tiles_dir: PathBuf,
    tile_id: crate::rmdf::TileId,
}

impl GeneratedRestrictionFixture {
    fn new(prefix: &str) -> Self {
        let dir = TempDir::new(prefix);
        let input_path = dir.path().join("fixture.osm.pbf");
        let tiles_dir = dir.path().join("tiles");
        fs::create_dir_all(&tiles_dir).unwrap();

        write_restriction_fixture_pbf(&input_path);

        let summary = generate_tiles(TileGenerationRequest {
            input: TileInputSource::File(input_path),
            output_dir: tiles_dir.clone(),
            tile_size_deg: TILE_SIZE_DEGREES,
            db_path: None,
        })
        .unwrap();

        assert_eq!(summary.tile_count, 1);

        Self {
            _dir: dir,
            tiles_dir,
            tile_id: crate::rmdf::TileId::from_coords(VIA_LAT, VIA_LON, TILE_SIZE_DEGREES),
        }
    }
}

fn write_restriction_fixture_pbf(path: &Path) {
    let mut file_info = FileInfo::default();
    file_info.with_writingprogram_str("ridi-router-phase4-test");

    let mut writer =
        Writer::from_file_info(path.to_path_buf(), file_info, CompressionType::Zlib).unwrap();
    writer.write_header().unwrap();

    for node in [
        node(START_OSM_ID as i64, 10.10, 20.10),
        node(VIA_OSM_ID as i64, VIA_LAT as f64, VIA_LON as f64),
        node(FORBIDDEN_EXIT_OSM_ID as i64, 10.11, 20.11),
        node(ALLOWED_EXIT_OSM_ID as i64, 10.11, 20.09),
        node(EXTRA_ALLOWED_EXIT_OSM_ID as i64, 10.12, 20.10),
        node(FINISH_OSM_ID as i64, 10.11, 20.08),
    ] {
        writer.write_element(Element::Node { node }).unwrap();
    }

    for way in [
        way(10, &[START_OSM_ID as i64, VIA_OSM_ID as i64]),
        way(20, &[VIA_OSM_ID as i64, FORBIDDEN_EXIT_OSM_ID as i64]),
        way(
            30,
            &[
                VIA_OSM_ID as i64,
                ALLOWED_EXIT_OSM_ID as i64,
                FINISH_OSM_ID as i64,
            ],
        ),
        way(40, &[VIA_OSM_ID as i64, EXTRA_ALLOWED_EXIT_OSM_ID as i64]),
    ] {
        writer.write_element(Element::Way { way }).unwrap();
    }

    writer
        .write_element(Element::Relation {
            relation: Relation::new(
                100,
                1,
                0,
                0,
                0,
                String::new(),
                true,
                vec![
                    Member::Way {
                        member: MemberData::new(10, "from".to_string()),
                    },
                    Member::Node {
                        member: MemberData::new(VIA_OSM_ID as i64, "via".to_string()),
                    },
                    Member::Way {
                        member: MemberData::new(20, "to".to_string()),
                    },
                ],
                vec![
                    Tag::new("type", "restriction"),
                    Tag::new("restriction", "no_right_turn"),
                ],
            ),
        })
        .unwrap();

    writer.close().unwrap();
}

fn node(id: i64, lat: f64, lon: f64) -> Node {
    Node::new(
        id,
        1,
        Coordinate::new(lat, lon),
        0,
        0,
        0,
        String::new(),
        true,
        Vec::new(),
    )
}

fn way(id: i64, refs: &[i64]) -> Way {
    Way::new(
        id,
        1,
        0,
        0,
        0,
        String::new(),
        true,
        refs.to_vec(),
        vec![Tag::new("highway", "primary")],
    )
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "ridi-router-{prefix}-{}-{}",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
