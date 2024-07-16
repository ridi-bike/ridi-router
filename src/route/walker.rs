use crate::map_data_graph::{MapDataGraph, MapDataLine, MapDataPoint};

#[derive(Debug, PartialEq)]
pub enum RouterWalker {
    WrongForkChoice {
        id: u64,
        available_fork_ids: Vec<u64>,
    },
}

pub type RouteElement = (MapDataLine, MapDataPoint);

pub struct RouteWalker<'a> {
    map_data_graph: &'a MapDataGraph,
    start: &'a MapDataPoint,
    end: &'a MapDataPoint,
    route_walked: Vec<RouteElement>,
    next_fork_choice_point_id: Option<u64>,
}

#[derive(Debug, PartialEq)]
pub enum RouteWalkerMoveResult {
    Fork(Vec<RouteElement>),
    DeadEnd,
    Finish,
}

impl<'a> RouteWalker<'a> {
    pub fn new(
        map_data_graph: &'a MapDataGraph,
        start: &'a MapDataPoint,
        end: &'a MapDataPoint,
    ) -> Self {
        Self {
            map_data_graph,
            start,
            end,
            route_walked: Vec::new(),
            next_fork_choice_point_id: None,
        }
    }

    fn get_available_lines(&self, point: &MapDataPoint) -> Vec<RouteElement> {
        let prev_point = if let Some(idx) = self.route_walked.len().checked_sub(2) {
            if let Some(p) = self.route_walked.get(idx) {
                &p.1
            } else {
                &self.start
            }
        } else {
            &self.start
        };

        self.map_data_graph
            .get_adjacent(&point)
            .into_iter()
            .filter(|(_, p)| p.id != prev_point.id)
            .collect()
    }

    pub fn set_fork_choice_point_id(&mut self, id: &u64) -> () {
        self.next_fork_choice_point_id = Some(*id);
    }

    pub fn move_forward_to_next_fork(&mut self) -> Result<RouteWalkerMoveResult, RouterWalker> {
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
                    .ok_or(RouterWalker::WrongForkChoice {
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

    pub fn move_backwards_to_prev_fork(&mut self) -> Option<Vec<(MapDataLine, MapDataPoint)>> {
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
    use core::panic;

    use crate::{
        route::walker::{RouteWalkerMoveResult, RouterWalker},
        test_utils::{get_point_with_id, get_test_map_data_graph, line_is_between_point_ids},
    };

    use super::RouteWalker;

    #[test]
    fn walker_same_start_end() {
        let map_data = get_test_map_data_graph();

        let mut walker = RouteWalker::new(&map_data, get_point_with_id(1), get_point_with_id(1));

        assert_eq!(
            walker.move_forward_to_next_fork(),
            Ok(RouteWalkerMoveResult::Finish)
        );
        assert_eq!(walker.get_route().clone(), Vec::new());
    }

    #[test]
    fn walker_error_on_wrong_choice() {
        let map_data = get_test_map_data_graph();

        let mut walker = RouteWalker::new(&map_data, get_point_with_id(2), get_point_with_id(3));

        walker.set_fork_choice_point_id(&99);

        assert_eq!(
            walker.move_forward_to_next_fork(),
            Err(RouterWalker::WrongForkChoice {
                id: 99,
                available_fork_ids: vec![1, 3]
            })
        );
        assert_eq!(walker.get_route().clone(), Vec::new());
    }

    #[test]
    fn waker_one_step_no_fork() {
        let map_data = get_test_map_data_graph();

        let from_id = 1;
        let to_id = 2;

        let mut walker = RouteWalker::new(
            &map_data,
            get_point_with_id(from_id.clone()),
            get_point_with_id(to_id.clone()),
        );
        assert_eq!(
            walker.move_forward_to_next_fork(),
            Ok(RouteWalkerMoveResult::Finish)
        );
        let route = walker.get_route().clone();
        assert_eq!(route.len(), 1);
        let el = route.get(0);
        if let Some((l, p)) = el {
            assert!(line_is_between_point_ids(l.clone(), from_id, to_id));
            assert_eq!(p.id, to_id);
        } else {
            assert!(false)
        }
    }

    #[test]
    fn walker_choose_path() {
        let map_data = get_test_map_data_graph();

        let from_id = 1;
        let to_id = 7;
        let fork_ch_id = 6;

        let mut walker = RouteWalker::new(
            &map_data,
            get_point_with_id(from_id.clone()),
            get_point_with_id(to_id.clone()),
        );

        let choices = match walker.move_forward_to_next_fork() {
            Err(_) => panic!("Error received from move"),
            Ok(RouteWalkerMoveResult::Fork(c)) => c,
            _ => panic!("did not get choices for routes"),
        };

        assert_eq!(choices.len(), 3);

        choices.iter().for_each(|(l, p)| {
            assert!(p.id == 5 || p.id == 4 || p.id == 6);
            assert!(
                line_is_between_point_ids(l.clone(), 5, 3)
                    || line_is_between_point_ids(l.clone(), 4, 3)
                    || line_is_between_point_ids(l.clone(), 6, 3)
            )
        });

        walker.set_fork_choice_point_id(&fork_ch_id);

        assert!(walker.move_forward_to_next_fork() == Ok(RouteWalkerMoveResult::Finish));

        let route = walker.get_route().clone();
        assert_eq!(route.len(), 4);

        let el = route.get(0);
        assert!(el.is_some());
        if let Some((l, p)) = el {
            assert!(line_is_between_point_ids(l.clone(), 2, 1));
            assert_eq!(p.id, 2);
        }

        let el = route.get(1);
        assert!(el.is_some());
        if let Some((l, p)) = el {
            assert!(line_is_between_point_ids(l.clone(), 3, 2));
            assert_eq!(p.id, 3);
        }

        let el = route.get(2);
        assert!(el.is_some());
        if let Some((l, p)) = el {
            assert!(line_is_between_point_ids(l.clone(), 6, 3));
            assert_eq!(p.id, 6);
        }
        let el = route.get(3);
        assert!(el.is_some());
        if let Some((l, p)) = el {
            assert!(line_is_between_point_ids(l.clone(), 7, 6));
            assert_eq!(p.id, 7);
        }
    }

    #[test]
    fn walker_reach_dead_end_walk_back() {
        let map_data = get_test_map_data_graph();

        let from_id = 1;
        let to_id = 4;
        let fork_ch_id_1 = 6;
        let fork_ch_id_2 = 4;

        let mut walker = RouteWalker::new(
            &map_data,
            get_point_with_id(from_id.clone()),
            get_point_with_id(to_id.clone()),
        );

        let choices = match walker.move_forward_to_next_fork() {
            Err(_) => panic!("Error received from move"),
            Ok(RouteWalkerMoveResult::Fork(c)) => c,
            _ => panic!("did not get choices for routes"),
        };
        assert_eq!(choices.len(), 3);

        choices.iter().for_each(|(l, p)| {
            assert!(p.id == 5 || p.id == 4 || p.id == 6);
            assert!(
                line_is_between_point_ids(l.clone(), 5, 3)
                    || line_is_between_point_ids(l.clone(), 4, 3)
                    || line_is_between_point_ids(l.clone(), 6, 3)
            )
        });

        walker.set_fork_choice_point_id(&fork_ch_id_1);

        assert!(walker.move_forward_to_next_fork() == Ok(RouteWalkerMoveResult::DeadEnd));

        let choices = match walker.move_backwards_to_prev_fork() {
            None => panic!("Expected to be back at point 3 with choices"),
            Some(c) => c,
        };

        choices.iter().for_each(|(l, p)| {
            assert!(p.id == 5 || p.id == 4 || p.id == 6);
            assert!(
                line_is_between_point_ids(l.clone(), 5, 3)
                    || line_is_between_point_ids(l.clone(), 4, 3)
                    || line_is_between_point_ids(l.clone(), 6, 3)
            )
        });

        walker.set_fork_choice_point_id(&fork_ch_id_2);

        assert!(walker.move_forward_to_next_fork() == Ok(RouteWalkerMoveResult::Finish));

        let route = walker.get_route().clone();
        assert_eq!(route.len(), 3);

        let el = route.get(0);
        assert!(el.is_some());
        if let Some((l, p)) = el {
            assert!(line_is_between_point_ids(l.clone(), 2, 1));
            assert_eq!(p.id, 2);
        }

        let el = route.get(1);
        assert!(el.is_some());
        if let Some((l, p)) = el {
            assert!(line_is_between_point_ids(l.clone(), 3, 2));
            assert_eq!(p.id, 3);
        }

        let el = route.get(2);
        assert!(el.is_some());
        if let Some((l, p)) = el {
            assert!(line_is_between_point_ids(l.clone(), 4, 3));
            assert_eq!(p.id, 4);
        }
    }
}
