use crate::map_data_graph::{MapDataGraph, MapDataLine, MapDataPoint};

pub enum RouterError {
    WrongForkChoice {
        id: u64,
        available_fork_ids: Vec<u64>,
    },
}

pub type RouteElement = (MapDataLine, MapDataPoint);

pub struct RouteWalker {
    map_data_graph: MapDataGraph,
    start: MapDataPoint,
    end: MapDataPoint,
    route_walked: Vec<RouteElement>,
    next_fork_choice_point_id: Option<u64>,
}

pub enum RouteWalkerMoveResult {
    Fork(Vec<RouteElement>),
    DeadEnd,
    Finish,
}

impl RouteWalker {
    pub fn new(map_data_graph: MapDataGraph, start: MapDataPoint, end: MapDataPoint) -> Self {
        Self {
            map_data_graph,
            start,
            end,
            route_walked: Vec::new(),
            next_fork_choice_point_id: None,
        }
    }

    fn get_available_lines(&self, point: &MapDataPoint) -> Vec<RouteElement> {
        self.map_data_graph
            .get_adjacent(&point)
            .into_iter()
            .filter(|(_, p)| p.id != point.id)
            .collect()
    }

    pub fn move_forward_to_next_fork(&mut self) -> Result<RouteWalkerMoveResult, RouterError> {
        loop {
            let point = match self.route_walked.last() {
                Some((_, p)) => p,
                None => &self.start,
            };
            if point.id == self.end.id {
                return Ok(RouteWalkerMoveResult::Finish);
            }

            let available_lines = self.get_available_lines(point);

            if available_lines.len() > 1 && self.next_fork_choice_point_id.is_none() {
                return Ok(RouteWalkerMoveResult::Fork(available_lines));
            }

            let next_index = if let Some(next_id) = self.next_fork_choice_point_id {
                self.next_fork_choice_point_id = None;
                available_lines
                    .iter()
                    .position(|(_, point)| point.id == next_id)
                    .ok_or(RouterError::WrongForkChoice {
                        id: next_id,
                        available_fork_ids: available_lines.iter().map(|(_, p)| p.id).collect(),
                    })?
            } else {
                0
            };

            let next_point = match available_lines.get(next_index) {
                None => return Ok(RouteWalkerMoveResult::DeadEnd),
                Some(element) => element.clone(),
            };

            self.route_walked.push(next_point);
        }
    }

    pub fn move_backwards_to_fork(&mut self) -> Option<Vec<(MapDataLine, MapDataPoint)>> {
        self.next_fork_choice_point_id = None;
        let current_fork = self.route_walked.pop();
        if current_fork.is_none() {
            return None;
        }
        while let Some((
            _,
            _point @ MapDataPoint {
                id: _,
                fork: false,
                lon: _,
                lat: _,
                part_of_ways: _,
            },
        )) = self.route_walked.last()
        {
            self.route_walked.pop();
        }

        if let Some((_, point)) = self.route_walked.last() {
            return Some(self.get_available_lines(point));
        }

        None
    }

    pub fn get_route(&self) -> &Vec<(MapDataLine, MapDataPoint)> {
        &self.route_walked
    }
}

#[cfg(test)]
mod tests {
    use crate::map_data_graph::{MapDataGraph, MapDataNode, MapDataWay};

    #[test]
    fn walk_line() {
        let test_data: Vec<(Vec<MapDataNode>, Vec<MapDataWay>)> = vec![(
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
            ],
            vec![
                MapDataWay {
                    id: 1234,
                    node_ids: vec![1, 2, 3, 4],
                    one_way: false,
                },
                MapDataWay {
                    id: 5367,
                    node_ids: vec![5, 3, 6, 7],
                    one_way: false,
                },
            ],
        )];
        let mut map_data = MapDataGraph::new();
        for (test_nodes, test_ways) in &test_data {
            for test_node in test_nodes {
                map_data.insert_node(test_node.clone());
            }
            for test_way in test_ways {
                map_data.insert_way(test_way.clone());
            }
        }
    }
}
