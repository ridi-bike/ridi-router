use crate::map_data_graph::{
    MapDataGraph, MapDataLine, MapDataNode, MapDataPoint, MapDataWay, MapDataWayNodeIds,
};

pub fn get_test_data() -> (Vec<MapDataNode>, Vec<MapDataWay>) {
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
            MapDataNode {
                id: 1,
                lat: 1.0,
                lon: 1.0,
            },
            MapDataNode {
                id: 2,
                lat: 2.0,
                lon: 2.0,
            },
            MapDataNode {
                id: 3,
                lat: 3.0,
                lon: 3.0,
            },
            MapDataNode {
                id: 4,
                lat: 4.0,
                lon: 4.0,
            },
            MapDataNode {
                id: 5,
                lat: 5.0,
                lon: 5.0,
            },
            MapDataNode {
                id: 6,
                lat: 6.0,
                lon: 6.0,
            },
            MapDataNode {
                id: 7,
                lat: 7.0,
                lon: 7.0,
            },
            MapDataNode {
                id: 8,
                lat: 8.0,
                lon: 8.0,
            },
            MapDataNode {
                id: 9,
                lat: 9.0,
                lon: 9.0,
            },
            MapDataNode {
                id: 11,
                lat: 11.0,
                lon: 11.0,
            },
            MapDataNode {
                id: 12,
                lat: 12.0,
                lon: 12.0,
            },
        ],
        vec![
            MapDataWay {
                id: 1234,
                node_ids: MapDataWayNodeIds::from_vec(vec![1, 2, 3, 4]),
                one_way: false,
            },
            MapDataWay {
                id: 5367,
                node_ids: MapDataWayNodeIds::from_vec(vec![5, 3, 6, 7]),
                one_way: false,
            },
            MapDataWay {
                id: 489,
                node_ids: MapDataWayNodeIds::from_vec(vec![4, 8, 9]),
                one_way: false,
            },
            MapDataWay {
                id: 68,
                node_ids: MapDataWayNodeIds::from_vec(vec![6, 8]),
                one_way: false,
            },
            MapDataWay {
                id: 1112,
                node_ids: MapDataWayNodeIds::from_vec(vec![11, 12]),
                one_way: false,
            },
        ],
    )
}

pub fn get_point_with_id(id: u64) -> MapDataPoint {
    MapDataPoint {
        id,
        lat: id as f64,
        lon: id as f64,
        fork: false,
        part_of_ways: Vec::new(),
    }
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
    let ids = [line.point_ids.0, line.point_ids.1];
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

pub fn route_matches_ids(route: Vec<(MapDataLine, MapDataPoint)>, ids: Vec<u64>) -> bool {
    ids.iter()
        .enumerate()
        .map(|(idx, &id)| {
            let route_element = route.get(idx);
            if let Some(route_element) = route_element {
                if route_element.1.id == id {
                    return true;
                }
            }
            false
        })
        .all(|v| v)
}
