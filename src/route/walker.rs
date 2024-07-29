use std::{rc::Rc, usize, vec};

use crate::map_data_graph::{
    MapDataGraph, MapDataLine, MapDataLineRef, MapDataPoint, MapDataPointRef,
};

#[derive(Debug, PartialEq)]
pub enum RouterWalkerError {
    WrongForkChoice {
        id: u64,
        available_fork_ids: Vec<u64>,
    },
}

#[derive(Debug, PartialEq, Clone)]
pub struct RouteSegment {
    line: MapDataLineRef,
    end_point: MapDataPointRef,
}

impl RouteSegment {
    pub fn new(line: MapDataLineRef, end_point: MapDataPointRef) -> Self {
        Self { line, end_point }
    }
    pub fn get_end_point(&self) -> &MapDataPointRef {
        &self.end_point
    }
    pub fn get_line(&self) -> &MapDataLineRef {
        &self.line
    }
}

impl From<(MapDataLineRef, MapDataPointRef)> for RouteSegment {
    fn from(value: (MapDataLineRef, MapDataPointRef)) -> Self {
        RouteSegment::new(value.0, value.1)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    route_segments: Vec<RouteSegment>,
}

impl Route {
    pub fn new() -> Self {
        Route {
            route_segments: Vec::new(),
        }
    }
    pub fn get_segment_last(&self) -> Option<&RouteSegment> {
        self.route_segments.last()
    }
    pub fn get_segment_by_index(&self, idx: usize) -> Option<&RouteSegment> {
        self.route_segments.get(idx)
    }
    pub fn get_segment_count(&self) -> usize {
        self.route_segments.len()
    }
    pub fn remove_last_segment(&mut self) -> Option<RouteSegment> {
        self.route_segments.pop()
    }
    pub fn add_segment(&mut self, segment: RouteSegment) -> () {
        self.route_segments.push(segment)
    }
    pub fn get_fork_before_last_segment(&self) -> Option<&RouteSegment> {
        match self.get_segment_last() {
            None => None,
            Some(last_segment) => self.route_segments.iter().rev().find(|route_segment| {
                route_segment.end_point.borrow().fork == true
                    && route_segment.end_point.borrow().id != last_segment.end_point.borrow().id
            }),
        }
    }
    pub fn contains_point_id(&self, id: u64) -> bool {
        self.route_segments
            .iter()
            .find(|segment| segment.end_point.borrow().id == id)
            .is_some()
    }
}

impl From<Vec<RouteSegment>> for Route {
    fn from(route_segments: Vec<RouteSegment>) -> Self {
        Route { route_segments }
    }
}

impl FromIterator<(MapDataLine, MapDataPoint)> for Route {
    fn from_iter<T: IntoIterator<Item = (MapDataLine, MapDataPoint)>>(iter: T) -> Self {
        iter.into_iter().collect::<Route>()
    }
}

impl IntoIterator for Route {
    type Item = RouteSegment;

    type IntoIter = std::vec::IntoIter<RouteSegment>;

    fn into_iter(self) -> Self::IntoIter {
        self.route_segments.into_iter()
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct RouteSegmentList {
    segment_list: Vec<RouteSegment>,
}

impl RouteSegmentList {
    pub fn new() -> Self {
        Self {
            segment_list: Vec::new(),
        }
    }
    pub fn get_segment_count(&self) -> usize {
        self.segment_list.len()
    }
    pub fn has_segment_with_point(&self, point: &MapDataPointRef) -> bool {
        self.segment_list
            .iter()
            .position(|route_segment| route_segment.get_end_point() == point)
            != None
    }
    pub fn get_all_segment_points(&self) -> Vec<MapDataPointRef> {
        self.segment_list
            .iter()
            .map(|segment| Rc::clone(&segment.end_point))
            .collect()
    }
    pub fn get_segment_from_point(&self, point: &MapDataPointRef) -> Option<&RouteSegment> {
        self.segment_list
            .iter()
            .find(|segment| segment.end_point == *point)
    }
    pub fn exclude_segments_where_points_in(
        &self,
        points: &Vec<MapDataPointRef>,
    ) -> RouteSegmentList {
        self.segment_list
            .iter()
            .filter(|segment| !points.contains(&&segment.end_point))
            .collect()
    }
    pub fn get_first_segment(&self) -> Option<&RouteSegment> {
        self.segment_list.get(0)
    }
}

impl IntoIterator for RouteSegmentList {
    type Item = RouteSegment;

    type IntoIter = vec::IntoIter<RouteSegment>;

    fn into_iter(self) -> Self::IntoIter {
        self.segment_list.into_iter()
    }
}

impl From<Vec<RouteSegment>> for RouteSegmentList {
    fn from(value: Vec<RouteSegment>) -> Self {
        Self {
            segment_list: value,
        }
    }
}

impl FromIterator<RouteSegment> for RouteSegmentList {
    fn from_iter<T: IntoIterator<Item = RouteSegment>>(iter: T) -> Self {
        RouteSegmentList {
            segment_list: iter.into_iter().collect(),
        }
    }
}
impl<'a> FromIterator<&'a RouteSegment> for RouteSegmentList {
    fn from_iter<T: IntoIterator<Item = &'a RouteSegment>>(iter: T) -> Self {
        RouteSegmentList {
            segment_list: Vec::from_iter(iter.into_iter().cloned()),
        }
    }
}

pub struct RouteWalker<'a> {
    map_data_graph: &'a MapDataGraph,
    start: MapDataPointRef,
    end: MapDataPointRef,
    route_walked: Route,
    next_fork_choice_point: Option<MapDataPointRef>,
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
    ) -> Self {
        Self {
            map_data_graph,
            start,
            end,
            route_walked: Route::new(),
            next_fork_choice_point: None,
        }
    }

    fn get_available_fork_segments(&self, point: MapDataPointRef) -> RouteSegmentList {
        let prev_point = if let Some(idx) = self.route_walked.get_segment_count().checked_sub(2) {
            if let Some(p) = self.route_walked.get_segment_by_index(idx) {
                &p.get_end_point()
            } else {
                &self.start
            }
        } else {
            &self.start
        };

        self.map_data_graph
            .get_adjacent(point)
            .into_iter()
            .filter(|(_, p)| p.borrow().id != prev_point.borrow().id)
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

            let available_segments = self.get_available_fork_segments(Rc::clone(&point));

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
                None => return Ok(RouteWalkerMoveResult::DeadEnd),
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
                if last_segment.get_end_point().borrow().fork {
                    break;
                }
            }
            self.route_walked.remove_last_segment();
        }

        if let Some(RouteSegment { line: _, end_point }) = self.route_walked.get_segment_last() {
            return Some(self.get_available_fork_segments(Rc::clone(&end_point)));
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
        route::walker::{Route, RouteWalkerMoveResult, RouterWalkerError},
        test_utils::{get_test_map_data_graph, line_is_between_point_ids},
    };

    use super::RouteWalker;

    #[test]
    fn walker_same_start_end() {
        let map_data = get_test_map_data_graph();
        let point1 = map_data.get_point_by_id(&1).unwrap();
        let point2 = map_data.get_point_by_id(&1).unwrap();

        let mut walker = RouteWalker::new(&map_data, point1, point2);

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

        let mut walker = RouteWalker::new(&map_data, point1, point2);

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

        let mut walker = RouteWalker::new(&map_data, point1, point2);
        assert_eq!(
            walker.move_forward_to_next_fork(),
            Ok(RouteWalkerMoveResult::Finish)
        );
        let route = walker.get_route().clone();
        assert_eq!(route.get_segment_count(), 1);
        let el = route.get_segment_by_index(0);
        if let Some(route_segment) = el {
            assert!(line_is_between_point_ids(
                &route_segment.line,
                from_id,
                to_id
            ));
            assert_eq!(route_segment.end_point.borrow().id, to_id);
        } else {
            assert!(false)
        }
    }

    #[test]
    fn walker_choose_path() {
        let map_data = get_test_map_data_graph();

        let point1 = map_data.get_point_by_id(&1).unwrap();
        let point2 = map_data.get_point_by_id(&7).unwrap();

        let mut walker = RouteWalker::new(&map_data, point1, point2);

        let choices = match walker.move_forward_to_next_fork() {
            Err(_) => panic!("Error received from move"),
            Ok(RouteWalkerMoveResult::Fork(c)) => c,
            _ => panic!("did not get choices for routes"),
        };

        assert_eq!(choices.get_segment_count(), 3);

        choices.into_iter().for_each(|route_segment| {
            assert!(
                route_segment.end_point.borrow().id == 5
                    || route_segment.end_point.borrow().id == 4
                    || route_segment.end_point.borrow().id == 6
            );
            assert!(
                line_is_between_point_ids(&route_segment.line, 5, 3)
                    || line_is_between_point_ids(&route_segment.line, 4, 3)
                    || line_is_between_point_ids(&route_segment.line, 6, 3)
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
                route_segment.end_point.borrow().id == 8
                    || route_segment.end_point.borrow().id == 7
            );
            assert!(
                line_is_between_point_ids(&route_segment.line, 8, 6)
                    || line_is_between_point_ids(&route_segment.line, 7, 6)
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
            assert!(line_is_between_point_ids(&route_segment.line, 2, 1));
            assert_eq!(route_segment.end_point.borrow().id, 2);
        }

        let el = route.get_segment_by_index(1);
        assert!(el.is_some());
        if let Some(route_segment) = el {
            assert!(line_is_between_point_ids(&route_segment.line, 3, 2));
            assert_eq!(route_segment.end_point.borrow().id, 3);
        }

        let el = route.get_segment_by_index(2);
        assert!(el.is_some());
        if let Some(route_segment) = el {
            assert!(line_is_between_point_ids(&route_segment.line, 6, 3));
            assert_eq!(route_segment.end_point.borrow().id, 6);
        }
        let el = route.get_segment_by_index(3);
        assert!(el.is_some());
        if let Some(route_segment) = el {
            assert!(line_is_between_point_ids(&route_segment.line, 7, 6));
            assert_eq!(route_segment.end_point.borrow().id, 7);
        }
    }

    #[test]
    fn walker_reach_dead_end_walk_back() {
        let map_data = get_test_map_data_graph();

        let point1 = map_data.get_point_by_id(&1).unwrap();
        let point2 = map_data.get_point_by_id(&4).unwrap();

        let mut walker = RouteWalker::new(&map_data, point1, point2);

        let choices = match walker.move_forward_to_next_fork() {
            Err(_) => panic!("Error received from move"),
            Ok(RouteWalkerMoveResult::Fork(c)) => c,
            _ => panic!("did not get choices for routes"),
        };
        assert_eq!(choices.get_segment_count(), 3);

        choices.into_iter().for_each(|route_segment| {
            assert!(
                route_segment.end_point.borrow().id == 5
                    || route_segment.end_point.borrow().id == 4
                    || route_segment.end_point.borrow().id == 6
            );
            assert!(
                line_is_between_point_ids(&route_segment.line, 5, 3)
                    || line_is_between_point_ids(&route_segment.line, 4, 3)
                    || line_is_between_point_ids(&route_segment.line, 6, 3)
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
                route_segment.end_point.borrow().id == 5
                    || route_segment.end_point.borrow().id == 4
                    || route_segment.end_point.borrow().id == 6
            );
            assert!(
                line_is_between_point_ids(&route_segment.line, 5, 3)
                    || line_is_between_point_ids(&route_segment.line, 4, 3)
                    || line_is_between_point_ids(&route_segment.line, 6, 3)
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
            assert!(line_is_between_point_ids(&route_segment.line, 2, 1));
            assert_eq!(route_segment.end_point.borrow().id, 2);
        }

        let el = route.get_segment_by_index(1);
        assert!(el.is_some());
        if let Some(route_segment) = el {
            assert!(line_is_between_point_ids(&route_segment.line, 3, 2));
            assert_eq!(route_segment.end_point.borrow().id, 3);
        }

        let el = route.get_segment_by_index(2);
        assert!(el.is_some());
        if let Some(route_segment) = el {
            assert!(line_is_between_point_ids(&route_segment.line, 4, 3));
            assert_eq!(route_segment.end_point.borrow().id, 4);
        }
    }
}
