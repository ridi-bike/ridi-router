use std::{fmt::Debug, rc::Rc};

use crate::{
    debug_writer::DebugLogger,
    map_data_graph::{MapDataGraph, MapDataPointRef, MapDataRuleType},
};

use super::{segment::RouteSegment, segment_list::RouteSegmentList, Route};

#[derive(Debug, PartialEq)]
pub enum RouterWalkerError {
    WrongForkChoice {
        id: u64,
        available_fork_ids: Vec<u64>,
    },
}

pub struct RouteWalker<'a> {
    walker_id: u16,
    map_data_graph: &'a MapDataGraph,
    start: MapDataPointRef,
    end: MapDataPointRef,
    route_walked: Route,
    next_fork_choice_point: Option<MapDataPointRef>,
    pub debug_logger: Box<dyn DebugLogger>,
}

#[derive(Debug, PartialEq)]
pub enum RouteWalkerMoveResult {
    Fork(RouteSegmentList),
    DeadEnd,
    Finish,
}

impl<'a> RouteWalker<'a> {
    pub fn new(
        map_data_graph: &'a MapDataGraph,
        start: MapDataPointRef,
        end: MapDataPointRef,
        debug_logger: Box<dyn DebugLogger>,
    ) -> Self {
        Self {
            walker_id: 1,
            map_data_graph,
            start: Rc::clone(&start),
            end,
            route_walked: Route::new(),
            next_fork_choice_point: None,
            debug_logger,
        }
    }

    pub fn split(&self) -> Self {
        Self {
            walker_id: self.walker_id + 1,
            map_data_graph: self.map_data_graph,
            start: Rc::clone(&self.start),
            end: Rc::clone(&self.end),
            route_walked: self.route_walked.clone(),
            next_fork_choice_point: None,
            debug_logger: self.debug_logger.split(),
        }
    }

    fn get_next_segments(&self) -> RouteSegmentList {
        let (center_point, center_line) =
            if let Some(idx) = self.route_walked.get_segment_count().checked_sub(1) {
                if let Some(p) = self.route_walked.get_segment_by_index(idx) {
                    (p.get_end_point(), Some(p.get_line()))
                } else {
                    (&self.start, None)
                }
            } else {
                (&self.start, None)
            };

        let center_line = match center_line {
            Some(l) => l,
            None => {
                let center_point_borrowed = center_point.borrow();

                let not_allow_rules = center_point_borrowed
                    .rules
                    .iter()
                    .filter(|rule| rule.rule_type == MapDataRuleType::NotAllowed)
                    .collect::<Vec<_>>();
                let segments = self.map_data_graph.get_adjacent(Rc::clone(&center_point));
                let segment_list = segments
                    .iter()
                    .filter_map(|(l, p)| {
                        if l.borrow().one_way && &l.borrow().points.1 == center_point {
                            return None;
                        }
                        if not_allow_rules.len() > 0 {
                            let not_allow_rules_for_segment = not_allow_rules
                                .iter()
                                .filter(|rule| rule.to_lines.contains(&l))
                                .collect::<Vec<_>>();
                            let other_segments =
                                segments.iter().filter(|s| &s.0 != l).collect::<Vec<_>>();

                            if not_allow_rules_for_segment.iter().any(|rule| {
                                other_segments
                                    .iter()
                                    .all(|seg| rule.to_lines.contains(&seg.0))
                            }) {
                                return None;
                            }
                        }
                        Some(RouteSegment::new(Rc::clone(&l), Rc::clone(&p)))
                    })
                    .collect::<RouteSegmentList>();
                return segment_list;
            }
        };

        let prev_point = if let Some(idx) = self.route_walked.get_segment_count().checked_sub(2) {
            if let Some(p) = self.route_walked.get_segment_by_index(idx) {
                &p.get_end_point().borrow()
            } else {
                &self.start.borrow()
            }
        } else {
            &self.start.borrow()
        };

        let center_point_borrowed = center_point.borrow();
        let only_allow_rules = center_point_borrowed
            .rules
            .iter()
            .filter(|rule| {
                rule.rule_type == MapDataRuleType::OnlyAllowed
                    && rule.from_lines.contains(&center_line)
            })
            .collect::<Vec<_>>();

        let not_allow_rules = center_point_borrowed
            .rules
            .iter()
            .filter(|rule| {
                rule.rule_type == MapDataRuleType::NotAllowed
                    && rule.from_lines.contains(&center_line)
            })
            .collect::<Vec<_>>();

        self.map_data_graph
            .get_adjacent(Rc::clone(&center_point))
            .into_iter()
            .filter(|(line_next, point_next)| {
                // do not offer the same line as you came from
                if point_next.borrow().id == prev_point.id {
                    return false;
                }

                // exclude if next line is one way and the direction is backwards
                if line_next.borrow().one_way && &line_next.borrow().points.1 == center_point {
                    return false;
                }

                // if no rules exist, don't check anything further
                if center_point.borrow().rules.len() == 0 {
                    return true;
                }

                // if not allow rules exist, make sure next line is not in them
                if not_allow_rules
                    .iter()
                    .any(|rule| rule.to_lines.contains(line_next))
                {
                    return false;
                }

                // if only allow rules exist, only check those
                if only_allow_rules.len() > 0 {
                    return only_allow_rules
                        .iter()
                        .any(|rule| rule.to_lines.contains(line_next));
                }

                // must not be in not allow rules
                true
            })
            .map(|(line, end_point)| RouteSegment::new(line, end_point))
            .collect()
    }

    pub fn set_fork_choice_point_id(&mut self, point: MapDataPointRef) -> () {
        self.next_fork_choice_point = Some(point);
    }

    pub fn move_forward_to_next_fork(
        &mut self,
    ) -> Result<RouteWalkerMoveResult, RouterWalkerError> {
        loop {
            let point = match self.route_walked.get_segment_last() {
                Some(route_segment) => &route_segment.get_end_point(),
                None => &self.start,
            };
            if *point == self.end {
                return Ok(RouteWalkerMoveResult::Finish);
            }
            self.debug_logger.log(format!(
                "raw choices for {:#?} : {:#?}",
                point,
                self.map_data_graph.get_adjacent(point.clone())
            ));

            let available_segments = self.get_next_segments();

            self.debug_logger.log(format!(
                "processed choices {:#?} : {:#?}",
                point, available_segments
            ));

            if available_segments.get_segment_count() > 1 && self.next_fork_choice_point.is_none() {
                return Ok(RouteWalkerMoveResult::Fork(available_segments));
            }

            let next_segment = if let Some(next_point) = self.next_fork_choice_point.take() {
                if !available_segments.has_segment_with_point(&next_point) {
                    return Err(RouterWalkerError::WrongForkChoice {
                        id: next_point.borrow().id,
                        available_fork_ids: available_segments
                            .get_all_segment_points()
                            .iter()
                            .map(|p| p.borrow().id)
                            .collect(),
                    });
                }
                available_segments.get_segment_from_point(&next_point)
            } else {
                available_segments.get_first_segment()
            };

            let next_segment = match next_segment {
                None => {
                    return Ok(RouteWalkerMoveResult::DeadEnd);
                }
                Some(segment) => segment,
            };

            self.route_walked.add_segment(next_segment.clone());
        }
    }

    pub fn move_backwards_to_prev_fork(&mut self) -> Option<RouteSegmentList> {
        self.next_fork_choice_point = None;
        let current_fork = self.route_walked.remove_last_segment();
        if current_fork.is_none() {
            return None;
        }
        loop {
            let last_segment = self.route_walked.get_segment_last();
            if let Some(last_segment) = last_segment {
                if last_segment.get_end_point().borrow().junction
                    && self.get_next_segments().get_segment_count() > 1
                {
                    break;
                }
            } else {
                break;
            }
            self.route_walked.remove_last_segment();
        }

        if self.route_walked.get_segment_last().is_some() {
            return Some(self.get_next_segments());
        }

        None
    }

    pub fn get_route(&self) -> &Route {
        &self.route_walked
    }
}

#[cfg(test)]
mod tests {
    use core::panic;

    use crate::{
        debug_writer::DebugLoggerVoidSink,
        route::walker::{Route, RouteWalkerMoveResult, RouterWalkerError},
        test_utils::{get_test_map_data_graph, line_is_between_point_ids},
    };

    use super::RouteWalker;

    #[test]
    fn walker_same_start_end() {
        let map_data = get_test_map_data_graph();
        let point1 = map_data.get_point_by_id(&1).unwrap();
        let point2 = map_data.get_point_by_id(&1).unwrap();

        let mut walker = RouteWalker::new(
            &map_data,
            point1.clone(),
            point2.clone(),
            Box::new(DebugLoggerVoidSink::default()),
        );

        assert_eq!(
            walker.move_forward_to_next_fork(),
            Ok(RouteWalkerMoveResult::Finish)
        );
        assert_eq!(walker.get_route().clone(), Route::new());
    }

    #[test]
    fn walker_error_on_wrong_choice() {
        let map_data = get_test_map_data_graph();
        let point1 = map_data.get_point_by_id(&2).unwrap();
        let point2 = map_data.get_point_by_id(&3).unwrap();

        let mut walker = RouteWalker::new(
            &map_data,
            point1.clone(),
            point2.clone(),
            Box::new(DebugLoggerVoidSink::default()),
        );

        let choice = map_data.get_point_by_id(&6).unwrap();
        walker.set_fork_choice_point_id(choice);

        assert_eq!(
            walker.move_forward_to_next_fork(),
            Err(RouterWalkerError::WrongForkChoice {
                id: 6,
                available_fork_ids: vec![1, 3]
            })
        );
        assert_eq!(walker.get_route().clone(), Route::new());
    }

    #[test]
    fn waker_one_step_no_fork() {
        let map_data = get_test_map_data_graph();

        let from_id = 1;
        let to_id = 2;
        let point1 = map_data.get_point_by_id(&1).unwrap();
        let point2 = map_data.get_point_by_id(&2).unwrap();

        let mut walker = RouteWalker::new(
            &map_data,
            point1.clone(),
            point2.clone(),
            Box::new(DebugLoggerVoidSink::default()),
        );
        assert_eq!(
            walker.move_forward_to_next_fork(),
            Ok(RouteWalkerMoveResult::Finish)
        );
        let route = walker.get_route().clone();
        assert_eq!(route.get_segment_count(), 1);
        let el = route.get_segment_by_index(0);
        if let Some(route_segment) = el {
            assert!(line_is_between_point_ids(
                &route_segment.get_line(),
                from_id,
                to_id
            ));
            assert_eq!(route_segment.get_end_point().borrow().id, to_id);
        } else {
            assert!(false)
        }
    }

    #[test]
    fn walker_choose_path() {
        let map_data = get_test_map_data_graph();

        let point1 = map_data.get_point_by_id(&1).unwrap();
        let point2 = map_data.get_point_by_id(&7).unwrap();

        let mut walker = RouteWalker::new(
            &map_data,
            point1.clone(),
            point2.clone(),
            Box::new(DebugLoggerVoidSink::default()),
        );

        let choices = match walker.move_forward_to_next_fork() {
            Err(_) => panic!("Error received from move"),
            Ok(RouteWalkerMoveResult::Fork(c)) => c,
            _ => panic!("did not get choices for routes"),
        };

        assert_eq!(choices.get_segment_count(), 3);

        choices.into_iter().for_each(|route_segment| {
            assert!(
                route_segment.get_end_point().borrow().id == 5
                    || route_segment.get_end_point().borrow().id == 4
                    || route_segment.get_end_point().borrow().id == 6
            );
            assert!(
                line_is_between_point_ids(&route_segment.get_line(), 5, 3)
                    || line_is_between_point_ids(&route_segment.get_line(), 4, 3)
                    || line_is_between_point_ids(&route_segment.get_line(), 6, 3)
            )
        });

        let choice = map_data.get_point_by_id(&6).unwrap();
        walker.set_fork_choice_point_id(choice);

        let choices = match walker.move_forward_to_next_fork() {
            Err(_) => panic!("Error received from move"),
            Ok(RouteWalkerMoveResult::Fork(c)) => c,
            _ => panic!("did not get choices for routes"),
        };
        assert_eq!(choices.get_segment_count(), 2);
        choices.into_iter().for_each(|route_segment| {
            assert!(
                route_segment.get_end_point().borrow().id == 8
                    || route_segment.get_end_point().borrow().id == 7
            );
            assert!(
                line_is_between_point_ids(&route_segment.get_line(), 8, 6)
                    || line_is_between_point_ids(&route_segment.get_line(), 7, 6)
            )
        });
        let choice = map_data.get_point_by_id(&7).unwrap();
        walker.set_fork_choice_point_id(choice);

        assert!(walker.move_forward_to_next_fork() == Ok(RouteWalkerMoveResult::Finish));

        let route = walker.get_route().clone();
        assert_eq!(route.get_segment_count(), 4);

        let el = route.get_segment_by_index(0);
        assert!(el.is_some());
        if let Some(route_segment) = el {
            assert!(line_is_between_point_ids(&route_segment.get_line(), 2, 1));
            assert_eq!(route_segment.get_end_point().borrow().id, 2);
        }

        let el = route.get_segment_by_index(1);
        assert!(el.is_some());
        if let Some(route_segment) = el {
            assert!(line_is_between_point_ids(&route_segment.get_line(), 3, 2));
            assert_eq!(route_segment.get_end_point().borrow().id, 3);
        }

        let el = route.get_segment_by_index(2);
        assert!(el.is_some());
        if let Some(route_segment) = el {
            assert!(line_is_between_point_ids(&route_segment.get_line(), 6, 3));
            assert_eq!(route_segment.get_end_point().borrow().id, 6);
        }
        let el = route.get_segment_by_index(3);
        assert!(el.is_some());
        if let Some(route_segment) = el {
            assert!(line_is_between_point_ids(&route_segment.get_line(), 7, 6));
            assert_eq!(route_segment.get_end_point().borrow().id, 7);
        }
    }

    #[test]
    fn walker_reach_dead_end_walk_back() {
        let map_data = get_test_map_data_graph();

        let point1 = map_data.get_point_by_id(&1).unwrap();
        let point2 = map_data.get_point_by_id(&4).unwrap();

        let mut walker = RouteWalker::new(
            &map_data,
            point1.clone(),
            point2.clone(),
            Box::new(DebugLoggerVoidSink::default()),
        );

        let choices = match walker.move_forward_to_next_fork() {
            Err(_) => panic!("Error received from move"),
            Ok(RouteWalkerMoveResult::Fork(c)) => c,
            _ => panic!("did not get choices for routes"),
        };
        assert_eq!(choices.get_segment_count(), 3);

        choices.into_iter().for_each(|route_segment| {
            assert!(
                route_segment.get_end_point().borrow().id == 5
                    || route_segment.get_end_point().borrow().id == 4
                    || route_segment.get_end_point().borrow().id == 6
            );
            assert!(
                line_is_between_point_ids(&route_segment.get_line(), 5, 3)
                    || line_is_between_point_ids(&route_segment.get_line(), 4, 3)
                    || line_is_between_point_ids(&route_segment.get_line(), 6, 3)
            )
        });

        let choice1 = map_data.get_point_by_id(&5).unwrap();

        walker.set_fork_choice_point_id(choice1);

        assert!(walker.move_forward_to_next_fork() == Ok(RouteWalkerMoveResult::DeadEnd));

        let choices = match walker.move_backwards_to_prev_fork() {
            None => panic!("Expected to be back at point 3 with choices"),
            Some(c) => c,
        };

        choices.into_iter().for_each(|route_segment| {
            assert!(
                route_segment.get_end_point().borrow().id == 5
                    || route_segment.get_end_point().borrow().id == 4
                    || route_segment.get_end_point().borrow().id == 6
            );
            assert!(
                line_is_between_point_ids(&route_segment.get_line(), 5, 3)
                    || line_is_between_point_ids(&route_segment.get_line(), 4, 3)
                    || line_is_between_point_ids(&route_segment.get_line(), 6, 3)
            )
        });

        let choice2 = map_data.get_point_by_id(&4).unwrap();
        walker.set_fork_choice_point_id(choice2);

        assert!(walker.move_forward_to_next_fork() == Ok(RouteWalkerMoveResult::Finish));

        let route = walker.get_route().clone();
        assert_eq!(route.get_segment_count(), 3);

        let el = route.get_segment_by_index(0);
        assert!(el.is_some());
        if let Some(route_segment) = el {
            assert!(line_is_between_point_ids(&route_segment.get_line(), 2, 1));
            assert_eq!(route_segment.get_end_point().borrow().id, 2);
        }

        let el = route.get_segment_by_index(1);
        assert!(el.is_some());
        if let Some(route_segment) = el {
            assert!(line_is_between_point_ids(&route_segment.get_line(), 3, 2));
            assert_eq!(route_segment.get_end_point().borrow().id, 3);
        }

        let el = route.get_segment_by_index(2);
        assert!(el.is_some());
        if let Some(route_segment) = el {
            assert!(line_is_between_point_ids(&route_segment.get_line(), 4, 3));
            assert_eq!(route_segment.get_end_point().borrow().id, 4);
        }
    }
}
