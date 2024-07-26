use crate::map_data_graph::{
    MapDataGraph, MapDataLine, MapDataPoint, MapDataWay, MapDataWayPoints, OsmNode, OsmWay,
};
use crate::route::walker::Route;

pub fn get_test_data() -> (Vec<OsmNode>, Vec<OsmWay>) {
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
            },
            OsmWay {
                id: 5367,
                point_ids: vec![5, 3, 6, 7],
            },
            OsmWay {
                id: 489,
                point_ids: vec![4, 8, 9],
            },
            OsmWay {
                id: 68,
                point_ids: vec![6, 8],
            },
            OsmWay {
                id: 1112,
                point_ids: vec![11, 12],
            },
        ],
    )
}

pub fn get_test_map_data_graph() -> MapDataGraph {
    let test_data = get_test_data();
    let mut map_data = MapDataGraph::new();
    let (test_nodes, test_ways) = &test_data;
    for test_node in test_nodes {
        map_data.insert_node(test_node.clone());
    }
    for test_way in test_ways {
        map_data.insert_way(test_way.clone()).unwrap();
    }

    map_data
}

pub fn line_is_between_point_ids(line: MapDataLine, id1: u64, id2: u64) -> bool {
    let ids = [line.points.0, line.points.1];
    line.id
        .split("-")
        .collect::<Vec<_>>()
        .contains(&id1.to_string().as_str())
        && line
            .id
            .split("-")
            .collect::<Vec<_>>()
            .contains(&id2.to_string().as_str())
        && ids.contains(&id1)
        && ids.contains(&id2)
}

pub fn route_matches_ids(route: Route, ids: Vec<u64>) -> bool {
    ids.iter()
        .enumerate()
        .map(|(idx, &id)| {
            let route_segment = route.get_segment_by_index(idx);
            if let Some(route_segment) = route_segment {
                if route_segment.get_end_point().id == id {
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
