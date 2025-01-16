use std::fmt::Debug;

use crate::map_data::{
    graph::{MapDataGraph, MapDataPointRef},
    rule::MapDataRuleType,
};

use super::route::{segment::Segment, segment_list::SegmentList, Route};

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum WalkerError {
    #[error("Invalid fork choice {id}. Available choices are: {available_fork_ids:?}")]
    WrongForkChoice {
        id: u64,
        available_fork_ids: Vec<u64>,
    },
}

pub struct Walker {
    start: MapDataPointRef,
    route_walked: Route,
    next_fork_choice_point: Option<MapDataPointRef>,
}

#[derive(Debug, PartialEq)]
pub enum WalkerMoveResult {
    Fork(SegmentList),
    DeadEnd,
    Finish,
}

impl Walker {
    pub fn new(start: MapDataPointRef) -> Self {
        Self {
            start: start.clone(),
            route_walked: Route::new(),
            next_fork_choice_point: None,
        }
    }

    pub fn get_last_point(&self) -> &MapDataPointRef {
        let last_element = self.get_route().get_segment_last();
        let last_point = match last_element {
            None => &self.start,
            Some(route_segment) => route_segment.get_end_point(),
        };
        last_point
    }

    fn get_fork_segments_for_point(&self, center_point: &MapDataPointRef) -> SegmentList {
        let center_point_borrowed = center_point.borrow();

        let not_allow_rules = center_point_borrowed
            .rules
            .iter()
            .filter(|rule| rule.rule_type == MapDataRuleType::NotAllowed)
            .collect::<Vec<_>>();
        let segments = MapDataGraph::get().get_adjacent(center_point.clone());
        let segment_list = segments
            .iter()
            .filter_map(|(l, p)| {
                if l.borrow().is_one_way() && &l.borrow().points.1 == center_point {
                    return None;
                }
                if !not_allow_rules.is_empty() {
                    let not_allow_rules_for_segment = not_allow_rules
                        .iter()
                        .filter(|rule| rule.to_lines.contains(l))
                        .collect::<Vec<_>>();
                    let other_segments = segments.iter().filter(|s| &s.0 != l).collect::<Vec<_>>();

                    if not_allow_rules_for_segment.iter().any(|rule| {
                        other_segments
                            .iter()
                            .all(|seg| rule.to_lines.contains(&seg.0))
                    }) {
                        return None;
                    }
                }
                Some(Segment::new(l.clone(), p.clone()))
            })
            .collect::<SegmentList>();

        segment_list
    }

    fn get_fork_segments_for_segment(&self, segment: &Segment) -> SegmentList {
        let center_point = segment.get_end_point();
        let center_line = segment.get_line();

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
                    && rule.from_lines.contains(center_line)
            })
            .collect::<Vec<_>>();

        let not_allow_rules = center_point_borrowed
            .rules
            .iter()
            .filter(|rule| {
                rule.rule_type == MapDataRuleType::NotAllowed
                    && rule.from_lines.contains(center_line)
            })
            .collect::<Vec<_>>();

        MapDataGraph::get()
            .get_adjacent(center_point.clone())
            .into_iter()
            .filter(|(line_next, point_next)| {
                // do not offer the same line as you came from
                if point_next.borrow().id == prev_point.id {
                    return false;
                }

                // exclude if next line is one way and the direction is backwards
                if line_next.borrow().is_one_way() && &line_next.borrow().points.1 == center_point {
                    return false;
                }

                // if no rules exist, don't check anything further
                if center_point.borrow().rules.is_empty() {
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
                if !only_allow_rules.is_empty() {
                    return only_allow_rules
                        .iter()
                        .any(|rule| rule.to_lines.contains(line_next));
                }

                // must not be in not allow rules
                true
            })
            .map(|(line, end_point)| Segment::new(line, end_point))
            .collect()
    }

    pub fn set_fork_choice_point_ref(&mut self, point: MapDataPointRef) {
        self.next_fork_choice_point = Some(point);
    }

    fn get_roundabout_exits(&self, segment: &Segment) -> SegmentList {
        if !segment.get_line().borrow().is_roundabout() {
            return SegmentList::new();
        }

        let mut segments = Vec::new();

        let mut current_segment = segment.clone();

        loop {
            let fork_segments = self.get_fork_segments_for_segment(&current_segment);
            let fork_segments: Vec<_> = fork_segments.into();

            segments.push(
                fork_segments
                    .iter()
                    .filter_map(|f| {
                        if f.get_line().borrow().is_roundabout() {
                            return None;
                        }
                        Some(f.clone())
                    })
                    .collect::<Vec<_>>(),
            );

            current_segment = match fork_segments
                .iter()
                .find(|s| s.get_line().borrow().is_roundabout())
            {
                None => break,
                Some(s) => {
                    if s.get_end_point() == segment.get_end_point() {
                        break;
                    }
                    s.clone()
                }
            };
        }

        SegmentList::from(segments.into_iter().flatten().collect::<Vec<_>>())
    }

    fn move_to_roundabout_exit(&mut self, exit_point: &MapDataPointRef) {
        let last_segment = match self.route_walked.get_segment_last() {
            Some(seg) => {
                if !seg.get_line().borrow().is_roundabout() {
                    return;
                }
                seg.clone()
            }
            None => return,
        };

        // let mut segments = Vec::new();

        let mut current_segment = last_segment.clone();

        loop {
            let fork_segments = self.get_fork_segments_for_segment(&current_segment);
            let fork_segments: Vec<_> = fork_segments.into();

            if fork_segments
                .iter()
                .any(|s| s.get_end_point() == exit_point)
            {
                break;
            }

            current_segment = match fork_segments
                .iter()
                .find(|s| s.get_line().borrow().is_roundabout())
            {
                None => break,
                Some(s) => {
                    if s.get_end_point() == last_segment.get_end_point() {
                        break;
                    }
                    s.clone()
                }
            };

            self.route_walked.add_segment(current_segment.clone());
        }
    }

    pub fn move_forward_to_next_fork<T: Fn(MapDataPointRef) -> bool>(
        &mut self,
        is_finished: T,
    ) -> Result<WalkerMoveResult, WalkerError> {
        loop {
            let point = match self.route_walked.get_segment_last() {
                Some(route_segment) => route_segment.get_end_point(),
                None => &self.start,
            };
            if is_finished(point.clone()) {
                return Ok(WalkerMoveResult::Finish);
            }

            let available_segments = match self.route_walked.get_segment_last() {
                None => self.get_fork_segments_for_point(&self.start),
                Some(segment) => {
                    if segment.get_line().borrow().is_roundabout() {
                        self.get_roundabout_exits(segment)
                    } else {
                        self.get_fork_segments_for_segment(segment)
                    }
                }
            };

            if available_segments.get_segment_count() > 1 && self.next_fork_choice_point.is_none() {
                return Ok(WalkerMoveResult::Fork(available_segments));
            }

            let next_segment = if let Some(next_point) = self.next_fork_choice_point.take() {
                if !available_segments.has_segment_with_point(&next_point) {
                    return Err(WalkerError::WrongForkChoice {
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
                    return Ok(WalkerMoveResult::DeadEnd);
                }
                Some(segment) => segment,
            };

            self.move_to_roundabout_exit(next_segment.get_end_point());

            self.route_walked.add_segment(next_segment.clone());
        }
    }

    pub fn move_backwards_to_prev_fork(&mut self) -> Option<SegmentList> {
        self.next_fork_choice_point = None;
        self.route_walked.remove_last_segment();
        loop {
            let last_segment = self.route_walked.get_segment_last();
            if let Some(last_segment) = last_segment {
                if (last_segment.get_end_point().borrow().is_junction()
                    && self
                        .get_fork_segments_for_segment(last_segment)
                        .get_segment_count()
                        > 1)
                    || (last_segment.get_line().borrow().is_roundabout()
                        && self.get_roundabout_exits(last_segment).get_segment_count() > 1)
                {
                    break;
                }
            } else {
                break;
            }
            self.route_walked.remove_last_segment();
        }

        if let Some(last_segment) = self.route_walked.get_segment_last() {
            return Some(self.get_fork_segments_for_segment(last_segment));
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
    use std::collections::HashMap;

    use rusty_fork::rusty_fork_test;
    use tracing::info;

    use crate::{
        map_data::{
            graph::MapDataGraph,
            osm::{OsmRelation, OsmRelationMember, OsmRelationMemberRole, OsmRelationMemberType},
        },
        router::{
            route::Route,
            walker::{WalkerError, WalkerMoveResult},
        },
        test_utils::{
            graph_from_test_dataset, line_is_between_point_ids, route_matches_ids,
            set_graph_static, test_dataset_1, test_dataset_2, test_dataset_3, OsmTestData,
        },
    };

    use super::Walker;

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_same_start_end() {
            set_graph_static(graph_from_test_dataset(test_dataset_1()));
            let point1 = MapDataGraph::get().test_get_point_ref_by_id(&1).unwrap();
            let point2 = MapDataGraph::get().test_get_point_ref_by_id(&1).unwrap();

            let mut walker = Walker::new(
                point1.clone(),
            );

            assert_eq!(
                walker.move_forward_to_next_fork(|p| p == point2),
                Ok(WalkerMoveResult::Finish)
            );
            assert_eq!(walker.get_route().clone(), Route::new());
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_error_on_wrong_choice() {
            set_graph_static(graph_from_test_dataset(test_dataset_1()));
            let point1 = MapDataGraph::get().test_get_point_ref_by_id(&2).unwrap();
            let point2 = MapDataGraph::get().test_get_point_ref_by_id(&3).unwrap();

            let mut walker = Walker::new(
                point1.clone(),
            );

            let choice = MapDataGraph::get().test_get_point_ref_by_id(&6).unwrap();
            walker.set_fork_choice_point_ref(choice);

            assert_eq!(
                walker.move_forward_to_next_fork(|p| p == point2),
                Err(WalkerError::WrongForkChoice {
                    id: 6,
                    available_fork_ids: vec![1, 3]
                })
            );
            assert_eq!(walker.get_route().clone(), Route::new());
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn waker_one_step_no_fork() {
            set_graph_static(graph_from_test_dataset(test_dataset_1()));

            let from_id = 1;
            let to_id = 2;
            let point1 = MapDataGraph::get().test_get_point_ref_by_id(&1).unwrap();
            let point2 = MapDataGraph::get().test_get_point_ref_by_id(&2).unwrap();

            let mut walker = Walker::new(
                point1.clone(),
            );
            assert_eq!(
                walker.move_forward_to_next_fork(|p| p == point2),
                Ok(WalkerMoveResult::Finish)
            );
            let route = walker.get_route().clone();
            assert_eq!(route.get_segment_count(), 1);
            let el = route.get_segment_by_index(0);
            if let Some(route_segment) = el {
                assert!(line_is_between_point_ids(
                    route_segment.get_line(),
                    from_id,
                    to_id
                ));
                assert_eq!(route_segment.get_end_point().borrow().id, to_id);
            } else {
                assert!(false)
            }
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_choose_path() {
            set_graph_static(graph_from_test_dataset(test_dataset_1()));

            let point1 = MapDataGraph::get().test_get_point_ref_by_id(&1).unwrap();
            let point2 = MapDataGraph::get().test_get_point_ref_by_id(&7).unwrap();

            let mut walker = Walker::new(
                point1.clone(),
            );

            let choices = match walker.move_forward_to_next_fork(|p| p == point2) {
                Err(_) => panic!("Error received from move"),
                Ok(WalkerMoveResult::Fork(c)) => c,
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
                    line_is_between_point_ids(route_segment.get_line(), 5, 3)
                        || line_is_between_point_ids(route_segment.get_line(), 4, 3)
                        || line_is_between_point_ids(route_segment.get_line(), 6, 3)
                )
            });

            let choice = MapDataGraph::get().test_get_point_ref_by_id(&6).unwrap();
            walker.set_fork_choice_point_ref(choice);

            let choices = match walker.move_forward_to_next_fork(|p| p == point2) {
                Err(_) => panic!("Error received from move"),
                Ok(WalkerMoveResult::Fork(c)) => c,
                _ => panic!("did not get choices for routes"),
            };
            assert_eq!(choices.get_segment_count(), 2);
            choices.into_iter().for_each(|route_segment| {
                assert!(
                    route_segment.get_end_point().borrow().id == 8
                        || route_segment.get_end_point().borrow().id == 7
                );
                assert!(
                    line_is_between_point_ids(route_segment.get_line(), 8, 6)
                        || line_is_between_point_ids(route_segment.get_line(), 7, 6)
                )
            });
            let choice = MapDataGraph::get().test_get_point_ref_by_id(&7).unwrap();
            walker.set_fork_choice_point_ref(choice);

            assert!(walker.move_forward_to_next_fork(|p| p == point2) == Ok(WalkerMoveResult::Finish));

            let route = walker.get_route().clone();
            assert_eq!(route.get_segment_count(), 4);

            let el = route.get_segment_by_index(0);
            assert!(el.is_some());
            if let Some(route_segment) = el {
                assert!(line_is_between_point_ids(route_segment.get_line(), 2, 1));
                assert_eq!(route_segment.get_end_point().borrow().id, 2);
            }

            let el = route.get_segment_by_index(1);
            assert!(el.is_some());
            if let Some(route_segment) = el {
                assert!(line_is_between_point_ids(route_segment.get_line(), 3, 2));
                assert_eq!(route_segment.get_end_point().borrow().id, 3);
            }

            let el = route.get_segment_by_index(2);
            assert!(el.is_some());
            if let Some(route_segment) = el {
                assert!(line_is_between_point_ids(route_segment.get_line(), 6, 3));
                assert_eq!(route_segment.get_end_point().borrow().id, 6);
            }
            let el = route.get_segment_by_index(3);
            assert!(el.is_some());
            if let Some(route_segment) = el {
                assert!(line_is_between_point_ids(route_segment.get_line(), 7, 6));
                assert_eq!(route_segment.get_end_point().borrow().id, 7);
            }
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_reach_dead_end_walk_back() {
            set_graph_static(graph_from_test_dataset(test_dataset_1()));

            let point1 = MapDataGraph::get().test_get_point_ref_by_id(&1).unwrap();
            let point2 = MapDataGraph::get().test_get_point_ref_by_id(&4).unwrap();

            let mut walker = Walker::new(
                point1.clone(),
            );

            let choices = match walker.move_forward_to_next_fork(|p| p == point2) {
                Err(_) => panic!("Error received from move"),
                Ok(WalkerMoveResult::Fork(c)) => c,
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
                    line_is_between_point_ids(route_segment.get_line(), 5, 3)
                        || line_is_between_point_ids(route_segment.get_line(), 4, 3)
                        || line_is_between_point_ids(route_segment.get_line(), 6, 3)
                )
            });

            let choice1 = MapDataGraph::get().test_get_point_ref_by_id(&5).unwrap();

            walker.set_fork_choice_point_ref(choice1);

            assert!(walker.move_forward_to_next_fork(|p| p == point2) == Ok(WalkerMoveResult::DeadEnd));

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
                    line_is_between_point_ids(route_segment.get_line(), 5, 3)
                        || line_is_between_point_ids(route_segment.get_line(), 4, 3)
                        || line_is_between_point_ids(route_segment.get_line(), 6, 3)
                )
            });

            let choice2 = MapDataGraph::get().test_get_point_ref_by_id(&4).unwrap();
            walker.set_fork_choice_point_ref(choice2);

            assert!(walker.move_forward_to_next_fork(|p| p == point2) == Ok(WalkerMoveResult::Finish));

            let route = walker.get_route().clone();
            assert_eq!(route.get_segment_count(), 3);

            let el = route.get_segment_by_index(0);
            assert!(el.is_some());
            if let Some(route_segment) = el {
                assert!(line_is_between_point_ids(route_segment.get_line(), 2, 1));
                assert_eq!(route_segment.get_end_point().borrow().id, 2);
            }

            let el = route.get_segment_by_index(1);
            assert!(el.is_some());
            if let Some(route_segment) = el {
                assert!(line_is_between_point_ids(route_segment.get_line(), 3, 2));
                assert_eq!(route_segment.get_end_point().borrow().id, 3);
            }

            let el = route.get_segment_by_index(2);
            assert!(el.is_some());
            if let Some(route_segment) = el {
                assert!(line_is_between_point_ids(route_segment.get_line(), 4, 3));
                assert_eq!(route_segment.get_end_point().borrow().id, 4);
            }
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn handle_roundabout() {
            set_graph_static(
                graph_from_test_dataset(
                    test_dataset_2()
                )
            );

            let start = MapDataGraph::get().test_get_point_ref_by_id(&6).unwrap();
            let finish = MapDataGraph::get().test_get_point_ref_by_id(&131).unwrap();

            let mut walker = Walker::new(
                start.clone(),
            );

            let choices = match walker.move_forward_to_next_fork(|p| p == finish) {
                Err(_) => panic!("Error received from move"),
                Ok(WalkerMoveResult::Fork(c)) => c,
                _ => panic!("did not get choices for routes"),
            };
            assert_eq!(choices.get_segment_count(), 2);

            choices.into_iter().for_each(|route_segment| {
                assert!(
                    route_segment.get_end_point().borrow().id == 2
                        || route_segment.get_end_point().borrow().id == 11
                );
                assert!(
                    line_is_between_point_ids(route_segment.get_line(), 7, 2)
                        || line_is_between_point_ids(route_segment.get_line(), 7, 11)
                )
            });

            let choice = MapDataGraph::get().test_get_point_ref_by_id(&11).unwrap();
            walker.set_fork_choice_point_ref(choice);

            let choices = match walker.move_forward_to_next_fork(|p| p == finish) {
                Err(_) => panic!("Error received from move"),
                Ok(WalkerMoveResult::Fork(c)) => c,
                _ => panic!("did not get choices for routes"),
            };

            assert_eq!(choices.get_segment_count(), 3);

            choices.into_iter().for_each(|route_segment| {
                assert!(
                    route_segment.get_end_point().borrow().id == 111
                        || route_segment.get_end_point().borrow().id == 121
                        || route_segment.get_end_point().borrow().id == 131
                );
                assert!(
                    line_is_between_point_ids(route_segment.get_line(), 11, 111)
                        || line_is_between_point_ids(route_segment.get_line(), 12, 121)
                        || line_is_between_point_ids(route_segment.get_line(), 13, 131)
                )
            });

            let choice = MapDataGraph::get().test_get_point_ref_by_id(&131).unwrap();
            walker.set_fork_choice_point_ref(choice);

            match walker.move_forward_to_next_fork(|p| p == finish) {
                Err(_) => panic!("Error received from move"),
                Ok(WalkerMoveResult::Finish) => {}
                _ => panic!("expected to reach finish"),
            };

            let route = walker.get_route().clone();
            assert!(route_matches_ids(route, vec![7, 11, 12, 13, 131]));
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn follow_one_way() {
            set_graph_static(
                graph_from_test_dataset(
                    test_dataset_2()
                )
            );

            let start = MapDataGraph::get().test_get_point_ref_by_id(&6).unwrap();
            let finish = MapDataGraph::get().test_get_point_ref_by_id(&9).unwrap();

            let mut walker = Walker::new(
                start.clone(),
            );

            let choices = match walker.move_forward_to_next_fork(|p| p == finish) {
                Err(_) => panic!("Error received from move"),
                Ok(WalkerMoveResult::Fork(c)) => c,
                _ => panic!("did not get choices for routes"),
            };
            let next = MapDataGraph::get().test_get_point_ref_by_id(&2).unwrap();
            assert!(choices.get_all_segment_points().contains(&next));

            let wrong_way_point = MapDataGraph::get().test_get_point_ref_by_id(&8).unwrap();
            assert!(!choices.get_all_segment_points().contains(&wrong_way_point));
        }
    }

    fn rule_test(test_data: OsmTestData, can_go_ids: Vec<u64>, cannot_go_ids: Vec<u64>) {
        set_graph_static(graph_from_test_dataset(test_data));

        let start = MapDataGraph::get().test_get_point_ref_by_id(&1).unwrap();
        let finish = MapDataGraph::get().test_get_point_ref_by_id(&7).unwrap();

        let mut walker = Walker::new(start.clone());

        let choices = match walker.move_forward_to_next_fork(|p| p == finish) {
            Err(_) => panic!("Error received from move"),
            Ok(WalkerMoveResult::Fork(c)) => c,
            Ok(v) => {
                if can_go_ids.is_empty() {
                    return;
                }
                panic!("did not get choices: {:#?}", v)
            }
        };

        for can_go_id in can_go_ids {
            let can_go = MapDataGraph::get()
                .test_get_point_ref_by_id(&can_go_id)
                .unwrap();
            info!("go {}", can_go_id);
            assert!(choices.get_all_segment_points().contains(&can_go));
        }

        for cannot_go_id in cannot_go_ids {
            let cannot_go = MapDataGraph::get()
                .test_get_point_ref_by_id(&cannot_go_id)
                .unwrap();
            info!("no go {}", cannot_go_id);
            assert!(!choices.get_all_segment_points().contains(&cannot_go));
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn rule_no_left() {
            let test_data = test_dataset_3();
            let rules: Vec<OsmRelation> = vec![
                OsmRelation {
                    id: 1,
                    members: vec![
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::From,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 3,
                            role: OsmRelationMemberRole::Via,
                            member_type: OsmRelationMemberType::Node
                        },
                        OsmRelationMember {
                            member_ref: 36,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        }
                    ],
                    tags: HashMap::from([
                        ("type".to_string(), "restriction".to_string()),
                        ("restriction".to_string(), "no_left_turn".to_string())
                    ])
                },
            ];

            rule_test(
                (test_data.0.clone(), test_data.1.clone(), rules.clone()),
                vec![5, 4],
                vec![6]
            );
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn rule_no_straight() {
            let test_data = test_dataset_3();
            let rules: Vec<OsmRelation> = vec![
                OsmRelation {
                    id: 1,
                    members: vec![
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::From,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 3,
                            role: OsmRelationMemberRole::Via,
                            member_type: OsmRelationMemberType::Node
                        },
                        OsmRelationMember {
                            member_ref: 34,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        }
                    ],
                    tags: HashMap::from([
                        ("type".to_string(), "restriction".to_string()),
                        ("restriction".to_string(), "no_straight_on".to_string())
                    ])
                },
            ];

            rule_test(
                (test_data.0.clone(), test_data.1.clone(), rules.clone()),
                vec![5,6],
                vec![4]
            );
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn rule_no_right() {
            let test_data = test_dataset_3();
            let rules: Vec<OsmRelation> = vec![
                OsmRelation {
                    id: 1,
                    members: vec![
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::From,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 3,
                            role: OsmRelationMemberRole::Via,
                            member_type: OsmRelationMemberType::Node
                        },
                        OsmRelationMember {
                            member_ref: 53,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        }
                    ],
                    tags: HashMap::from([
                        ("type".to_string(), "restriction".to_string()),
                        ("restriction".to_string(), "no_right_turn".to_string())
                    ])
                },
            ];

            rule_test(
                (test_data.0.clone(), test_data.1.clone(), rules.clone()),
                vec![4, 6],
                vec![5]
            );
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn rule_no_u() {
            let test_data = test_dataset_3();
            let rules: Vec<OsmRelation> = vec![
                OsmRelation {
                    id: 1,
                    members: vec![
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::From,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 3,
                            role: OsmRelationMemberRole::Via,
                            member_type: OsmRelationMemberType::Node
                        },
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        }
                    ],
                    tags: HashMap::from([
                        ("type".to_string(), "restriction".to_string()),
                        ("restriction".to_string(), "no_u_turn".to_string())
                    ])
                },
            ];

            rule_test(
                (test_data.0.clone(), test_data.1.clone(), rules.clone()),
                vec![4, 5, 6],
                vec![]
            );
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn rule_no_entry() {
            let test_data = test_dataset_3();
            let rules: Vec<OsmRelation> = vec![
                OsmRelation {
                    id: 1,
                    members: vec![
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::From,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 3,
                            role: OsmRelationMemberRole::Via,
                            member_type: OsmRelationMemberType::Node
                        },
                        OsmRelationMember {
                            member_ref: 34,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        }
                    ],
                    tags: HashMap::from([
                        ("type".to_string(), "restriction".to_string()),
                        ("restriction".to_string(), "no_entry".to_string())
                    ])
                },
            ];

            rule_test(
                (test_data.0.clone(), test_data.1.clone(), rules.clone()),
                vec![5, 6],
                vec![4]
            );
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn rule_no_exit() {
            let test_data = test_dataset_3();
            let rules: Vec<OsmRelation> = vec![
                OsmRelation {
                    id: 1,
                    members: vec![
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::From,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 3,
                            role: OsmRelationMemberRole::Via,
                            member_type: OsmRelationMemberType::Node
                        },
                        OsmRelationMember {
                            member_ref: 53,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 34,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        }
                    ],
                    tags: HashMap::from([
                        ("type".to_string(), "restriction".to_string()),
                        ("restriction".to_string(), "no_exit".to_string())
                    ])
                },
            ];

            rule_test(
                (test_data.0.clone(), test_data.1.clone(), rules.clone()),
                vec![4, 7], // only one left - 6th exit, it chooses that and stops at the next fork
                vec![]
            );
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn rule_only_right() {
            let test_data = test_dataset_3();
            let rules: Vec<OsmRelation> = vec![
                OsmRelation {
                    id: 1,
                    members: vec![
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::From,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 3,
                            role: OsmRelationMemberRole::Via,
                            member_type: OsmRelationMemberType::Node
                        },
                        OsmRelationMember {
                            member_ref: 53,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        }
                    ],
                    tags: HashMap::from([
                        ("type".to_string(), "restriction".to_string()),
                        ("restriction".to_string(), "only_right_turn".to_string())
                    ])
                },
            ];

            rule_test(
                (test_data.0.clone(), test_data.1.clone(), rules.clone()),
                vec![4, 7], // only 5th exit, goes to next fork and finds 4 and 7
                vec![]
            );
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn rule_only_left() {
            let test_data = test_dataset_3();
            let rules: Vec<OsmRelation> = vec![
                OsmRelation {
                    id: 1,
                    members: vec![
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::From,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 3,
                            role: OsmRelationMemberRole::Via,
                            member_type: OsmRelationMemberType::Node
                        },
                        OsmRelationMember {
                            member_ref: 53,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        }
                    ],
                    tags: HashMap::from([
                        ("type".to_string(), "restriction".to_string()),
                        ("restriction".to_string(), "only_left_turn".to_string())
                    ])
                },
            ];

            rule_test(
                (test_data.0.clone(), test_data.1.clone(), rules.clone()),
                vec![4, 7], // only 6th exit, it continues and find next fork of 4 and 7
                vec![]
            );
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn rule_only_straight() {
            let test_data = test_dataset_3();
            let rules: Vec<OsmRelation> = vec![
                OsmRelation {
                    id: 1,
                    members: vec![
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::From,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 3,
                            role: OsmRelationMemberRole::Via,
                            member_type: OsmRelationMemberType::Node
                        },
                        OsmRelationMember {
                            member_ref: 53,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        }
                    ],
                    tags: HashMap::from([
                        ("type".to_string(), "restriction".to_string()),
                        ("restriction".to_string(), "only_straight_on".to_string())
                    ])
                },
            ];

            rule_test(
                (test_data.0.clone(), test_data.1.clone(), rules.clone()),
                vec![4],
                vec![5, 6]
            );
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn rule_only_u() {
            let test_data = test_dataset_3();
            let rules: Vec<OsmRelation> = vec![
                OsmRelation {
                    id: 1,
                    members: vec![
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::From,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 3,
                            role: OsmRelationMemberRole::Via,
                            member_type: OsmRelationMemberType::Node
                        },
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        }
                    ],
                    tags: HashMap::from([
                        ("type".to_string(), "restriction".to_string()),
                        ("restriction".to_string(), "only_u_turn".to_string())
                    ])
                },
            ];

            rule_test(
                (test_data.0.clone(), test_data.1.clone(), rules.clone()),
                vec![],
                vec![4, 5, 6]
            );
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn rules_only_left_and_no_left() {
            let test_data = test_dataset_3();
            let rules: Vec<OsmRelation> = vec![
                OsmRelation {
                    id: 1,
                    members: vec![
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::From,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 3,
                            role: OsmRelationMemberRole::Via,
                            member_type: OsmRelationMemberType::Node
                        },
                        OsmRelationMember {
                            member_ref: 36,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        }
                    ],
                    tags: HashMap::from([
                        ("type".to_string(), "restriction".to_string()),
                        ("restriction".to_string(), "only_left_turn".to_string())
                    ])
                },
                OsmRelation {
                    id: 1,
                    members: vec![
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::From,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 3,
                            role: OsmRelationMemberRole::Via,
                            member_type: OsmRelationMemberType::Node
                        },
                        OsmRelationMember {
                            member_ref: 36,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        }
                    ],
                    tags: HashMap::from([
                        ("type".to_string(), "restriction".to_string()),
                        ("restriction".to_string(), "no_left_turn".to_string())
                    ])
                },
            ];

            rule_test(
                (test_data.0.clone(), test_data.1.clone(), rules.clone()),
                vec![],
                vec![4, 5, 6]
            );
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn rules_no_left_and_no_right() {
            let test_data = test_dataset_3();
            let rules: Vec<OsmRelation> = vec![
                OsmRelation {
                    id: 1,
                    members: vec![
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::From,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 3,
                            role: OsmRelationMemberRole::Via,
                            member_type: OsmRelationMemberType::Node
                        },
                        OsmRelationMember {
                            member_ref: 53,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        }
                    ],
                    tags: HashMap::from([
                        ("type".to_string(), "restriction".to_string()),
                        ("restriction".to_string(), "no_right_turn".to_string())
                    ])
                },
                OsmRelation {
                    id: 1,
                    members: vec![
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::From,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 3,
                            role: OsmRelationMemberRole::Via,
                            member_type: OsmRelationMemberType::Node
                        },
                        OsmRelationMember {
                            member_ref: 36,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        }
                    ],
                    tags: HashMap::from([
                        ("type".to_string(), "restriction".to_string()),
                        ("restriction".to_string(), "no_left_turn".to_string())
                    ])
                },
            ];

            rule_test(
                (test_data.0.clone(), test_data.1.clone(), rules.clone()),
                vec![5, 6], // 4th is the only exit, moves to next fork and finds 5 and 6
                vec![]
            );
        }
    }
    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn rules_only_left_only_right() {
            let test_data = test_dataset_3();
            let rules: Vec<OsmRelation> = vec![
                OsmRelation {
                    id: 1,
                    members: vec![
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::From,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 3,
                            role: OsmRelationMemberRole::Via,
                            member_type: OsmRelationMemberType::Node
                        },
                        OsmRelationMember {
                            member_ref: 36,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        }
                    ],
                    tags: HashMap::from([
                        ("type".to_string(), "restriction".to_string()),
                        ("restriction".to_string(), "only_left_turn".to_string())
                    ])
                },
                OsmRelation {
                    id: 1,
                    members: vec![
                        OsmRelationMember {
                            member_ref: 13,
                            role: OsmRelationMemberRole::From,
                            member_type: OsmRelationMemberType::Way
                        },
                        OsmRelationMember {
                            member_ref: 3,
                            role: OsmRelationMemberRole::Via,
                            member_type: OsmRelationMemberType::Node
                        },
                        OsmRelationMember {
                            member_ref: 53,
                            role: OsmRelationMemberRole::To,
                            member_type: OsmRelationMemberType::Way
                        }
                    ],
                    tags: HashMap::from([
                        ("type".to_string(), "restriction".to_string()),
                        ("restriction".to_string(), "only_right_turn".to_string())
                    ])
                },
            ];

            rule_test(
                (test_data.0.clone(), test_data.1.clone(), rules.clone()),
                vec![5, 6],
                vec![4]
            );
        }
    }
}
