use std::{collections::HashMap, sync::Arc};

use crate::{
    map_data::{
        graph::{ElementTagSetRef, MapDataGraph, MapDataLineRef, MapDataPointRef},
        line::{LineDirection, MapDataLine},
        point::MapDataPoint,
        rule::{MapDataRule, MapDataRuleType},
    },
    router::route::Route,
    RoutingContext,
};
use ridi_router_common::osm::{
    OsmNode, OsmRelation, OsmRelationMemberRole, OsmRelationMemberType, OsmWay,
};
pub type OsmTestData = (Vec<OsmNode>, Vec<OsmWay>, Vec<OsmRelation>);

pub struct RoutingTestContext {
    pub graph: Arc<MapDataGraph>,
}

impl RoutingTestContext {
    pub fn new(test_data: OsmTestData) -> Self {
        Self {
            graph: Arc::new(graph_from_test_dataset(test_data)),
        }
    }

    pub fn resolver(&self) -> RoutingContext<'_> {
        RoutingContext::new(self.graph.as_ref())
    }

    pub fn point(&self, id: u64) -> MapDataPointRef {
        self.graph
            .test_get_point_ref_by_id(&id)
            .unwrap_or_else(|| panic!("missing test point id {id}"))
    }
}

fn make_osm_point_with_id(id: u64) -> OsmNode {
    OsmNode {
        lat: id as f64,
        lon: id as f64,
        id,
        residential_in_proximity: false,
        nogo_area: false,
    }
}

pub fn test_dataset_2() -> OsmTestData {
    // 1 - - 2 - - 3 - - 4 - - 5
    //       |     |
    //       /\    \/
    //       |     |
    // 6 - - 7 -<- 8 - - 9 - - 10
    //      /r\
    //     /r r\
    //111-11     13-131
    //    \r  r/
    //     \rr/
    //      12
    //      |
    //      121

    let tags_with_highway = HashMap::from([("highway".to_string(), "primary".to_string())]);

    (
        vec![
            make_osm_point_with_id(1),
            make_osm_point_with_id(2),
            make_osm_point_with_id(3),
            make_osm_point_with_id(4),
            make_osm_point_with_id(5),
            make_osm_point_with_id(6),
            make_osm_point_with_id(7),
            make_osm_point_with_id(8),
            make_osm_point_with_id(9),
            make_osm_point_with_id(10),
            make_osm_point_with_id(11),
            make_osm_point_with_id(12),
            make_osm_point_with_id(13),
            make_osm_point_with_id(111),
            make_osm_point_with_id(121),
            make_osm_point_with_id(131),
        ],
        vec![
            OsmWay {
                id: 12345,
                point_ids: vec![1, 2, 3, 4, 5],
                tags: Some(tags_with_highway.clone()),
            },
            OsmWay {
                id: 67,
                point_ids: vec![6, 7],
                tags: Some(HashMap::from([
                    ("oneway".to_string(), "yes".to_string()),
                    ("highway".to_string(), "primary".to_string()),
                ])),
            },
            OsmWay {
                id: 87,
                point_ids: vec![8, 7],
                tags: Some(HashMap::from([
                    ("oneway".to_string(), "yes".to_string()),
                    ("highway".to_string(), "primary".to_string()),
                ])),
            },
            OsmWay {
                id: 8910,
                point_ids: vec![8, 9, 10],
                tags: Some(tags_with_highway.clone()),
            },
            OsmWay {
                id: 72,
                point_ids: vec![7, 2],
                tags: Some(HashMap::from([
                    ("oneway".to_string(), "yes".to_string()),
                    ("highway".to_string(), "primary".to_string()),
                ])),
            },
            OsmWay {
                id: 38,
                point_ids: vec![3, 8],
                tags: Some(HashMap::from([
                    ("oneway".to_string(), "yes".to_string()),
                    ("highway".to_string(), "primary".to_string()),
                ])),
            },
            OsmWay {
                id: 7111213,
                point_ids: vec![7, 11, 12, 13],
                tags: Some(HashMap::from([
                    ("junction".to_string(), "roundabout".to_string()),
                    ("highway".to_string(), "primary".to_string()),
                ])),
            },
            OsmWay {
                id: 11111,
                point_ids: vec![111, 11],
                tags: Some(tags_with_highway.clone()),
            },
            OsmWay {
                id: 12121,
                point_ids: vec![121, 12],
                tags: Some(tags_with_highway.clone()),
            },
            OsmWay {
                id: 13131,
                point_ids: vec![131, 13],
                tags: Some(tags_with_highway.clone()),
            },
        ],
        Vec::new(),
    )
}

pub fn test_dataset_1() -> OsmTestData {
    //       1
    //       |
    //       |
    //       2
    //       |
    //       |
    // 5 - - 3 - - 6 - - 7
    //       |     |
    //       |     |
    //       4 - - 8 - - 9
    //
    //       11 - 12
    //

    let tags_with_highway = HashMap::from([("highway".to_string(), "primary".to_string())]);

    (
        vec![
            OsmNode {
                id: 1,
                lat: 1.0,
                lon: 1.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 2,
                lat: 2.0,
                lon: 2.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 3,
                lat: 3.0,
                lon: 3.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 4,
                lat: 4.0,
                lon: 4.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 5,
                lat: 5.0,
                lon: 5.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 6,
                lat: 6.0,
                lon: 6.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 7,
                lat: 7.0,
                lon: 7.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 8,
                lat: 8.0,
                lon: 8.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 9,
                lat: 9.0,
                lon: 9.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 11,
                lat: 11.0,
                lon: 11.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 12,
                lat: 12.0,
                lon: 12.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
        ],
        vec![
            OsmWay {
                id: 1234,
                point_ids: vec![1, 2, 3, 4],
                tags: Some(tags_with_highway.clone()),
            },
            OsmWay {
                id: 5367,
                point_ids: vec![5, 3, 6, 7],
                tags: Some(tags_with_highway.clone()),
            },
            OsmWay {
                id: 489,
                point_ids: vec![4, 8, 9],
                tags: Some(tags_with_highway.clone()),
            },
            OsmWay {
                id: 68,
                point_ids: vec![6, 8],
                tags: Some(tags_with_highway.clone()),
            },
            OsmWay {
                id: 1112,
                point_ids: vec![11, 12],
                tags: Some(tags_with_highway.clone()),
            },
        ],
        Vec::new(),
    )
}

pub fn test_dataset_3() -> OsmTestData {
    //          1
    //          |
    //          |
    //    5 - - 3 - - 6
    //   /|     |     |\
    //  | |     |     | |
    //  | \ - - 4 - - / |
    //  |               |
    //  \ - - - 7 - - - /

    let tags_with_highway = HashMap::from([("highway".to_string(), "primary".to_string())]);

    (
        vec![
            OsmNode {
                id: 1,
                lat: 1.0,
                lon: 1.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 3,
                lat: 3.0,
                lon: 3.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 4,
                lat: 4.0,
                lon: 4.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 5,
                lat: 5.0,
                lon: 5.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 6,
                lat: 6.0,
                lon: 6.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 7,
                lat: 7.0,
                lon: 7.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
        ],
        vec![
            OsmWay {
                id: 13,
                point_ids: vec![1, 3],
                tags: Some(tags_with_highway.clone()),
            },
            OsmWay {
                id: 34,
                point_ids: vec![3, 4],
                tags: Some(tags_with_highway.clone()),
            },
            OsmWay {
                id: 53,
                point_ids: vec![5, 3],
                tags: Some(tags_with_highway.clone()),
            },
            OsmWay {
                id: 36,
                point_ids: vec![3, 6],
                tags: Some(tags_with_highway.clone()),
            },
            OsmWay {
                id: 54,
                point_ids: vec![5, 4],
                tags: Some(tags_with_highway.clone()),
            },
            OsmWay {
                id: 64,
                point_ids: vec![6, 4],
                tags: Some(tags_with_highway.clone()),
            },
            OsmWay {
                id: 576,
                point_ids: vec![5, 7, 6],
                tags: Some(tags_with_highway.clone()),
            },
        ],
        Vec::new(),
    )
}

pub fn graph_from_test_dataset(test_data: OsmTestData) -> MapDataGraph {
    let map_data = MapDataGraph::new_test();
    let (test_nodes, test_ways, test_relations) = &test_data;

    let test_tile_id = crate::rmdf::TileId { col: 0, row: 0 };

    // Build a map from way ID to the lines it contains
    let mut way_to_lines: HashMap<u64, Vec<MapDataLineRef>> = HashMap::new();

    // First pass: Create MapDataPoints without lines
    for test_node in test_nodes {
        let point = MapDataPoint {
            id: test_node.id,
            lat: test_node.lat as f32,
            lon: test_node.lon as f32,
            lines: Vec::new(),
            rules: Vec::new(),
            residential_in_proximity: test_node.residential_in_proximity,
            nogo_area: test_node.nogo_area,
        };
        map_data.test_insert_point(point);
    }

    // Second pass: Create lines and track way->lines mapping
    for (way_idx, test_way) in test_ways.iter().enumerate() {
        let is_one_way = test_way.is_one_way();
        let is_roundabout = test_way.is_roundabout();

        let direction = if is_roundabout {
            LineDirection::Roundabout
        } else if is_one_way {
            LineDirection::OneWay
        } else {
            LineDirection::BothWays
        };

        let mut way_lines = Vec::new();

        // Create lines between consecutive points in the way
        for i in 0..test_way.point_ids.len() - 1 {
            let point_a_id = test_way.point_ids[i];
            let point_b_id = test_way.point_ids[i + 1];
            let line_id = (way_idx * 1000 + i) as u64;

            let line = MapDataLine {
                points: (
                    MapDataPointRef::new(test_tile_id, point_a_id),
                    MapDataPointRef::new(test_tile_id, point_b_id),
                ),
                direction: direction.clone(),
                tags: ElementTagSetRef::new(test_tile_id, 0),
            };

            map_data.test_insert_line(line, line_id);

            let line_ref = MapDataLineRef::new(test_tile_id, line_id);
            way_lines.push(line_ref.clone());

            // Update point line references
            map_data.test_add_line_to_point(point_a_id, line_ref.clone());
            map_data.test_add_line_to_point(point_b_id, line_ref);
        }

        way_to_lines.insert(test_way.id, way_lines);
    }

    // Third pass: Process relations and add rules to points
    for relation in test_relations {
        // Only process restriction relations
        if relation.tags.get("type").map(|t| t.as_str()) != Some("restriction") {
            continue;
        }

        let restriction_type = relation.tags.get("restriction").map(|t| t.as_str());
        let rule_type = match restriction_type {
            Some("no_left_turn")
            | Some("no_right_turn")
            | Some("no_straight_on")
            | Some("no_u_turn")
            | Some("no_entry")
            | Some("no_exit") => MapDataRuleType::NotAllowed,
            Some("only_left_turn")
            | Some("only_right_turn")
            | Some("only_straight_on")
            | Some("only_u_turn") => MapDataRuleType::OnlyAllowed,
            _ => continue,
        };

        // Find from, via, and to members
        let mut from_way_id: Option<u64> = None;
        let mut via_node_id: Option<u64> = None;
        let mut to_way_ids: Vec<u64> = Vec::new();

        for member in &relation.members {
            match member.role {
                OsmRelationMemberRole::From => {
                    if member.member_type == OsmRelationMemberType::Way {
                        from_way_id = Some(member.member_ref);
                    }
                }
                OsmRelationMemberRole::Via => {
                    if member.member_type == OsmRelationMemberType::Node {
                        via_node_id = Some(member.member_ref);
                    }
                }
                OsmRelationMemberRole::To => {
                    if member.member_type == OsmRelationMemberType::Way {
                        to_way_ids.push(member.member_ref);
                    }
                }
                OsmRelationMemberRole::Other(_) => {}
            }
        }

        let (Some(from_way), Some(via_node)) = (from_way_id, via_node_id) else {
            continue;
        };

        if to_way_ids.is_empty() {
            continue;
        }

        // Get the line refs for the from way
        let Some(from_lines) = way_to_lines.get(&from_way) else {
            continue;
        };

        // Collect all to_lines from all to_ways
        let mut all_to_lines = Vec::new();
        for to_way in &to_way_ids {
            if let Some(to_lines) = way_to_lines.get(to_way) {
                all_to_lines.extend(to_lines.iter().cloned());
            }
        }

        if all_to_lines.is_empty() {
            continue;
        }

        // Create the rule and add it to the via point
        let rule = MapDataRule {
            from_lines: from_lines.clone(),
            to_lines: all_to_lines,
            rule_type,
        };
        map_data.test_add_rule_to_point(via_node, rule);
    }
    map_data
}
pub fn line_is_between_point_ids(
    ctx: &RoutingContext<'_>,
    line: &MapDataLineRef,
    id1: u64,
    id2: u64,
) -> bool {
    let line = ctx.line(line);
    let point_ids = [ctx.point(&line.points.0).id, ctx.point(&line.points.1).id];
    point_ids.contains(&id1) && point_ids.contains(&id2)
}
pub fn route_matches_ids(ctx: &RoutingContext<'_>, route: Route, ids: &[u64]) -> bool {
    ids.iter()
        .enumerate()
        .map(|(idx, &id)| {
            let route_segment = route.get_segment_by_index(idx);
            if let Some(route_segment) = route_segment {
                if ctx.point(route_segment.get_end_point()).id == id {
                    return true;
                }
            }
            false
        })
        .all(|v| v)
}

#[allow(dead_code)]
pub fn get_test_data_osm_json_nodes() -> Vec<&'static str> {
    vec![
        r#"{"#,
        r#"  "version": 0.6,"#,
        r#"  "generator": "Overpass API 0.7.62.1 084b4234","#,
        r#"  "osm3s": {"#,
        r#"    "timestamp_osm_base": "2024-07-23T11:01:29Z","#,
        r#"    "copyright": "The data included in this document is from www.openstreetmap.org. The data is made available under ODbL.""#,
        r#"  },"#,
        r#"  "elements": ["#,
        r#""#,
        r#"{"#,
        r#"  "type": "node","#,
        r#"  "id": 18483373,"#,
        r#"  "lat": 57.1995635,"#,
        r#"  "lon": 25.0419124",#,
        r#"  "tags": {"#,
        r#"    "highway": "traffic_signals""#,
        r#"  }"#,
        r#"},"#,
        r#"{"#,
        r#"  "type": "way","#,
        r#"  "id": 83402701,"#,
        r#"  "nodes": ["#,
        r#"    249790708,"#,
        r#"    1862710503"#,
        r#"  ],"#,
        r#"  "tags": {"#,
        r#"    "highway": "unclassified""#,
        r#"  }"#,
        r#"},"#,
        r#"{"#,
        r#"  "type": "relation","#,
        r#"  "id": 16896043,"#,
        r#"  "members": ["#,
        r#"    {"#,
        r#"      "type": "way","#,
        r#"      "ref": 979880972,"#,
        r#"      "role": "from""#,
        r#"    },"#,
        r#"    {"#,
        r#"      "type": "node","#,
        r#"      "ref": 32705747,"#,
        r#"      "role": "via""#,
        r#"    },"#,
        r#"    {"#,
        r#"      "type": "way","#,
        r#"      "ref": 69666743,"#,
        r#"      "role": "to""#,
        r#"    }"#,
        r#"  ],"#,
        r#"  "tags": {"#,
        r#"    "restriction": "no_right_turn","#,
        r#"    "type": "restriction""#,
        r#"  }"#,
        r#"}"#,
        r#"  ]"#,
        r#"}"#,
    ]
}
#[allow(dead_code)]
pub fn get_test_data_osm_json() -> Vec<&'static str> {
    vec![
        r#"{"#,
        r#"  "version": 0.6,"#,
        r#"  "generator": "Overpass API 0.7.62.1 084b4234","#,
        r#"  "osm3s": {"#,
        r#"    "timestamp_osm_base": "2024-07-23T11:01:29Z","#,
        r#"    "copyright": "The data included in this document is from www.openstreetmap.org. The data is made available under ODbL.""#,
        r#"  },"#,
        r#"  "elements": ["#,
        r#""#,
        r#"{"#,
        r#"  "type": "node","#,
        r#"  "id": 18483373,"#,
        r#"  "lat": 57.1995635,"#,
        r#"  "lon": 25.0419124"#,
        r#"},"#,
        r#"{"#,
        r#"  "type": "node","#,
        r#"  "id": 18483475,"#,
        r#"  "lat": 57.1455443,"#,
        r#"  "lon": 24.8581908,"#,
        r#"  "tags": {"#,
        r#"    "highway": "traffic_signals""#,
        r#"  }"#,
        r#"},"#,
        r#"{"#,
        r#"  "type": "node","#,
        r#"  "id": 18483521,"#,
        r#"  "lat": 57.1485002,"#,
        r#"  "lon": 24.8561211"#,
        r#"},"#,
        r#"            {"#,
        r#"  "type": "way","#,
        r#"  "id": 80944232,"#,
        r#"  "nodes": ["#,
        r#"    1242609397,"#,
        r#"    923273378,"#,
        r#"    923273458"#,
        r#"  ],"#,
        r#"  "tags": {"#,
        r#"    "highway": "living_street","#,
        r#"    "name": "Alūksnes iela""#,
        r#"  }"#,
        r#"},"#,
        r#"{"#,
        r#"  "type": "way","#,
        r#"  "id": 83402701,"#,
        r#"  "nodes": ["#,
        r#"    249790708,"#,
        r#"    1862710503"#,
        r#"  ],"#,
        r#"  "tags": {"#,
        r#"    "highway": "unclassified""#,
        r#"  }"#,
        r#"},"#,
        r#"        {"#,
        r#"  "type": "relation","#,
        r#"  "id": 14385700,"#,
        r#"  "members": ["#,
        r#"    {"#,
        r#"      "type": "way","#,
        r#"      "ref": 37854864,"#,
        r#"      "role": "from""#,
        r#"    },"#,
        r#"    {"#,
        r#"      "type": "node","#,
        r#"      "ref": 6721285159,"#,
        r#"      "role": "via""#,
        r#"    },"#,
        r#"    {"#,
        r#"      "type": "way","#,
        r#"      "ref": 37854864,"#,
        r#"      "role": "to""#,
        r#"    }"#,
        r#"  ],"#,
        r#"  "tags": {"#,
        r#"    "restriction": "no_u_turn","#,
        r#"    "type": "restriction""#,
        r#"  }"#,
        r#"},"#,
        r#"{"#,
        r#"  "type": "relation","#,
        r#"  "id": 16896043,"#,
        r#"  "members": ["#,
        r#"    {"#,
        r#"      "type": "way","#,
        r#"      "ref": 979880972,"#,
        r#"      "role": "from""#,
        r#"    },"#,
        r#"    {"#,
        r#"      "type": "node","#,
        r#"      "ref": 32705747,"#,
        r#"      "role": "via""#,
        r#"    },"#,
        r#"    {"#,
        r#"      "type": "way","#,
        r#"      "ref": 69666743,"#,
        r#"      "role": "to""#,
        r#"    }"#,
        r#"  ],"#,
        r#"  "tags": {"#,
        r#"    "restriction": "no_right_turn","#,
        r#"    "type": "restriction""#,
        r#"  }"#,
        r#"}"#,
        r#"  ]"#,
        r#"}"#,
    ]
}

#[cfg(test)]
mod tests {
    use super::{test_dataset_1, test_dataset_2, RoutingTestContext};

    #[test]
    fn routing_test_context_exposes_local_graph_and_context() {
        let test_ctx = RoutingTestContext::new(test_dataset_1());
        let ctx = test_ctx.resolver();

        let start = test_ctx.point(1);
        let finish = test_ctx.point(7);

        assert_eq!(ctx.point(&start).id, 1);
        assert_eq!(ctx.point(&finish).id, 7);
    }

    #[test]
    fn routing_test_contexts_support_multiple_graphs_in_one_process() {
        let first = RoutingTestContext::new(test_dataset_1());
        let second = RoutingTestContext::new(test_dataset_2());

        let first_ctx = first.resolver();
        let second_ctx = second.resolver();

        assert!(first.graph.test_get_point_ref_by_id(&121).is_none());
        assert!(second.graph.test_get_point_ref_by_id(&121).is_some());

        assert_eq!(first_ctx.point(&first.point(7)).lines.len(), 1);
        assert_eq!(second_ctx.point(&second.point(7)).lines.len(), 4);
    }
}
