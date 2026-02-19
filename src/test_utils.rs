use std::collections::HashMap;

use crate::{
    map_data::{
        graph::{ElementTagSetRef, MapDataGraph, MapDataLineRef, MapDataPointRef, MAP_DATA_GRAPH},
        line::{LineDirection, MapDataLine},
        osm::{OsmNode, OsmRelation, OsmWay},
        point::MapDataPoint,
    },
    router::route::Route,
};

pub type OsmTestData = (Vec<OsmNode>, Vec<OsmWay>, Vec<OsmRelation>);

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

// REMOVED: JSON support has been removed. Use graph_from_test_dataset instead.
// pub fn graph_from_test_file(file: &PathBuf) -> MapDataGraph {
//     ...
// }

pub fn graph_from_test_dataset(test_data: OsmTestData) -> MapDataGraph {
    let map_data = MapDataGraph::new_test();
    let (test_nodes, test_ways, _test_relations) = &test_data;

    // First pass: Create MapDataPoints without lines (we'll add lines after)
    let test_tile_id = crate::rmdf::TileId { col: 0, row: 0 };
    for test_node in test_nodes {
        let point = MapDataPoint {
            id: test_node.id,
            lat: test_node.lat as f32,
            lon: test_node.lon as f32,
            lines: Vec::new(), // Will be populated in second pass
            rules: Vec::new(),
            residential_in_proximity: test_node.residential_in_proximity,
            nogo_area: test_node.nogo_area,
        };
        map_data.test_insert_point(point);
    }

    // Second pass: Create lines and update point line references
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

        // Create lines between consecutive points in the way
        for i in 0..test_way.point_ids.len() - 1 {
            let point_a_id = test_way.point_ids[i];
            let point_b_id = test_way.point_ids[i + 1];
            let line_id = (way_idx * 1000 + i) as u64; // Generate a unique line ID

            let line = MapDataLine {
                points: (
                    MapDataPointRef::new(test_tile_id, point_a_id),
                    MapDataPointRef::new(test_tile_id, point_b_id),
                ),
                direction: direction.clone(),
                tags: ElementTagSetRef::new(test_tile_id, 0), // Dummy tag ref
            };

            map_data.test_insert_line(line, line_id);

            // Update point line references
            map_data.test_add_line_to_point(point_a_id, MapDataLineRef::new(test_tile_id, line_id));
            map_data.test_add_line_to_point(point_b_id, MapDataLineRef::new(test_tile_id, line_id));
        }
    }

    map_data
}

pub fn set_graph_static(map_data: MapDataGraph) -> &'static MapDataGraph {
    MAP_DATA_GRAPH.get_or_init(|| map_data)
}

pub fn line_is_between_point_ids(line: &MapDataLineRef, id1: u64, id2: u64) -> bool {
    let point_ids = [
        line.get().points.0.get().id,
        line.get().points.1.get().id,
    ];
    point_ids.contains(&id1) && point_ids.contains(&id2)
}
pub fn route_matches_ids(route: Route, ids: Vec<u64>) -> bool {
    ids.iter()
        .enumerate()
        .map(|(idx, &id)| {
            let route_segment = route.get_segment_by_index(idx);
            if let Some(route_segment) = route_segment {
                if route_segment.get_end_point().get().id == id {
                    return true;
                }
            }
            false
        })
        .all(|v| v)
}

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
