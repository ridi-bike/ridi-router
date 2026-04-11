use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
};

use tracing::trace;

use crate::{map_data::graph::MapDataPointRef, router::rules::RouterRules, RoutingContext};

use super::{
    itinerary::Itinerary,
    route::Route,
    walker::{Walker, WalkerMoveResult},
    weights::{WeightCalc, WeightCalcInput, WeightCalcStage},
};

#[derive(Debug, Clone, PartialEq)]
pub enum WeightCalcResult {
    ForkChoiceUseWithWeight(u8),
    ForkChoiceDoNotUse,
    LastSegmentDoNotUse,
}

#[derive(Debug)]
pub struct DiscardedForkChoices {
    choices: Vec<HashMap<MapDataPointRef, HashSet<MapDataPointRef>>>,
    reset_at_new_next: bool,
}
impl DiscardedForkChoices {
    pub fn new(reset_at_new_next: bool) -> Self {
        Self {
            choices: vec![HashMap::new()],
            reset_at_new_next,
        }
    }

    pub fn set_new_next(&mut self) {
        if self.reset_at_new_next {
            self.choices.push(HashMap::new());
        }
    }

    pub fn set_prev_next(&mut self) {
        if self.reset_at_new_next {
            self.choices.pop();
        }
    }

    pub fn add_discarded_choice(
        &mut self,
        point_ref: &MapDataPointRef,
        choice_point_ref: &MapDataPointRef,
    ) {
        let existing_choices = self.choices.last()
            .expect("There should always be an entry. set_new_next and set_prev_next incorrectly called")
            .get(point_ref);
        if let Some(existing_choices) = existing_choices {
            let mut existing_choices = existing_choices.clone();
            existing_choices.insert(choice_point_ref.clone());
            self.choices
                .last_mut()
                .expect("There should always be an entry. set_new_next and set_prev_next incorrectly called")
                .insert(point_ref.clone(), existing_choices);
        } else {
            let mut ids = HashSet::new();
            ids.insert(choice_point_ref.clone());
            self.choices
                .last_mut()
                .expect("There should always be an entry. set_new_next and set_prev_next incorrectly called")
                .insert(point_ref.clone(), ids);
        }
    }

    pub fn get_discarded_choices_for_point(
        &self,
        point_ref: &MapDataPointRef,
    ) -> Option<Vec<MapDataPointRef>> {
        self.choices
            .last()
            .expect("There should always be an entry. set_new_next and set_prev_next incorrectly called")
            .get(point_ref)
            .map(|ids| ids.clone().into_iter().collect())
    }
}

#[derive(Clone)]
pub struct ForkWeights {
    pub discard_fork: bool,
    weight_list: HashMap<MapDataPointRef, u32>,
}

impl ForkWeights {
    pub fn new() -> Self {
        Self {
            discard_fork: false,
            weight_list: HashMap::new(),
        }
    }
    pub fn discard(&mut self) {
        self.discard_fork = true;
    }

    pub fn add_choice_weight(&mut self, choice_point_ref: &MapDataPointRef, weight: u32) {
        let existing_weight = self.weight_list.get(choice_point_ref).copied().unwrap_or(0);
        self.weight_list
            .insert(choice_point_ref.clone(), existing_weight + weight);
    }

    fn get_choices_sorted_by_weight(&self) -> Vec<(&MapDataPointRef, &u32)> {
        let mut vec = self.weight_list.iter().collect::<Vec<_>>();
        vec.sort_by(|v, v2| v2.1.cmp(v.1));
        vec
    }

    pub fn get_choice_id_by_index_from_heaviest(&self, idx: usize) -> Option<MapDataPointRef> {
        if self.discard_fork {
            return None;
        }
        let vec = self.get_choices_sorted_by_weight();
        vec.get(idx).map(|w| w.0).cloned()
    }
}

impl Debug for ForkWeights {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.weight_list
                .iter()
                .fold(String::new(), |all, el| format!(
                    "{}\n\t{}:{}",
                    all, el.0, el.1
                ))
        )
    }
}

pub enum NavigationResult {
    Stuck,
    Stopped,
    Finished(Route),
}

pub struct Navigator {
    itinerary: Itinerary,
    rules: RouterRules,
    walker: Walker,
    route_once_weight_calcs: Vec<WeightCalc>,
    per_fork_weight_calcs: Vec<WeightCalc>,
    discarded_fork_choices: DiscardedForkChoices,
}

#[hotpath::measure_all]
impl Navigator {
    pub fn new(
        itinerary: Itinerary,
        rules: RouterRules,
        weight_calcs: Vec<WeightCalc>,
        reset_at_new_next: bool,
    ) -> Self {
        let mut route_once_weight_calcs = Vec::new();
        let mut per_fork_weight_calcs = Vec::new();
        for weight_calc in weight_calcs {
            match weight_calc.stage {
                WeightCalcStage::RouteOnce => route_once_weight_calcs.push(weight_calc),
                WeightCalcStage::PerForkChoice => per_fork_weight_calcs.push(weight_calc),
            }
        }

        Self {
            walker: Walker::new(itinerary.start.clone()),
            itinerary,
            rules,
            route_once_weight_calcs,
            per_fork_weight_calcs,
            discarded_fork_choices: DiscardedForkChoices::new(reset_at_new_next),
        }
    }

    #[tracing::instrument(skip(self, ctx))]
    pub(crate) fn generate_routes_with_context(
        mut self,
        ctx: &RoutingContext<'_>,
    ) -> NavigationResult {
        trace!("Route generation started");

        let mut loop_counter = 0;
        loop {
            loop_counter += 1;

            let move_result = self
                .walker
                .move_forward_to_next_fork_with_context(ctx, |p| self.itinerary.is_finished(p));

            if move_result == Ok(WalkerMoveResult::Finish) {
                return NavigationResult::Finished(self.walker.get_route().clone());
            }
            if let Ok(WalkerMoveResult::Fork(fork_choices)) = move_result {
                let last_point = self.walker.get_last_point();
                let discarded_choices = &self
                    .discarded_fork_choices
                    .get_discarded_choices_for_point(last_point)
                    .map_or(Vec::new(), |d| d);
                let fork_choices = fork_choices.exclude_segments_where_points_in(discarded_choices);

                if self.itinerary.check_set_next(ctx, last_point.clone()) {
                    self.discarded_fork_choices.set_new_next();
                }

                let mut fork_weights = ForkWeights::new();
                if let Some(representative_fork_segment) = fork_choices.get_first_segment() {
                    for weight_calc in &self.route_once_weight_calcs {
                        let weight_calc_result = (weight_calc.calc)(WeightCalcInput {
                            route: self.walker.get_route(),
                            itinerary: &self.itinerary,
                            current_fork_segment: representative_fork_segment,
                            walker_from_fork: Walker::new(
                                representative_fork_segment.get_end_point().clone(),
                            ),
                            rules: &self.rules,
                            ctx,
                        });
                        if matches!(
                            weight_calc_result,
                            WeightCalcResult::LastSegmentDoNotUse
                                | WeightCalcResult::ForkChoiceDoNotUse
                        ) {
                            fork_weights.discard();
                            break;
                        }
                    }
                }

                if !fork_weights.discard_fork {
                    for fork_route_segment in fork_choices.clone() {
                        let mut total_weight = 0u32;
                        let mut skip_choice = false;

                        for weight_calc in &self.per_fork_weight_calcs {
                            let weight_calc_result = (weight_calc.calc)(WeightCalcInput {
                                route: self.walker.get_route(),
                                itinerary: &self.itinerary,
                                current_fork_segment: &fork_route_segment,
                                walker_from_fork: Walker::new(
                                    fork_route_segment.get_end_point().clone(),
                                ),
                                rules: &self.rules,
                                ctx,
                            });

                            match weight_calc_result {
                                WeightCalcResult::ForkChoiceUseWithWeight(weight) => {
                                    total_weight += weight as u32;
                                }
                                WeightCalcResult::ForkChoiceDoNotUse => {
                                    skip_choice = true;
                                    break;
                                }
                                WeightCalcResult::LastSegmentDoNotUse => {
                                    fork_weights.discard();
                                    skip_choice = true;
                                    break;
                                }
                            }
                        }

                        if fork_weights.discard_fork {
                            break;
                        }

                        if !skip_choice {
                            fork_weights.add_choice_weight(
                                fork_route_segment.get_end_point(),
                                total_weight,
                            );
                        }
                    }
                }

                let chosen_fork_point = fork_weights.get_choice_id_by_index_from_heaviest(0);

                if let Some(chosen_fork_point) = chosen_fork_point {
                    self.discarded_fork_choices
                        .add_discarded_choice(last_point, &chosen_fork_point);
                    self.walker.set_fork_choice_point_ref(chosen_fork_point);
                } else {
                    if self
                        .walker
                        .get_route()
                        .get_junction_before_last_segment(ctx)
                        .is_none()
                    {
                        trace!("Stuck");
                        return NavigationResult::Stuck;
                    }
                    if self
                        .itinerary
                        .check_set_back(self.walker.get_last_point().clone())
                    {
                        self.discarded_fork_choices.set_prev_next();
                    }
                    self.walker.move_backwards_to_prev_fork_with_context(ctx);
                }
            } else if move_result == Ok(WalkerMoveResult::DeadEnd) {
                if self
                    .itinerary
                    .check_set_back(self.walker.get_last_point().clone())
                {
                    self.discarded_fork_choices.set_prev_next();
                }
                self.walker.move_backwards_to_prev_fork_with_context(ctx);
            }

            if loop_counter >= self.rules.basic.step_limit.0 {
                trace!("Reached loop {loop_counter}, stopping");
                return NavigationResult::Stopped;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::assertions_on_constants)]
mod test {
    use crate::{
        router::{
            itinerary::Itinerary,
            navigator::{NavigationResult, WeightCalcResult},
            rules::RouterRules,
            weights::{WeightCalc, WeightCalcInput, WeightCalcStage},
        },
        test_utils::{route_matches_ids, test_dataset_1, RoutingTestContext},
    };

    use super::Navigator;
    use rusty_fork::rusty_fork_test;

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn navigate_pick_best() {
            fn weight(input: WeightCalcInput) -> WeightCalcResult {
                let prev_point = match input.route.get_segment_last() {
                    Some(segment) => segment.get_end_point(),
                    None => &input.itinerary.start.clone(),
                };
                if input.ctx.point(prev_point).id == 3
                    && input.ctx.point(input.current_fork_segment.get_end_point()).id == 6
                {
                    return WeightCalcResult::ForkChoiceUseWithWeight(10);
                }
                WeightCalcResult::ForkChoiceUseWithWeight(1)
            }
            let test_ctx = RoutingTestContext::new(test_dataset_1());
            let ctx = test_ctx.resolver();
            let from = test_ctx.point(1);
            let to = test_ctx.point(7);
            let itinerary = Itinerary::new_start_finish(from, to, Vec::new(), 0.);
            let navigator = Navigator::new(
                itinerary.clone(),
                RouterRules::default(),
                vec![WeightCalc{calc: weight, name:"weight".to_string(), stage: WeightCalcStage::PerForkChoice}],
                false
            );
            let route = match navigator.generate_routes_with_context(&ctx) {
                crate::router::navigator::NavigationResult::Finished(r) => r,
                _ => {
                    assert!(false);
                    return ;
                }
            };

            assert!(route_matches_ids(&ctx, route.clone(), &[2, 3, 6, 7]));

            fn weight2(input: WeightCalcInput) -> WeightCalcResult {
                let prev_point = match input.route.get_segment_last() {
                    Some(segment) => segment.get_end_point(),
                    None => &input.itinerary.finish.clone(),
                };

                if input.ctx.point(prev_point).id == 3
                    && input.ctx.point(input.current_fork_segment.get_end_point()).id == 4
                {
                    return WeightCalcResult::ForkChoiceUseWithWeight(10);
                }
                WeightCalcResult::ForkChoiceUseWithWeight(1)
            }
            let navigator = Navigator::new(
                itinerary,
                RouterRules::default(),
                vec![WeightCalc{ calc:weight2, name:"weight2".to_string(), stage: WeightCalcStage::PerForkChoice }],
                false
            );
            let route = match navigator.generate_routes_with_context(&ctx) {
                crate::router::navigator::NavigationResult::Finished(r) => r,
                _ => {
                    assert!(false);
                    return ;
                }
            };

            assert!(route_matches_ids(&ctx, route.clone(), &[2, 3, 4, 8, 6, 7]));
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn navigate_dead_end_pick_next_best() {
            fn weight(input: WeightCalcInput) -> WeightCalcResult {
                let prev_point = match input.route.get_segment_last() {
                    Some(segment) => segment.get_end_point(),
                    None => &input.itinerary.finish.clone(),
                };

                if input.ctx.point(prev_point).id == 3 {
                    if input.ctx.point(input.current_fork_segment.get_end_point()).id == 5 {
                        return WeightCalcResult::ForkChoiceUseWithWeight(10);
                    }
                    if input.ctx.point(input.current_fork_segment.get_end_point()).id == 6 {
                        return WeightCalcResult::ForkChoiceUseWithWeight(5);
                    }
                }
                if input.ctx.point(prev_point).id == 6
                    && input.ctx.point(input.current_fork_segment.get_end_point()).id == 7
                {
                    return WeightCalcResult::ForkChoiceUseWithWeight(10);
                }
                WeightCalcResult::ForkChoiceUseWithWeight(1)
            }
            let test_ctx = RoutingTestContext::new(test_dataset_1());
            let ctx = test_ctx.resolver();
            let from = test_ctx.point(1);
            let to = test_ctx.point(7);
            let itinerary = Itinerary::new_start_finish(from, to, Vec::new(), 0.);
            let navigator = Navigator::new(
                itinerary,
                RouterRules::default(),
                vec![WeightCalc{ calc: weight, name:"weight".to_string(), stage: WeightCalcStage::PerForkChoice }],
                false,
            );
            let route = match navigator.generate_routes_with_context(&ctx) {
                crate::router::navigator::NavigationResult::Finished(r) => r,
                _ => {
                    assert!(false);
                    return ;
                }
            };

            assert!(route_matches_ids(&ctx, route.clone(), &[2, 3, 6, 7]));
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn navigate_all_stuck_return_no_routes() {
            fn weight(_input: WeightCalcInput) -> WeightCalcResult {
                WeightCalcResult::ForkChoiceUseWithWeight(1)
            }
            let test_ctx = RoutingTestContext::new(test_dataset_1());
            let ctx = test_ctx.resolver();
            let from = test_ctx.point(1);
            let to = test_ctx.point(11);
            let itinerary = Itinerary::new_start_finish(from, to, Vec::new(), 0.);
            let navigator = Navigator::new(
                itinerary,
                RouterRules::default(),
                vec![WeightCalc{calc: weight, name:"weight".to_string(), stage: WeightCalcStage::PerForkChoice}],
                false,
            );

            if let NavigationResult::Finished(_) = navigator.generate_routes_with_context(&ctx) {
                assert!(false);
            }
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn navigate_no_routes_with_do_not_use_weight() {
            fn weight(input: WeightCalcInput) -> WeightCalcResult {
                if input.ctx.point(input.current_fork_segment.get_end_point()).id == 7 {
                    return WeightCalcResult::ForkChoiceDoNotUse;
                }
                WeightCalcResult::ForkChoiceUseWithWeight(1)
            }
            let test_ctx = RoutingTestContext::new(test_dataset_1());
            let ctx = test_ctx.resolver();
            let from = test_ctx.point(1);
            let to = test_ctx.point(7);
            let itinerary = Itinerary::new_start_finish(from, to, Vec::new(), 0.);
            let navigator = Navigator::new(
                itinerary,
                RouterRules::default(),
                vec![WeightCalc{ calc: weight, name:"weight".to_string(), stage: WeightCalcStage::PerForkChoice}],
                false
            );
            if let NavigationResult::Finished(_) = navigator.generate_routes_with_context(&ctx) {
                assert!(false);
            }
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn navigate_on_weight_sum() {
            fn weight1(input: WeightCalcInput) -> WeightCalcResult {
                let prev_point = match input.route.get_segment_last() {
                    Some(segment) => segment.get_end_point(),
                    None => &input.itinerary.finish.clone(),
                };
                if input.ctx.point(prev_point).id == 3
                    && input.ctx.point(input.current_fork_segment.get_end_point()).id == 6
                {
                    return WeightCalcResult::ForkChoiceUseWithWeight(10);
                }
                WeightCalcResult::ForkChoiceUseWithWeight(6)
            }
            fn weight2(input: WeightCalcInput) -> WeightCalcResult {
                let prev_point = match input.route.get_segment_last() {
                    Some(segment) => segment.get_end_point(),
                    None => &input.itinerary.finish.clone(),
                };

                if input.ctx.point(prev_point).id == 3
                    && input.ctx.point(input.current_fork_segment.get_end_point()).id == 6
                {
                    return WeightCalcResult::ForkChoiceUseWithWeight(1);
                }
                WeightCalcResult::ForkChoiceUseWithWeight(6)
            }
            let test_ctx = RoutingTestContext::new(test_dataset_1());
            let ctx = test_ctx.resolver();
            let from = test_ctx.point(1);
            let to = test_ctx.point(7);
            let itinerary = Itinerary::new_start_finish(from, to, Vec::new(), 0.);
            let navigator = Navigator::new(
                itinerary,
                RouterRules::default(),
                vec![
                    WeightCalc{calc: weight1, name:"weight1".to_string(), stage: WeightCalcStage::PerForkChoice},
                    WeightCalc{ calc: weight2, name:"weight2".to_string(), stage: WeightCalcStage::PerForkChoice},
                ],
                false,
            );
            let route = match navigator.generate_routes_with_context(&ctx) {
                crate::router::navigator::NavigationResult::Finished(r) => r,
                _ => {
                    assert!(false);
                    return ;
                }
            };
            assert!(route_matches_ids(&ctx, route.clone(), &[2, 3, 4, 8, 6, 7]));
        }
    }
}

#[cfg(test)]
#[path = "navigator_phase1_tests.rs"]
mod phase1_tests;

#[cfg(test)]
#[path = "navigator_phase3_tests.rs"]
mod phase3_tests;
