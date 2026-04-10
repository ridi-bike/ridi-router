use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fmt::Debug,
};

use smallvec::SmallVec;

use crate::{
    map_data::{
        graph::{MapDataLineRef, MapDataPointRef},
        rule::{MapDataRule, MapDataRuleType},
    },
    RoutingContext,
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct OrientedSegmentKey {
    line_ref: MapDataLineRef,
    end_point: MapDataPointRef,
}

#[derive(Default)]
struct WalkerCaches {
    fork_classifications: HashMap<OrientedSegmentKey, SmallVec<[Segment; 4]>>,
    roundabout_exits: HashMap<OrientedSegmentKey, SmallVec<[Segment; 4]>>,
}

pub struct Walker {
    start: MapDataPointRef,
    route_walked: Route,
    next_fork_choice_point: Option<MapDataPointRef>,
    caches: RefCell<WalkerCaches>,
}

#[derive(Debug, PartialEq)]
pub enum WalkerMoveResult {
    Fork(SegmentList),
    DeadEnd,
    Finish,
}

#[derive(Debug, Clone, PartialEq)]
enum NextStepClass {
    None,
    One(Segment),
    Many(SmallVec<[Segment; 4]>),
}

impl NextStepClass {
    fn push(&mut self, segment: Segment) {
        match self {
            NextStepClass::None => *self = NextStepClass::One(segment),
            NextStepClass::One(_) => {
                let first = match std::mem::replace(self, NextStepClass::None) {
                    NextStepClass::One(first) => first,
                    _ => unreachable!("only One can reach this branch"),
                };
                let mut many = SmallVec::<[Segment; 4]>::new();
                many.push(first);
                many.push(segment);
                *self = NextStepClass::Many(many);
            }
            NextStepClass::Many(many) => many.push(segment),
        }
    }



    #[cfg(test)]
    fn into_segment_list(self) -> SegmentList {
        match self {
            NextStepClass::None => SegmentList::new(),
            NextStepClass::One(segment) => SegmentList::from(vec![segment]),
            NextStepClass::Many(segments) => SegmentList::from(segments.into_vec()),
        }
    }

    fn into_available_point_ids(self, ctx: &RoutingContext<'_>) -> Vec<u64> {
        match self {
            NextStepClass::None => Vec::new(),
            NextStepClass::One(segment) => vec![ctx.point_id(segment.get_end_point())],
            NextStepClass::Many(segments) => segments
                .into_iter()
                .map(|segment| ctx.point_id(segment.get_end_point()))
                .collect(),
        }
    }
}

#[hotpath::measure_all]
impl Walker {
    pub fn new(start: MapDataPointRef) -> Self {
        Self {
            start: start.clone(),
            route_walked: Route::new(),
            next_fork_choice_point: None,
            caches: RefCell::new(WalkerCaches::default()),
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

    fn start_point_segment_blocked_by_not_allowed_rule(
        rules: &[MapDataRule],
        segments: &[(MapDataLineRef, MapDataPointRef)],
        candidate_line_ref: &MapDataLineRef,
    ) -> bool {
        rules
            .iter()
            .filter(|rule| {
                rule.rule_type == MapDataRuleType::NotAllowed
                    && rule.to_lines.contains(candidate_line_ref)
            })
            .any(|rule| {
                segments
                    .iter()
                    .filter(|(other_line_ref, _)| other_line_ref != candidate_line_ref)
                    .all(|(other_line_ref, _)| rule.to_lines.contains(other_line_ref))
            })
    }

    fn continuation_allowed_by_rules(
        rules: &[MapDataRule],
        center_line: &MapDataLineRef,
        line_next_ref: &MapDataLineRef,
    ) -> bool {
        if rules.is_empty() {
            return true;
        }

        let mut has_only_allowed_rule = false;
        let mut allowed_by_only_allowed_rule = false;

        for rule in rules {
            if !rule.from_lines.contains(center_line) {
                continue;
            }

            match rule.rule_type {
                MapDataRuleType::NotAllowed => {
                    if rule.to_lines.contains(line_next_ref) {
                        return false;
                    }
                }
                MapDataRuleType::OnlyAllowed => {
                    has_only_allowed_rule = true;
                    if rule.to_lines.contains(line_next_ref) {
                        allowed_by_only_allowed_rule = true;
                    }
                }
            }
        }

        !has_only_allowed_rule || allowed_by_only_allowed_rule
    }

    fn classify_segments_for_point_with_context(
        &self,
        ctx: &RoutingContext<'_>,
        center_point: &MapDataPointRef,
    ) -> NextStepClass {
        ctx.with_point_and_adjacent_lines(center_point, |center_point_data, segments, lines| {
            let rules = center_point_data.rules.as_slice();
            let mut next_steps = NextStepClass::None;

            for (line_ref, point_ref) in segments {
                let line = lines
                    .get(line_ref)
                    .expect("adjacent line should exist in cache after hydration");
                if line.is_one_way() && &line.points.1 == center_point {
                    continue;
                }

                if Self::start_point_segment_blocked_by_not_allowed_rule(
                    rules, segments, line_ref,
                ) {
                    continue;
                }

                next_steps.push(Segment::new(line_ref.clone(), point_ref.clone()));
            }

            next_steps
        })
    }

    fn oriented_segment_key(segment: &Segment) -> OrientedSegmentKey {
        OrientedSegmentKey {
            line_ref: segment.get_line().clone(),
            end_point: segment.get_end_point().clone(),
        }
    }

    fn incoming_point_for_segment_with_context(
        &self,
        ctx: &RoutingContext<'_>,
        segment: &Segment,
    ) -> Option<MapDataPointRef> {
        ctx.with_line(segment.get_line(), |line| {
            if line.points.0 == *segment.get_end_point() {
                Some(line.points.1.clone())
            } else if line.points.1 == *segment.get_end_point() {
                Some(line.points.0.clone())
            } else {
                None
            }
        })
    }

    fn previous_point_id_from_route_history(&self, ctx: &RoutingContext<'_>) -> u64 {
        if let Some(idx) = self.route_walked.get_segment_count().checked_sub(2) {
            self.route_walked
                .get_segment_by_index(idx)
                .map(|segment| ctx.point_id(segment.get_end_point()))
                .unwrap_or_else(|| ctx.point_id(&self.start))
        } else {
            ctx.point_id(&self.start)
        }
    }

    fn previous_point_id_for_segment_with_context(
        &self,
        ctx: &RoutingContext<'_>,
        segment: &Segment,
    ) -> u64 {
        self.incoming_point_for_segment_with_context(ctx, segment)
            .map(|point| ctx.point_id(&point))
            .unwrap_or_else(|| self.previous_point_id_from_route_history(ctx))
    }

    fn with_fork_segments_for_segment_with_context<R>(
        &self,
        ctx: &RoutingContext<'_>,
        segment: &Segment,
        f: impl FnOnce(&[Segment]) -> R,
    ) -> R {
        let cache_key = Self::oriented_segment_key(segment);
        if let Some(cached) = self.caches.borrow().fork_classifications.get(&cache_key) {
            return f(cached.as_slice());
        }

        let center_point = segment.get_end_point();
        let center_line = segment.get_line();
        let prev_point_id = self.previous_point_id_for_segment_with_context(ctx, segment);

        let next_steps = ctx.with_point_and_adjacent_lines(
            center_point,
            |center_point_data, adjacent, lines| {
                let rules = center_point_data.rules.as_slice();
                let mut next_steps = SmallVec::<[Segment; 4]>::new();

                for (line_next_ref, point_next_ref) in adjacent {
                    if ctx.point_id(point_next_ref) == prev_point_id {
                        continue;
                    }

                    let line_next = lines
                        .get(line_next_ref)
                        .expect("adjacent line should exist in cache after hydration");
                    if line_next.is_one_way() && &line_next.points.1 == center_point {
                        continue;
                    }

                    if !Self::continuation_allowed_by_rules(rules, center_line, line_next_ref) {
                        continue;
                    }

                    next_steps.push(Segment::new(
                        line_next_ref.clone(),
                        point_next_ref.clone(),
                    ));
                }

                next_steps
            },
        );

        self.caches
            .borrow_mut()
            .fork_classifications
            .insert(cache_key.clone(), next_steps);

        let caches = self.caches.borrow();
        let cached = caches
            .fork_classifications
            .get(&cache_key)
            .expect("fork classification should exist in cache after hydration");
        f(cached.as_slice())
    }

    #[cfg(test)]
    fn classify_fork_segments_for_segment_with_context(
        &self,
        ctx: &RoutingContext<'_>,
        segment: &Segment,
    ) -> NextStepClass {
        self.with_fork_segments_for_segment_with_context(ctx, segment, |segments| {
            match segments {
                [] => NextStepClass::None,
                [segment] => NextStepClass::One(segment.clone()),
                _ => NextStepClass::Many(segments.iter().cloned().collect()),
            }
        })
    }

    fn get_fork_segments_for_segment_with_context(
        &self,
        ctx: &RoutingContext<'_>,
        segment: &Segment,
    ) -> SegmentList {
        self.with_fork_segments_for_segment_with_context(ctx, segment, |segments| {
            SegmentList::from_iter(segments.iter())
        })
    }

    pub fn set_fork_choice_point_ref(&mut self, point: MapDataPointRef) {
        self.next_fork_choice_point = Some(point);
    }

    fn with_roundabout_exits_for_segment_with_context<R>(
        &self,
        ctx: &RoutingContext<'_>,
        segment: &Segment,
        f: impl FnOnce(&[Segment]) -> R,
    ) -> R {
        if !ctx.with_line(segment.get_line(), |line| line.is_roundabout()) {
            return f(&[]);
        }

        let cache_key = Self::oriented_segment_key(segment);
        if let Some(cached) = self.caches.borrow().roundabout_exits.get(&cache_key) {
            return f(cached.as_slice());
        }

        let mut visited_points: HashSet<MapDataPointRef> = HashSet::new();
        let mut exits = SmallVec::<[Segment; 4]>::new();
        let mut current_segment = segment.clone();
        let roundabout_entry_exit_point = self
            .route_walked
            .get_segment_last()
            .filter(|last_segment| *last_segment == segment)
            .and_then(|_| self.route_walked.get_segment_count().checked_sub(2))
            .and_then(|idx| self.route_walked.get_segment_by_index(idx))
            .and_then(|entry_segment| {
                self.incoming_point_for_segment_with_context(ctx, entry_segment)
            });

        loop {
            let mut next_roundabout_segment = None;

            self.with_fork_segments_for_segment_with_context(
                ctx,
                &current_segment,
                |fork_segments| {
                    for fork_segment in fork_segments {
                        if ctx.with_line(fork_segment.get_line(), |line| line.is_roundabout()) {
                            next_roundabout_segment.get_or_insert_with(|| fork_segment.clone());
                        } else if roundabout_entry_exit_point.as_ref()
                            != Some(fork_segment.get_end_point())
                        {
                            exits.push(fork_segment.clone());
                        }
                    }
                },
            );

            current_segment = match next_roundabout_segment {
                None => break,
                Some(roundabout_segment) => {
                    if roundabout_segment.get_end_point() == segment.get_end_point() {
                        break;
                    }
                    roundabout_segment
                }
            };
            if !visited_points.insert(current_segment.get_end_point().clone()) {
                break;
            }
        }

        self.caches
            .borrow_mut()
            .roundabout_exits
            .insert(cache_key.clone(), exits);

        let caches = self.caches.borrow();
        let cached = caches
            .roundabout_exits
            .get(&cache_key)
            .expect("roundabout exits should exist in cache after hydration");
        f(cached.as_slice())
    }

    #[cfg(test)]
    fn get_roundabout_exits_with_context(
        &self,
        ctx: &RoutingContext<'_>,
        segment: &Segment,
    ) -> SegmentList {
        self.with_roundabout_exits_for_segment_with_context(ctx, segment, |segments| {
            SegmentList::from_iter(segments.iter())
        })
    }

    fn move_to_roundabout_exit_with_context(
        &mut self,
        ctx: &RoutingContext<'_>,
        exit_point: &MapDataPointRef,
    ) {
        let mut visited_points: HashSet<MapDataPointRef> = HashSet::new();

        let last_segment = match self.route_walked.get_segment_last() {
            Some(segment) => {
                if !ctx.with_line(segment.get_line(), |line| line.is_roundabout()) {
                    return;
                }
                segment.clone()
            }
            None => return,
        };

        let mut current_segment = last_segment.clone();

        loop {
            let last_point = if let Some(last_segment) = self.route_walked.get_segment_last() {
                last_segment.get_end_point().clone()
            } else {
                self.start.clone()
            };
            if !visited_points.insert(last_point) {
                break;
            }

            let mut next_roundabout_segment = None;
            let mut found_exit = false;
            self.with_fork_segments_for_segment_with_context(
                ctx,
                &current_segment,
                |segments| {
                    for segment in segments {
                        if segment.get_end_point() == exit_point {
                            found_exit = true;
                            break;
                        }

                        if ctx.with_line(segment.get_line(), |line| line.is_roundabout()) {
                            next_roundabout_segment.get_or_insert_with(|| segment.clone());
                        }
                    }
                },
            );

            if found_exit {
                break;
            }

            current_segment = match next_roundabout_segment {
                None => break,
                Some(segment) => {
                    if segment.get_end_point() == last_segment.get_end_point() {
                        break;
                    }
                    segment
                }
            };

            self.route_walked.add_segment(ctx, current_segment.clone());
        }
    }

    pub(crate) fn move_forward_to_next_fork_with_context<T: Fn(MapDataPointRef) -> bool>(
        &mut self,
        ctx: &RoutingContext<'_>,
        is_finished: T,
    ) -> Result<WalkerMoveResult, WalkerError> {
        let mut visited_junction: HashSet<MapDataPointRef> = HashSet::new();
        loop {
            let point = match self.route_walked.get_segment_last() {
                Some(route_segment) => route_segment.get_end_point(),
                None => &self.start,
            };
            if is_finished(point.clone()) {
                return Ok(WalkerMoveResult::Finish);
            }

            let next_segment = match self.route_walked.get_segment_last() {
                None => {
                    let available_segments =
                        self.classify_segments_for_point_with_context(ctx, &self.start);
                    match (available_segments, self.next_fork_choice_point.take()) {
                        (NextStepClass::None, None) => None,
                        (NextStepClass::None, Some(next_point)) => {
                            return Err(WalkerError::WrongForkChoice {
                                id: ctx.point_id(&next_point),
                                available_fork_ids: Vec::new(),
                            });
                        }
                        (NextStepClass::One(segment), None) => Some(segment),
                        (NextStepClass::One(segment), Some(next_point)) => {
                            if segment.get_end_point() != &next_point {
                                return Err(WalkerError::WrongForkChoice {
                                    id: ctx.point_id(&next_point),
                                    available_fork_ids: vec![ctx.point_id(segment.get_end_point())],
                                });
                            }
                            Some(segment)
                        }
                        (NextStepClass::Many(segments), None) => {
                            return Ok(WalkerMoveResult::Fork(SegmentList::from(
                                segments.into_vec(),
                            )));
                        }
                        (NextStepClass::Many(mut segments), Some(next_point)) => {
                            if let Some(index) = segments
                                .iter()
                                .position(|segment| segment.get_end_point() == &next_point)
                            {
                                Some(segments.remove(index))
                            } else {
                                return Err(WalkerError::WrongForkChoice {
                                    id: ctx.point_id(&next_point),
                                    available_fork_ids: NextStepClass::Many(segments)
                                        .into_available_point_ids(ctx),
                                });
                            }
                        }
                    }
                }
                Some(segment) => {
                    if ctx.with_line(segment.get_line(), |line| line.is_roundabout()) {
                        let next_fork_choice_point = self.next_fork_choice_point.take();
                        let (next_segment, fork_choices) = self
                            .with_roundabout_exits_for_segment_with_context(
                                ctx,
                                segment,
                                |available_segments| {
                                    if available_segments.len() > 1
                                        && next_fork_choice_point.is_none()
                                    {
                                        return Ok((
                                            None,
                                            Some(SegmentList::from_iter(
                                                available_segments.iter(),
                                            )),
                                        ));
                                    }

                                    if let Some(next_point) = next_fork_choice_point {
                                        if let Some(segment) = available_segments
                                            .iter()
                                            .find(|segment| segment.get_end_point() == &next_point)
                                        {
                                            return Ok((Some(segment.clone()), None));
                                        }

                                        return Err(WalkerError::WrongForkChoice {
                                            id: ctx.point_id(&next_point),
                                            available_fork_ids: available_segments
                                                .iter()
                                                .map(|segment| {
                                                    ctx.point_id(segment.get_end_point())
                                                })
                                                .collect(),
                                        });
                                    }

                                    Ok((available_segments.first().cloned(), None))
                                },
                            )?;
                        if let Some(fork_choices) = fork_choices {
                            return Ok(WalkerMoveResult::Fork(fork_choices));
                        }
                        next_segment
                    } else {
                        let next_fork_choice_point = self.next_fork_choice_point.take();
                        let (next_segment, fork_choices) = self
                            .with_fork_segments_for_segment_with_context(
                                ctx,
                                segment,
                                |available_segments| match (
                                    available_segments,
                                    next_fork_choice_point,
                                ) {
                                    ([], None) => Ok((None, None)),
                                    ([], Some(next_point)) => Err(WalkerError::WrongForkChoice {
                                        id: ctx.point_id(&next_point),
                                        available_fork_ids: Vec::new(),
                                    }),
                                    ([segment], None) => Ok((Some(segment.clone()), None)),
                                    ([segment], Some(next_point)) => {
                                        if segment.get_end_point() != &next_point {
                                            return Err(WalkerError::WrongForkChoice {
                                                id: ctx.point_id(&next_point),
                                                available_fork_ids: vec![ctx.point_id(
                                                    segment.get_end_point(),
                                                )],
                                            });
                                        }
                                        Ok((Some(segment.clone()), None))
                                    }
                                    (segments, None) => Ok((
                                        None,
                                        Some(SegmentList::from_iter(segments.iter())),
                                    )),
                                    (segments, Some(next_point)) => {
                                        if let Some(segment) = segments
                                            .iter()
                                            .find(|segment| segment.get_end_point() == &next_point)
                                        {
                                            Ok((Some(segment.clone()), None))
                                        } else {
                                            Err(WalkerError::WrongForkChoice {
                                                id: ctx.point_id(&next_point),
                                                available_fork_ids: segments
                                                    .iter()
                                                    .map(|segment| {
                                                        ctx.point_id(segment.get_end_point())
                                                    })
                                                    .collect(),
                                            })
                                        }
                                    }
                                },
                            )?;
                        if let Some(fork_choices) = fork_choices {
                            return Ok(WalkerMoveResult::Fork(fork_choices));
                        }
                        next_segment
                    }
                }
            };

            let next_segment = match next_segment {
                None => {
                    return Ok(WalkerMoveResult::DeadEnd);
                }
                Some(segment) => segment,
            };

            if ctx.point_is_junction(next_segment.get_end_point()) {
                if visited_junction.contains(next_segment.get_end_point()) {
                    return Ok(WalkerMoveResult::DeadEnd);
                }
                visited_junction.insert(next_segment.get_end_point().clone());
            }

            self.move_to_roundabout_exit_with_context(ctx, next_segment.get_end_point());
            self.route_walked.add_segment(ctx, next_segment.clone());
        }
    }

    pub(crate) fn move_backwards_to_prev_fork_with_context(
        &mut self,
        ctx: &RoutingContext<'_>,
    ) -> Option<SegmentList> {
        self.next_fork_choice_point = None;
        self.route_walked.remove_last_segment();
        loop {
            let last_segment = self.route_walked.get_segment_last();
            if let Some(last_segment) = last_segment {
                let is_junction_fork = ctx.point_is_junction(last_segment.get_end_point())
                    && self.with_fork_segments_for_segment_with_context(
                        ctx,
                        last_segment,
                        |segments| segments.len() > 1,
                    );
                let is_roundabout_fork = ctx
                    .with_line(last_segment.get_line(), |line| line.is_roundabout())
                    && self.with_roundabout_exits_for_segment_with_context(
                        ctx,
                        last_segment,
                        |segments| segments.len() > 1,
                    );

                if is_junction_fork || is_roundabout_fork {
                    break;
                }
            } else {
                break;
            }
            self.route_walked.remove_last_segment();
        }

        if let Some(last_segment) = self.route_walked.get_segment_last() {
            return Some(self.get_fork_segments_for_segment_with_context(ctx, last_segment));
        }

        None
    }

    pub fn get_route(&self) -> &Route {
        &self.route_walked
    }

    #[cfg(test)]
    fn fork_classification_cache_size(&self) -> usize {
        self.caches.borrow().fork_classifications.len()
    }

    #[cfg(test)]
    fn roundabout_exit_cache_size(&self) -> usize {
        self.caches.borrow().roundabout_exits.len()
    }
}

#[cfg(test)]
mod tests {
    use core::panic;
    use std::collections::HashMap;

    use crate::{
        router::{
            route::Route,
            walker::{WalkerError, WalkerMoveResult},
        },
        test_utils::{
            graph_from_test_dataset, line_is_between_point_ids, route_matches_ids, test_dataset_1,
            test_dataset_2, test_dataset_3, OsmTestData,
        },
        RoutingContext,
    };
    use ridi_router_common::osm::{
        OsmNode, OsmRelation, OsmRelationMember, OsmRelationMemberRole, OsmRelationMemberType,
        OsmWay,
    };
    use rusty_fork::rusty_fork_test;
    use tracing::info;

    use super::Walker;
    use crate::router::route::segment::Segment;

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_same_start_end() {
            let graph = graph_from_test_dataset(test_dataset_1());
        let ctx = RoutingContext::new(&graph);
            let point1 = graph.test_get_point_ref_by_id(&1).unwrap();
            let point2 = graph.test_get_point_ref_by_id(&1).unwrap();

            let mut walker = Walker::new(
                point1.clone(),
            );

            assert_eq!(
                walker.move_forward_to_next_fork_with_context(&ctx, |p| p == point2),
                Ok(WalkerMoveResult::Finish)
            );
            assert_eq!(walker.get_route().clone(), Route::new());
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_error_on_wrong_choice() {
            let graph = graph_from_test_dataset(test_dataset_1());
        let ctx = RoutingContext::new(&graph);
            let point1 = graph.test_get_point_ref_by_id(&2).unwrap();
            let point2 = graph.test_get_point_ref_by_id(&3).unwrap();

            let mut walker = Walker::new(
                point1.clone(),
            );

            let choice = graph.test_get_point_ref_by_id(&6).unwrap();
            walker.set_fork_choice_point_ref(choice);

            assert_eq!(
                walker.move_forward_to_next_fork_with_context(&ctx, |p| p == point2),
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
            let graph = graph_from_test_dataset(test_dataset_1());
        let ctx = RoutingContext::new(&graph);

            let from_id = 1;
            let to_id = 2;
            let point1 = graph.test_get_point_ref_by_id(&1).unwrap();
            let point2 = graph.test_get_point_ref_by_id(&2).unwrap();

            let mut walker = Walker::new(
                point1.clone(),
            );
            assert_eq!(
                walker.move_forward_to_next_fork_with_context(&ctx, |p| p == point2),
                Ok(WalkerMoveResult::Finish)
            );
            let route = walker.get_route().clone();
            assert_eq!(route.get_segment_count(), 1);
            let el = route.get_segment_by_index(0);
            if let Some(route_segment) = el {
                assert!(line_is_between_point_ids(&ctx,
                    route_segment.get_line(),
                    from_id,
                    to_id
                ));
                assert_eq!(ctx.point(route_segment.get_end_point()).id, to_id);
            } else {
                assert!(false)
            }
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn classify_fork_segments_matches_existing_fork_choices() {
            let graph = graph_from_test_dataset(test_dataset_1());
            let ctx = RoutingContext::new(&graph);

            let point1 = graph.test_get_point_ref_by_id(&1).unwrap();
            let point3 = graph.test_get_point_ref_by_id(&3).unwrap();

            let mut walker = Walker::new(point1);
            assert_eq!(
                walker.move_forward_to_next_fork_with_context(&ctx, |p| p == point3),
                Ok(WalkerMoveResult::Finish)
            );

            let last_segment = walker.get_route().get_segment_last().cloned().unwrap();
            let classified = walker
                .classify_fork_segments_for_segment_with_context(&ctx, &last_segment)
                .into_segment_list();

            assert_eq!(classified.get_segment_count(), 3);
            classified.into_iter().for_each(|route_segment| {
                let end_point_id = ctx.point(route_segment.get_end_point()).id;
                assert!(end_point_id == 4 || end_point_id == 5 || end_point_id == 6);
                assert!(
                    line_is_between_point_ids(&ctx, route_segment.get_line(), 3, end_point_id)
                        || line_is_between_point_ids(
                            &ctx,
                            route_segment.get_line(),
                            end_point_id,
                            3,
                        )
                );
            });
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_choose_path() {
            let graph = graph_from_test_dataset(test_dataset_1());
        let ctx = RoutingContext::new(&graph);

            let point1 = graph.test_get_point_ref_by_id(&1).unwrap();
            let point2 = graph.test_get_point_ref_by_id(&7).unwrap();

            let mut walker = Walker::new(
                point1.clone(),
            );

            let choices = match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == point2) {
                Err(_) => panic!("Error received from move"),
                Ok(WalkerMoveResult::Fork(c)) => c,
                _ => panic!("did not get choices for routes"),
            };

            assert_eq!(choices.get_segment_count(), 3);

            choices.into_iter().for_each(|route_segment| {
                assert!(
                    ctx.point(route_segment.get_end_point()).id == 5
                        || ctx.point(route_segment.get_end_point()).id == 4
                        || ctx.point(route_segment.get_end_point()).id == 6
                );
                assert!(
                    line_is_between_point_ids(&ctx, route_segment.get_line(), 5, 3)
                        || line_is_between_point_ids(&ctx, route_segment.get_line(), 4, 3)
                        || line_is_between_point_ids(&ctx, route_segment.get_line(), 6, 3)
                )
            });

            let choice = graph.test_get_point_ref_by_id(&6).unwrap();
            walker.set_fork_choice_point_ref(choice);

            let choices = match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == point2) {
                Err(_) => panic!("Error received from move"),
                Ok(WalkerMoveResult::Fork(c)) => c,
                _ => panic!("did not get choices for routes"),
            };
            assert_eq!(choices.get_segment_count(), 2);
            choices.into_iter().for_each(|route_segment| {
                assert!(
                    ctx.point(route_segment.get_end_point()).id == 8
                        || ctx.point(route_segment.get_end_point()).id == 7
                );
                assert!(
                    line_is_between_point_ids(&ctx, route_segment.get_line(), 8, 6)
                        || line_is_between_point_ids(&ctx, route_segment.get_line(), 7, 6)
                )
            });
            let choice = graph.test_get_point_ref_by_id(&7).unwrap();
            walker.set_fork_choice_point_ref(choice);

            assert!(walker.move_forward_to_next_fork_with_context(&ctx, |p| p == point2) == Ok(WalkerMoveResult::Finish));

            let route = walker.get_route().clone();
            assert_eq!(route.get_segment_count(), 4);

            let el = route.get_segment_by_index(0);
            assert!(el.is_some());
            if let Some(route_segment) = el {
                assert!(line_is_between_point_ids(&ctx, route_segment.get_line(), 2, 1));
                assert_eq!(ctx.point(route_segment.get_end_point()).id, 2);
            }

            let el = route.get_segment_by_index(1);
            assert!(el.is_some());
            if let Some(route_segment) = el {
                assert!(line_is_between_point_ids(&ctx, route_segment.get_line(), 3, 2));
                assert_eq!(ctx.point(route_segment.get_end_point()).id, 3);
            }

            let el = route.get_segment_by_index(2);
            assert!(el.is_some());
            if let Some(route_segment) = el {
                assert!(line_is_between_point_ids(&ctx, route_segment.get_line(), 6, 3));
                assert_eq!(ctx.point(route_segment.get_end_point()).id, 6);
            }
            let el = route.get_segment_by_index(3);
            assert!(el.is_some());
            if let Some(route_segment) = el {
                assert!(line_is_between_point_ids(&ctx, route_segment.get_line(), 7, 6));
                assert_eq!(ctx.point(route_segment.get_end_point()).id, 7);
            }
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_reach_dead_end_walk_back() {
            let graph = graph_from_test_dataset(test_dataset_1());
        let ctx = RoutingContext::new(&graph);

            let point1 = graph.test_get_point_ref_by_id(&1).unwrap();
            let point2 = graph.test_get_point_ref_by_id(&4).unwrap();

            let mut walker = Walker::new(
                point1.clone(),
            );

            let choices = match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == point2) {
                Err(_) => panic!("Error received from move"),
                Ok(WalkerMoveResult::Fork(c)) => c,
                _ => panic!("did not get choices for routes"),
            };
            assert_eq!(choices.get_segment_count(), 3);

            choices.into_iter().for_each(|route_segment| {
                assert!(
                    ctx.point(route_segment.get_end_point()).id == 5
                        || ctx.point(route_segment.get_end_point()).id == 4
                        || ctx.point(route_segment.get_end_point()).id == 6
                );
                assert!(
                    line_is_between_point_ids(&ctx, route_segment.get_line(), 5, 3)
                        || line_is_between_point_ids(&ctx, route_segment.get_line(), 4, 3)
                        || line_is_between_point_ids(&ctx, route_segment.get_line(), 6, 3)
                )
            });

            let choice1 = graph.test_get_point_ref_by_id(&5).unwrap();

            walker.set_fork_choice_point_ref(choice1);

            assert!(
                walker.move_forward_to_next_fork_with_context(&ctx, |p| p == point2)
                    == Ok(WalkerMoveResult::DeadEnd)
            );
            assert!(!walker.get_route().has_looped(&ctx, None));

            let choices = match walker.move_backwards_to_prev_fork_with_context(&ctx) {
                None => panic!("Expected to be back at point 3 with choices"),
                Some(c) => c,
            };
            assert!(!walker.get_route().has_looped(&ctx, None));

            choices.into_iter().for_each(|route_segment| {
                assert!(
                    ctx.point(route_segment.get_end_point()).id == 5
                        || ctx.point(route_segment.get_end_point()).id == 4
                        || ctx.point(route_segment.get_end_point()).id == 6
                );
                assert!(
                    line_is_between_point_ids(&ctx, route_segment.get_line(), 5, 3)
                        || line_is_between_point_ids(&ctx, route_segment.get_line(), 4, 3)
                        || line_is_between_point_ids(&ctx, route_segment.get_line(), 6, 3)
                )
            });

            let choice2 = graph.test_get_point_ref_by_id(&4).unwrap();
            walker.set_fork_choice_point_ref(choice2);

            assert!(
                walker.move_forward_to_next_fork_with_context(&ctx, |p| p == point2)
                    == Ok(WalkerMoveResult::Finish)
            );
            assert!(!walker.get_route().has_looped(&ctx, None));

            let route = walker.get_route().clone();
            assert_eq!(route.get_segment_count(), 3);

            let el = route.get_segment_by_index(0);
            assert!(el.is_some());
            if let Some(route_segment) = el {
                assert!(line_is_between_point_ids(&ctx, route_segment.get_line(), 2, 1));
                assert_eq!(ctx.point(route_segment.get_end_point()).id, 2);
            }

            let el = route.get_segment_by_index(1);
            assert!(el.is_some());
            if let Some(route_segment) = el {
                assert!(line_is_between_point_ids(&ctx, route_segment.get_line(), 3, 2));
                assert_eq!(ctx.point(route_segment.get_end_point()).id, 3);
            }

            let el = route.get_segment_by_index(2);
            assert!(el.is_some());
            if let Some(route_segment) = el {
                assert!(line_is_between_point_ids(&ctx, route_segment.get_line(), 4, 3));
                assert_eq!(ctx.point(route_segment.get_end_point()).id, 4);
            }
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn handle_roundabout() {
            let graph = graph_from_test_dataset(test_dataset_2());
        let ctx = RoutingContext::new(&graph);

            let start = graph.test_get_point_ref_by_id(&6).unwrap();
            let finish = graph.test_get_point_ref_by_id(&131).unwrap();

            let mut walker = Walker::new(
                start.clone(),
            );

            let choices = match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Err(_) => panic!("Error received from move"),
                Ok(WalkerMoveResult::Fork(c)) => c,
                _ => panic!("did not get choices for routes"),
            };
            assert_eq!(choices.get_segment_count(), 2);

            choices.into_iter().for_each(|route_segment| {
                assert!(
                    ctx.point(route_segment.get_end_point()).id == 2
                        || ctx.point(route_segment.get_end_point()).id == 11
                );
                assert!(
                    line_is_between_point_ids(&ctx, route_segment.get_line(), 7, 2)
                        || line_is_between_point_ids(&ctx, route_segment.get_line(), 7, 11)
                )
            });

            let choice = graph.test_get_point_ref_by_id(&11).unwrap();
            walker.set_fork_choice_point_ref(choice);

            let choices = match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Err(_) => panic!("Error received from move"),
                Ok(WalkerMoveResult::Fork(c)) => c,
                _ => panic!("did not get choices for routes"),
            };

            assert_eq!(choices.get_segment_count(), 3);

            choices.into_iter().for_each(|route_segment| {
                assert!(
                    ctx.point(route_segment.get_end_point()).id == 111
                        || ctx.point(route_segment.get_end_point()).id == 121
                        || ctx.point(route_segment.get_end_point()).id == 131
                );
                assert!(
                    line_is_between_point_ids(&ctx, route_segment.get_line(), 11, 111)
                        || line_is_between_point_ids(&ctx, route_segment.get_line(), 12, 121)
                        || line_is_between_point_ids(&ctx, route_segment.get_line(), 13, 131)
                )
            });

            let choice = graph.test_get_point_ref_by_id(&131).unwrap();
            walker.set_fork_choice_point_ref(choice);

            match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Err(_) => panic!("Error received from move"),
                Ok(WalkerMoveResult::Finish) => {}
                _ => panic!("expected to reach finish"),
            };

            let route = walker.get_route().clone();
            assert!(route_matches_ids(&ctx, route, &[7, 11, 12, 13, 131]));
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn get_roundabout_exits_handles_simple_roundabout() {
            let tags_with_highway = HashMap::from([("highway".to_string(), "primary".to_string())]);
            let roundabout_tags = HashMap::from([
                ("junction".to_string(), "roundabout".to_string()),
                ("highway".to_string(), "primary".to_string()),
            ]);

            let graph = graph_from_test_dataset((
                vec![
                    OsmNode { id: 6, lat: 6.0, lon: 6.0, residential_in_proximity: false, nogo_area: false },
                    OsmNode { id: 7, lat: 7.0, lon: 7.0, residential_in_proximity: false, nogo_area: false },
                    OsmNode { id: 11, lat: 11.0, lon: 11.0, residential_in_proximity: false, nogo_area: false },
                    OsmNode { id: 12, lat: 12.0, lon: 12.0, residential_in_proximity: false, nogo_area: false },
                    OsmNode { id: 111, lat: 111.0, lon: 111.0, residential_in_proximity: false, nogo_area: false },
                ],
                vec![
                    OsmWay {
                        id: 67,
                        point_ids: vec![6, 7],
                        tags: Some(tags_with_highway.clone()),
                    },
                    OsmWay {
                        id: 71127,
                        point_ids: vec![7, 11, 12, 7],
                        tags: Some(roundabout_tags),
                    },
                    OsmWay {
                        id: 11111,
                        point_ids: vec![111, 11],
                        tags: Some(tags_with_highway),
                    },
                ],
                Vec::new(),
            ));
            let ctx = RoutingContext::new(&graph);

            let start = graph.test_get_point_ref_by_id(&6).unwrap();
            let point7 = graph.test_get_point_ref_by_id(&7).unwrap();
            let point11 = graph.test_get_point_ref_by_id(&11).unwrap();
            let mut walker = Walker::new(start.clone());

            let approach_segment = ctx
                .with_adjacent(&start, |adjacent| {
                    adjacent
                        .iter()
                        .find(|(_, point)| *point == point7)
                        .cloned()
                })
                .map(Segment::from)
                .unwrap();
            let roundabout_segment = ctx
                .with_adjacent(&point7, |adjacent| {
                    adjacent
                        .iter()
                        .find(|(_, point)| *point == point11)
                        .cloned()
                })
                .map(Segment::from)
                .unwrap();

            walker.route_walked.add_segment(&ctx, approach_segment);
            walker
                .route_walked
                .add_segment(&ctx, roundabout_segment.clone());

            let exits = walker.get_roundabout_exits_with_context(&ctx, &roundabout_segment);

            assert_eq!(exits.get_segment_count(), 1);
            assert!(
                exits
                    .get_all_segment_points()
                    .contains(&graph.test_get_point_ref_by_id(&111).unwrap())
            );
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn get_roundabout_exits_stops_when_roundabout_revisits_point() {
            let tags_with_highway = HashMap::from([("highway".to_string(), "primary".to_string())]);
            let roundabout_tags = HashMap::from([
                ("junction".to_string(), "roundabout".to_string()),
                ("highway".to_string(), "primary".to_string()),
            ]);

            let graph = graph_from_test_dataset((
                vec![
                    OsmNode { id: 6, lat: 6.0, lon: 6.0, residential_in_proximity: false, nogo_area: false },
                    OsmNode { id: 7, lat: 7.0, lon: 7.0, residential_in_proximity: false, nogo_area: false },
                    OsmNode { id: 11, lat: 11.0, lon: 11.0, residential_in_proximity: false, nogo_area: false },
                    OsmNode { id: 12, lat: 12.0, lon: 12.0, residential_in_proximity: false, nogo_area: false },
                    OsmNode { id: 13, lat: 13.0, lon: 13.0, residential_in_proximity: false, nogo_area: false },
                    OsmNode { id: 111, lat: 111.0, lon: 111.0, residential_in_proximity: false, nogo_area: false },
                    OsmNode { id: 131, lat: 131.0, lon: 131.0, residential_in_proximity: false, nogo_area: false },
                ],
                vec![
                    OsmWay {
                        id: 67,
                        point_ids: vec![6, 7],
                        tags: Some(tags_with_highway.clone()),
                    },
                    OsmWay {
                        id: 711121312,
                        point_ids: vec![7, 11, 12, 13, 12],
                        tags: Some(roundabout_tags),
                    },
                    OsmWay {
                        id: 11111,
                        point_ids: vec![111, 11],
                        tags: Some(tags_with_highway.clone()),
                    },
                    OsmWay {
                        id: 13131,
                        point_ids: vec![131, 13],
                        tags: Some(tags_with_highway),
                    },
                ],
                Vec::new(),
            ));
            let ctx = RoutingContext::new(&graph);

            let start = graph.test_get_point_ref_by_id(&6).unwrap();
            let finish = graph.test_get_point_ref_by_id(&131).unwrap();
            let mut walker = Walker::new(start);

            match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Ok(WalkerMoveResult::Fork(_)) => {}
                other => panic!("expected fork at roundabout entry, got {other:?}"),
            }

            let roundabout_segment = walker.get_route().get_segment_last().cloned().unwrap();
            let exits = walker.get_roundabout_exits_with_context(&ctx, &roundabout_segment);
            let exit_ids: Vec<_> = exits
                .get_all_segment_points()
                .iter()
                .map(|point| ctx.point_id(point))
                .collect();

            assert_eq!(exit_ids, vec![111, 131]);
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn move_to_roundabout_exit_keeps_continuing_roundabout_segments() {
            let graph = graph_from_test_dataset(test_dataset_2());
            let ctx = RoutingContext::new(&graph);

            let start = graph.test_get_point_ref_by_id(&6).unwrap();
            let finish = graph.test_get_point_ref_by_id(&131).unwrap();
            let mut walker = Walker::new(start);

            match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Ok(WalkerMoveResult::Fork(_)) => {}
                other => panic!("expected entry fork, got {other:?}"),
            }

            walker.set_fork_choice_point_ref(graph.test_get_point_ref_by_id(&11).unwrap());
            match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Ok(WalkerMoveResult::Fork(_)) => {}
                other => panic!("expected roundabout exit fork, got {other:?}"),
            }

            walker.move_to_roundabout_exit_with_context(
                &ctx,
                &graph.test_get_point_ref_by_id(&131).unwrap(),
            );

            let route = walker.get_route().clone();
            assert!(route_matches_ids(&ctx, route, &[7, 11, 12, 13]));
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn follow_one_way() {
            let graph = graph_from_test_dataset(test_dataset_2());
        let ctx = RoutingContext::new(&graph);

            let start = graph.test_get_point_ref_by_id(&6).unwrap();
            let finish = graph.test_get_point_ref_by_id(&9).unwrap();

            let mut walker = Walker::new(
                start.clone(),
            );

            let choices = match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Err(_) => panic!("Error received from move"),
                Ok(WalkerMoveResult::Fork(c)) => c,
                _ => panic!("did not get choices for routes"),
            };
            let next = graph.test_get_point_ref_by_id(&2).unwrap();
            assert!(choices.get_all_segment_points().contains(&next));

            let wrong_way_point = graph.test_get_point_ref_by_id(&8).unwrap();
            assert!(!choices.get_all_segment_points().contains(&wrong_way_point));
        }
    }

    fn rule_test(test_data: OsmTestData, can_go_ids: Vec<u64>, cannot_go_ids: Vec<u64>) {
        let graph = graph_from_test_dataset(test_data);
        let ctx = RoutingContext::new(&graph);

        let start = graph.test_get_point_ref_by_id(&1).unwrap();
        let finish = graph.test_get_point_ref_by_id(&7).unwrap();

        let mut walker = Walker::new(start.clone());

        let choices = match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
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
            let can_go = graph.test_get_point_ref_by_id(&can_go_id).unwrap();
            info!("go {}", can_go_id);
            assert!(choices.get_all_segment_points().contains(&can_go));
        }

        for cannot_go_id in cannot_go_ids {
            let cannot_go = graph.test_get_point_ref_by_id(&cannot_go_id).unwrap();
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
    fn assert_choice_ids(
        ctx: &RoutingContext<'_>,
        choices: &crate::router::route::segment_list::SegmentList,
        expected: &[u64],
    ) {
        let mut actual = choices
            .get_all_segment_points()
            .into_iter()
            .map(|point| ctx.point_id(&point))
            .collect::<Vec<_>>();
        actual.sort_unstable();

        let mut expected = expected.to_vec();
        expected.sort_unstable();

        assert_eq!(actual, expected);
    }

    fn osm_node(id: u64) -> ridi_router_common::osm::OsmNode {
        ridi_router_common::osm::OsmNode {
            id,
            lat: id as f64,
            lon: id as f64,
            residential_in_proximity: false,
            nogo_area: false,
        }
    }

    fn osm_way(id: u64, point_ids: &[u64]) -> ridi_router_common::osm::OsmWay {
        ridi_router_common::osm::OsmWay {
            id,
            point_ids: point_ids.to_vec(),
            tags: Some(HashMap::from([(
                "highway".to_string(),
                "primary".to_string(),
            )])),
        }
    }

    fn osm_one_way(id: u64, point_ids: &[u64]) -> ridi_router_common::osm::OsmWay {
        ridi_router_common::osm::OsmWay {
            id,
            point_ids: point_ids.to_vec(),
            tags: Some(HashMap::from([
                ("highway".to_string(), "primary".to_string()),
                ("oneway".to_string(), "yes".to_string()),
            ])),
        }
    }

    fn restriction_relation(
        from_way_id: u64,
        via_node_id: u64,
        to_way_ids: &[u64],
        restriction: &str,
    ) -> OsmRelation {
        let mut members = vec![
            OsmRelationMember {
                member_ref: from_way_id,
                role: OsmRelationMemberRole::From,
                member_type: OsmRelationMemberType::Way,
            },
            OsmRelationMember {
                member_ref: via_node_id,
                role: OsmRelationMemberRole::Via,
                member_type: OsmRelationMemberType::Node,
            },
        ];
        members.extend(to_way_ids.iter().map(|to_way_id| OsmRelationMember {
            member_ref: *to_way_id,
            role: OsmRelationMemberRole::To,
            member_type: OsmRelationMemberType::Way,
        }));

        OsmRelation {
            id: from_way_id * 1000 + via_node_id,
            members,
            tags: HashMap::from([
                ("type".to_string(), "restriction".to_string()),
                ("restriction".to_string(), restriction.to_string()),
            ]),
        }
    }

    fn corridor_finish_dataset() -> OsmTestData {
        (
            vec![osm_node(1), osm_node(2), osm_node(3), osm_node(4)],
            vec![osm_way(1234, &[1, 2, 3, 4])],
            Vec::new(),
        )
    }

    fn corridor_dead_end_dataset() -> OsmTestData {
        (
            vec![
                osm_node(1),
                osm_node(2),
                osm_node(3),
                osm_node(4),
                osm_node(99),
            ],
            vec![osm_way(1234, &[1, 2, 3, 4])],
            Vec::new(),
        )
    }

    fn corridor_fork_dataset() -> OsmTestData {
        (
            vec![
                osm_node(1),
                osm_node(2),
                osm_node(3),
                osm_node(4),
                osm_node(5),
                osm_node(6),
            ],
            vec![
                osm_way(12, &[1, 2]),
                osm_way(23, &[2, 3]),
                osm_way(34, &[3, 4]),
                osm_way(35, &[3, 5]),
                osm_way(46, &[4, 6]),
            ],
            Vec::new(),
        )
    }

    fn corridor_one_way_dataset() -> OsmTestData {
        (
            vec![
                osm_node(1),
                osm_node(2),
                osm_node(3),
                osm_node(4),
                osm_node(5),
                osm_node(6),
            ],
            vec![
                osm_way(12, &[1, 2]),
                osm_way(23, &[2, 3]),
                osm_way(34, &[3, 4]),
                osm_way(35, &[3, 5]),
                osm_one_way(63, &[6, 3]),
            ],
            Vec::new(),
        )
    }

    fn corridor_only_allowed_dataset() -> OsmTestData {
        (
            vec![
                osm_node(1),
                osm_node(2),
                osm_node(3),
                osm_node(4),
                osm_node(5),
                osm_node(6),
                osm_node(7),
                osm_node(8),
                osm_node(99),
            ],
            vec![
                osm_way(12, &[1, 2]),
                osm_way(23, &[2, 3]),
                osm_way(34, &[3, 4]),
                osm_way(35, &[3, 5]),
                osm_way(36, &[3, 6]),
                osm_way(47, &[4, 7]),
                osm_way(48, &[4, 8]),
            ],
            vec![restriction_relation(23, 3, &[34], "only_straight_on")],
        )
    }

    fn corridor_not_allowed_dataset() -> OsmTestData {
        (
            vec![
                osm_node(1),
                osm_node(2),
                osm_node(3),
                osm_node(4),
                osm_node(5),
                osm_node(6),
                osm_node(99),
            ],
            vec![
                osm_way(12, &[1, 2]),
                osm_way(23, &[2, 3]),
                osm_way(34, &[3, 4]),
                osm_way(35, &[3, 5]),
                osm_way(36, &[3, 6]),
            ],
            vec![restriction_relation(23, 3, &[35], "no_right_turn")],
        )
    }

    fn start_point_rule_filtering_dataset() -> OsmTestData {
        (
            vec![
                osm_node(1),
                osm_node(2),
                osm_node(3),
                osm_node(4),
                osm_node(99),
            ],
            vec![
                osm_way(12, &[1, 2]),
                osm_way(13, &[1, 3]),
                osm_way(14, &[1, 4]),
            ],
            vec![restriction_relation(12, 1, &[12, 13, 14], "no_exit")],
        )
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_moves_through_single_choice_corridor_until_finish() {
            let graph = graph_from_test_dataset(corridor_finish_dataset());
            let ctx = RoutingContext::new(&graph);
            let start = graph.test_get_point_ref_by_id(&1).unwrap();
            let finish = graph.test_get_point_ref_by_id(&4).unwrap();

            let mut walker = Walker::new(start);

            assert_eq!(
                walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish),
                Ok(WalkerMoveResult::Finish)
            );
            assert!(route_matches_ids(&ctx, walker.get_route().clone(), &[2, 3, 4]));
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_returns_fork_after_single_choice_corridor() {
            let graph = graph_from_test_dataset(corridor_fork_dataset());
            let ctx = RoutingContext::new(&graph);
            let start = graph.test_get_point_ref_by_id(&1).unwrap();
            let finish = graph.test_get_point_ref_by_id(&6).unwrap();

            let mut walker = Walker::new(start);

            let choices = match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Ok(WalkerMoveResult::Fork(choices)) => choices,
                other => panic!("expected fork after corridor, got {other:?}"),
            };

            assert!(route_matches_ids(&ctx, walker.get_route().clone(), &[2, 3]));
            assert_choice_ids(&ctx, &choices, &[4, 5]);
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_returns_dead_end_after_single_choice_corridor() {
            let graph = graph_from_test_dataset(corridor_dead_end_dataset());
            let ctx = RoutingContext::new(&graph);
            let start = graph.test_get_point_ref_by_id(&1).unwrap();
            let finish = graph.test_get_point_ref_by_id(&99).unwrap();

            let mut walker = Walker::new(start);

            assert_eq!(
                walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish),
                Ok(WalkerMoveResult::DeadEnd)
            );
            assert!(route_matches_ids(&ctx, walker.get_route().clone(), &[2, 3, 4]));
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_follows_explicit_choice_after_corridor() {
            let graph = graph_from_test_dataset(corridor_fork_dataset());
            let ctx = RoutingContext::new(&graph);
            let start = graph.test_get_point_ref_by_id(&1).unwrap();
            let finish = graph.test_get_point_ref_by_id(&6).unwrap();

            let mut walker = Walker::new(start);

            match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Ok(WalkerMoveResult::Fork(_)) => {}
                other => panic!("expected initial fork, got {other:?}"),
            }

            walker.set_fork_choice_point_ref(graph.test_get_point_ref_by_id(&4).unwrap());
            assert_eq!(
                walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish),
                Ok(WalkerMoveResult::Finish)
            );
            assert!(route_matches_ids(&ctx, walker.get_route().clone(), &[2, 3, 4, 6]));
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_wrong_choice_after_corridor_returns_same_available_points() {
            let graph = graph_from_test_dataset(corridor_fork_dataset());
            let ctx = RoutingContext::new(&graph);
            let start = graph.test_get_point_ref_by_id(&1).unwrap();
            let finish = graph.test_get_point_ref_by_id(&6).unwrap();

            let mut walker = Walker::new(start);

            match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Ok(WalkerMoveResult::Fork(_)) => {}
                other => panic!("expected initial fork, got {other:?}"),
            }

            walker.set_fork_choice_point_ref(graph.test_get_point_ref_by_id(&6).unwrap());
            assert_eq!(
                walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish),
                Err(WalkerError::WrongForkChoice {
                    id: 6,
                    available_fork_ids: vec![4, 5],
                })
            );
            assert!(route_matches_ids(&ctx, walker.get_route().clone(), &[2, 3]));
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_filters_reverse_edge_from_incoming_segment() {
            let graph = graph_from_test_dataset(corridor_fork_dataset());
            let ctx = RoutingContext::new(&graph);
            let start = graph.test_get_point_ref_by_id(&1).unwrap();
            let finish = graph.test_get_point_ref_by_id(&6).unwrap();

            let mut walker = Walker::new(start);

            let choices = match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Ok(WalkerMoveResult::Fork(choices)) => choices,
                other => panic!("expected fork after corridor, got {other:?}"),
            };

            assert!(!choices
                .get_all_segment_points()
                .contains(&graph.test_get_point_ref_by_id(&2).unwrap()));
            assert_choice_ids(&ctx, &choices, &[4, 5]);
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_respects_one_way_after_corridor() {
            let graph = graph_from_test_dataset(corridor_one_way_dataset());
            let ctx = RoutingContext::new(&graph);
            let start = graph.test_get_point_ref_by_id(&1).unwrap();
            let finish = graph.test_get_point_ref_by_id(&6).unwrap();

            let mut walker = Walker::new(start);

            let choices = match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Ok(WalkerMoveResult::Fork(choices)) => choices,
                other => panic!("expected fork after corridor, got {other:?}"),
            };

            assert_choice_ids(&ctx, &choices, &[4, 5]);
            assert!(!choices
                .get_all_segment_points()
                .contains(&graph.test_get_point_ref_by_id(&6).unwrap()));
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_respects_only_allowed_rule_after_corridor() {
            let graph = graph_from_test_dataset(corridor_only_allowed_dataset());
            let ctx = RoutingContext::new(&graph);
            let start = graph.test_get_point_ref_by_id(&1).unwrap();
            let finish = graph.test_get_point_ref_by_id(&99).unwrap();

            let mut walker = Walker::new(start);

            let choices = match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Ok(WalkerMoveResult::Fork(choices)) => choices,
                other => panic!("expected downstream fork after forced branch, got {other:?}"),
            };

            assert!(route_matches_ids(&ctx, walker.get_route().clone(), &[2, 3, 4]));
            assert_choice_ids(&ctx, &choices, &[7, 8]);
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_respects_not_allowed_rule_after_corridor() {
            let graph = graph_from_test_dataset(corridor_not_allowed_dataset());
            let ctx = RoutingContext::new(&graph);
            let start = graph.test_get_point_ref_by_id(&1).unwrap();
            let finish = graph.test_get_point_ref_by_id(&99).unwrap();

            let mut walker = Walker::new(start);

            let choices = match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Ok(WalkerMoveResult::Fork(choices)) => choices,
                other => panic!("expected filtered fork, got {other:?}"),
            };

            assert!(route_matches_ids(&ctx, walker.get_route().clone(), &[2, 3]));
            assert_choice_ids(&ctx, &choices, &[4, 6]);
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn walker_start_point_rule_filtering_matches_current_behavior() {
            let graph = graph_from_test_dataset(start_point_rule_filtering_dataset());
            let ctx = RoutingContext::new(&graph);
            let start = graph.test_get_point_ref_by_id(&1).unwrap();
            let finish = graph.test_get_point_ref_by_id(&99).unwrap();

            let mut walker = Walker::new(start);

            // This intentionally freezes the current start-point rule quirk: when every start
            // branch is mentioned in a NotAllowed-style rule set, the walker surfaces no legal
            // first move and returns DeadEnd with an empty route.
            assert_eq!(
                walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish),
                Ok(WalkerMoveResult::DeadEnd)
            );
            assert_eq!(walker.get_route().get_segment_count(), 0);
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn move_backwards_to_prev_fork_still_returns_expected_choices_after_refactor() {
            let graph = graph_from_test_dataset(corridor_fork_dataset());
            let ctx = RoutingContext::new(&graph);
            let start = graph.test_get_point_ref_by_id(&1).unwrap();
            let finish = graph.test_get_point_ref_by_id(&6).unwrap();

            let mut walker = Walker::new(start);

            match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Ok(WalkerMoveResult::Fork(_)) => {}
                other => panic!("expected initial fork, got {other:?}"),
            }

            walker.set_fork_choice_point_ref(graph.test_get_point_ref_by_id(&5).unwrap());
            assert_eq!(
                walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish),
                Ok(WalkerMoveResult::DeadEnd)
            );

            let choices = walker
                .move_backwards_to_prev_fork_with_context(&ctx)
                .expect("expected previous fork choices");

            assert!(route_matches_ids(&ctx, walker.get_route().clone(), &[2, 3]));
            assert_choice_ids(&ctx, &choices, &[4, 5]);
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn fork_classification_cache_reuses_revisited_oriented_segment() {
            let graph = graph_from_test_dataset(corridor_fork_dataset());
            let ctx = RoutingContext::new(&graph);
            let start = graph.test_get_point_ref_by_id(&1).unwrap();
            let finish = graph.test_get_point_ref_by_id(&6).unwrap();

            let mut walker = Walker::new(start);

            match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Ok(WalkerMoveResult::Fork(_)) => {}
                other => panic!("expected initial fork, got {other:?}"),
            }
            assert_eq!(walker.fork_classification_cache_size(), 2);

            walker.set_fork_choice_point_ref(graph.test_get_point_ref_by_id(&5).unwrap());
            assert_eq!(
                walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish),
                Ok(WalkerMoveResult::DeadEnd)
            );
            assert_eq!(walker.fork_classification_cache_size(), 3);

            let choices = walker
                .move_backwards_to_prev_fork_with_context(&ctx)
                .expect("expected previous fork choices");

            assert_eq!(walker.fork_classification_cache_size(), 3);
            assert_choice_ids(&ctx, &choices, &[4, 5]);
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn fork_classification_cache_keeps_rule_filtered_choices() {
            let graph = graph_from_test_dataset(corridor_only_allowed_dataset());
            let ctx = RoutingContext::new(&graph);
            let start = graph.test_get_point_ref_by_id(&1).unwrap();
            let finish = graph.test_get_point_ref_by_id(&99).unwrap();

            let mut walker = Walker::new(start);

            let choices = match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Ok(WalkerMoveResult::Fork(choices)) => choices,
                other => panic!("expected downstream fork after forced branch, got {other:?}"),
            };

            assert_eq!(walker.fork_classification_cache_size(), 3);
            assert_choice_ids(&ctx, &choices, &[7, 8]);

            let last_segment = walker
                .get_route()
                .get_segment_last()
                .cloned()
                .expect("expected cached fork segment");
            let cached_choices = walker.get_fork_segments_for_segment_with_context(&ctx, &last_segment);

            assert_eq!(walker.fork_classification_cache_size(), 3);
            assert_choice_ids(&ctx, &cached_choices, &[7, 8]);
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn roundabout_exit_cache_reuses_oriented_segment() {
            let graph = graph_from_test_dataset(test_dataset_2());
            let ctx = RoutingContext::new(&graph);
            let start = graph.test_get_point_ref_by_id(&6).unwrap();
            let finish = graph.test_get_point_ref_by_id(&131).unwrap();

            let mut walker = Walker::new(start);
            match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Ok(WalkerMoveResult::Fork(_)) => {}
                other => panic!("expected entry fork, got {other:?}"),
            }

            walker.set_fork_choice_point_ref(graph.test_get_point_ref_by_id(&11).unwrap());
            match walker.move_forward_to_next_fork_with_context(&ctx, |p| p == finish) {
                Ok(WalkerMoveResult::Fork(_)) => {}
                other => panic!("expected roundabout exit fork, got {other:?}"),
            }

            let roundabout_segment = walker
                .get_route()
                .get_segment_last()
                .cloned()
                .expect("expected roundabout segment");

            let first = walker.get_roundabout_exits_with_context(&ctx, &roundabout_segment);
            assert_eq!(walker.roundabout_exit_cache_size(), 1);
            let second = walker.get_roundabout_exits_with_context(&ctx, &roundabout_segment);

            assert_eq!(walker.roundabout_exit_cache_size(), 1);
            assert_choice_ids(&ctx, &first, &[111, 121, 131]);
            assert_choice_ids(&ctx, &second, &[111, 121, 131]);
        }
    }
}
