use std::{
    fs, io,
    path::{Path, PathBuf},
    process,
    sync::{Arc, Mutex, OnceLock},
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
use ridi_router_tiles::{
    generate_tiles, TileGenerationRequest, TileGenerationSummary, TileInputSource,
};
use tracing_subscriber::{
    fmt::{self, MakeWriter},
    prelude::*,
};

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
const BORDER_DUPLICATED_VIA_LON: f32 = 20.999;
const BORDER_DUPLICATED_EAST_TILE_LON: f32 = 21.001;

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

#[test]
fn generated_tiles_hydrate_rules_for_buffer_duplicated_via_point_in_both_tiles() {
    let dir = TempDir::new("routing-phase4-border-duplicated-restriction");
    let input_path = dir.path().join("fixture.osm.pbf");
    let tiles_dir = dir.path().join("tiles");
    fs::create_dir_all(&tiles_dir).unwrap();

    write_border_duplicated_restriction_fixture_pbf(&input_path);

    let summary = generate_fixture_tiles(input_path, tiles_dir.clone());
    assert_eq!(summary.tile_count, 2);

    let tile_manager = crate::rmdf::TileManager::new(tiles_dir).unwrap();
    let graph = MapDataGraph::new(tile_manager);
    let west_tile_id =
        crate::rmdf::TileId::from_coords(VIA_LAT, BORDER_DUPLICATED_VIA_LON, TILE_SIZE_DEGREES);
    let east_tile_id = crate::rmdf::TileId::from_coords(
        VIA_LAT,
        BORDER_DUPLICATED_EAST_TILE_LON,
        TILE_SIZE_DEGREES,
    );

    for tile_id in [west_tile_id, east_tile_id] {
        let via_point = graph.get_point_from_tiles(tile_id, VIA_OSM_ID);
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
    }
}

#[test]
fn generated_tiles_warn_and_summarize_skipped_unsupported_relations() {
    let dir = TempDir::new("routing-phase4-unsupported-restriction");
    let input_path = dir.path().join("fixture.osm.pbf");
    let tiles_dir = dir.path().join("tiles");
    fs::create_dir_all(&tiles_dir).unwrap();

    write_unsupported_restriction_fixture_pbf(&input_path);

    let log_buffer = init_test_log_capture();
    let summary = generate_fixture_tiles(input_path, tiles_dir);
    assert_eq!(summary.tile_count, 1);

    let output = log_buffer.as_string();
    assert!(output.contains("Skipping restriction relation 100 [via_way]"));
    assert!(output.contains("via-way restrictions are not supported"));
    assert!(output.contains("Skipped restriction relations during tile generation: via_way=1"));
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

        let summary = generate_fixture_tiles(input_path, tiles_dir.clone());
        assert_eq!(summary.tile_count, 1);

        Self {
            _dir: dir,
            tiles_dir,
            tile_id: crate::rmdf::TileId::from_coords(VIA_LAT, VIA_LON, TILE_SIZE_DEGREES),
        }
    }
}

fn generate_fixture_tiles(input_path: PathBuf, output_dir: PathBuf) -> TileGenerationSummary {
    generate_tiles(TileGenerationRequest {
        input: TileInputSource::File(input_path),
        output_dir,
        tile_size_deg: TILE_SIZE_DEGREES,
        db_path: None,
    })
    .unwrap()
}

fn write_restriction_fixture_pbf(path: &Path) {
    write_pbf_fixture(
        path,
        vec![
            node(START_OSM_ID as i64, 10.10, 20.10),
            node(VIA_OSM_ID as i64, VIA_LAT as f64, VIA_LON as f64),
            node(FORBIDDEN_EXIT_OSM_ID as i64, 10.11, 20.11),
            node(ALLOWED_EXIT_OSM_ID as i64, 10.11, 20.09),
            node(EXTRA_ALLOWED_EXIT_OSM_ID as i64, 10.12, 20.10),
            node(FINISH_OSM_ID as i64, 10.11, 20.08),
        ],
        vec![
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
        ],
        vec![restriction_relation(
            100,
            "no_right_turn",
            vec![
                way_member(10, "from"),
                node_member(VIA_OSM_ID as i64, "via"),
                way_member(20, "to"),
            ],
        )],
    );
}

fn write_border_duplicated_restriction_fixture_pbf(path: &Path) {
    write_pbf_fixture(
        path,
        vec![
            node(START_OSM_ID as i64, 10.10, 20.998),
            node(
                VIA_OSM_ID as i64,
                VIA_LAT as f64,
                BORDER_DUPLICATED_VIA_LON as f64,
            ),
            node(FORBIDDEN_EXIT_OSM_ID as i64, 10.11, 21.002),
            node(ALLOWED_EXIT_OSM_ID as i64, 10.11, 20.997),
            node(EXTRA_ALLOWED_EXIT_OSM_ID as i64, 10.12, 20.999),
            node(FINISH_OSM_ID as i64, 10.11, 20.996),
        ],
        vec![
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
        ],
        vec![restriction_relation(
            100,
            "no_right_turn",
            vec![
                way_member(10, "from"),
                node_member(VIA_OSM_ID as i64, "via"),
                way_member(20, "to"),
            ],
        )],
    );
}

fn write_unsupported_restriction_fixture_pbf(path: &Path) {
    write_pbf_fixture(
        path,
        vec![
            node(START_OSM_ID as i64, 10.10, 20.10),
            node(VIA_OSM_ID as i64, VIA_LAT as f64, VIA_LON as f64),
            node(FORBIDDEN_EXIT_OSM_ID as i64, 10.11, 20.11),
            node(ALLOWED_EXIT_OSM_ID as i64, 10.11, 20.09),
        ],
        vec![
            way(10, &[START_OSM_ID as i64, VIA_OSM_ID as i64]),
            way(20, &[VIA_OSM_ID as i64, FORBIDDEN_EXIT_OSM_ID as i64]),
            way(30, &[VIA_OSM_ID as i64, ALLOWED_EXIT_OSM_ID as i64]),
        ],
        vec![restriction_relation(
            100,
            "no_right_turn",
            vec![
                way_member(10, "from"),
                way_member(20, "via"),
                way_member(30, "to"),
            ],
        )],
    );
}

fn write_pbf_fixture(path: &Path, nodes: Vec<Node>, ways: Vec<Way>, relations: Vec<Relation>) {
    let mut file_info = FileInfo::default();
    file_info.with_writingprogram_str("ridi-router-phase4-test");

    let mut writer =
        Writer::from_file_info(path.to_path_buf(), file_info, CompressionType::Zlib).unwrap();
    writer.write_header().unwrap();

    for node in nodes {
        writer.write_element(Element::Node { node }).unwrap();
    }

    for way in ways {
        writer.write_element(Element::Way { way }).unwrap();
    }

    for relation in relations {
        writer
            .write_element(Element::Relation { relation })
            .unwrap();
    }

    writer.close().unwrap();
}

fn restriction_relation(id: i64, restriction: &str, members: Vec<Member>) -> Relation {
    Relation::new(
        id,
        1,
        0,
        0,
        0,
        String::new(),
        true,
        members,
        vec![
            Tag::new("type", "restriction"),
            Tag::new("restriction", restriction),
        ],
    )
}

fn way_member(id: i64, role: &str) -> Member {
    Member::Way {
        member: MemberData::new(id, role.to_string()),
    }
}

fn node_member(id: i64, role: &str) -> Member {
    Member::Node {
        member: MemberData::new(id, role.to_string()),
    }
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

#[derive(Clone, Default)]
struct SharedLogBuffer {
    inner: Arc<Mutex<Vec<u8>>>,
}

impl SharedLogBuffer {
    fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }

    fn as_string(&self) -> String {
        String::from_utf8(self.inner.lock().unwrap().clone()).unwrap()
    }
}

impl<'a> MakeWriter<'a> for SharedLogBuffer {
    type Writer = SharedLogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        SharedLogWriter {
            inner: self.inner.clone(),
        }
    }
}

struct SharedLogWriter {
    inner: Arc<Mutex<Vec<u8>>>,
}

impl io::Write for SharedLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn init_test_log_capture() -> SharedLogBuffer {
    static TEST_LOG_BUFFER: OnceLock<SharedLogBuffer> = OnceLock::new();
    static TEST_LOG_SUBSCRIBER: OnceLock<()> = OnceLock::new();

    let log_buffer = TEST_LOG_BUFFER
        .get_or_init(SharedLogBuffer::default)
        .clone();
    TEST_LOG_SUBSCRIBER.get_or_init(|| {
        let subscriber = tracing_subscriber::registry().with(
            fmt::layer()
                .without_time()
                .with_ansi(false)
                .with_target(false)
                .with_writer(log_buffer.clone())
                .with_filter(tracing_subscriber::filter::LevelFilter::WARN),
        );
        tracing::subscriber::set_global_default(subscriber)
            .expect("test log subscriber should only be installed once");
    });
    log_buffer.clear();
    log_buffer
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
