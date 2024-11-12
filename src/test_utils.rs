use std::collections::HashMap;

use crate::{
    map_data::{
        graph::{MapDataGraph, MapDataLineRef},
        osm::{OsmNode, OsmRelation, OsmWay},
    },
    osm_data_reader::{DataSource, OsmDataReader},
    router::route::Route,
    MAP_DATA_GRAPH,
};

pub type OsmTestData = (Vec<OsmNode>, Vec<OsmWay>, Vec<OsmRelation>);

fn make_osm_point_with_id(id: u64) -> OsmNode {
    OsmNode {
        lat: id as f64,
        lon: id as f64,
        id,
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
            },
            OsmNode {
                id: 2,
                lat: 2.0,
                lon: 2.0,
            },
            OsmNode {
                id: 3,
                lat: 3.0,
                lon: 3.0,
            },
            OsmNode {
                id: 4,
                lat: 4.0,
                lon: 4.0,
            },
            OsmNode {
                id: 5,
                lat: 5.0,
                lon: 5.0,
            },
            OsmNode {
                id: 6,
                lat: 6.0,
                lon: 6.0,
            },
            OsmNode {
                id: 7,
                lat: 7.0,
                lon: 7.0,
            },
            OsmNode {
                id: 8,
                lat: 8.0,
                lon: 8.0,
            },
            OsmNode {
                id: 9,
                lat: 9.0,
                lon: 9.0,
            },
            OsmNode {
                id: 11,
                lat: 11.0,
                lon: 11.0,
            },
            OsmNode {
                id: 12,
                lat: 12.0,
                lon: 12.0,
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
            },
            OsmNode {
                id: 3,
                lat: 3.0,
                lon: 3.0,
            },
            OsmNode {
                id: 4,
                lat: 4.0,
                lon: 4.0,
            },
            OsmNode {
                id: 5,
                lat: 5.0,
                lon: 5.0,
            },
            OsmNode {
                id: 6,
                lat: 6.0,
                lon: 6.0,
            },
            OsmNode {
                id: 7,
                lat: 7.0,
                lon: 7.0,
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

pub fn graph_from_test_file(file: &str) -> MapDataGraph {
    let data_source = DataSource::JsonFile {
        file: file.to_string(),
    };
    let data_reader = OsmDataReader::new(data_source);
    data_reader.read_data().unwrap()
}

pub fn graph_from_test_dataset(test_data: OsmTestData) -> MapDataGraph {
    let mut map_data = MapDataGraph::new();
    let (test_nodes, test_ways, test_relations) = &test_data;
    for test_node in test_nodes {
        map_data.insert_node(test_node.clone());
    }
    for test_way in test_ways {
        map_data
            .insert_way(test_way.clone())
            .expect("failed to insert way");
    }
    for test_relation in test_relations {
        map_data
            .insert_relation(test_relation.clone())
            .expect("failed to insert relation");
    }

    map_data
}

pub fn set_graph_static(map_data: MapDataGraph) -> &'static MapDataGraph {
    MAP_DATA_GRAPH.get_or_init(|| map_data)
}

pub fn line_is_between_point_ids(line: &MapDataLineRef, id1: u64, id2: u64) -> bool {
    let point_ids = [
        line.borrow().points.0.borrow().id,
        line.borrow().points.1.borrow().id,
    ];
    line.borrow()
        .id
        .split("-")
        .collect::<Vec<_>>()
        .contains(&id1.to_string().as_str())
        && line
            .borrow()
            .id
            .split("-")
            .collect::<Vec<_>>()
            .contains(&id2.to_string().as_str())
        && point_ids.contains(&id1)
        && point_ids.contains(&id2)
}

pub fn route_matches_ids(route: Route, ids: Vec<u64>) -> bool {
    ids.iter()
        .enumerate()
        .map(|(idx, &id)| {
            let route_segment = route.get_segment_by_index(idx);
            if let Some(route_segment) = route_segment {
                if route_segment.get_end_point().borrow().id == id {
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
