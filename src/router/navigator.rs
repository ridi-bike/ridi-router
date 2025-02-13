use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
};

use tracing::trace;

use crate::{
    debug::writer::DebugWriter, map_data::graph::MapDataPointRef, router::rules::RouterRules,
};

use super::{
    itinerary::Itinerary,
    route::Route,
    walker::{Walker, WalkerMoveResult},
    weights::{WeightCalc, WeightCalcInput},
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
    pub fn add_calc_result(
        &mut self,
        choice_point_ref: &MapDataPointRef,
        weights: &Vec<WeightCalcResult>,
    ) {
        if weights
            .iter()
            .any(|weight| *weight == WeightCalcResult::LastSegmentDoNotUse)
        {
            self.discard_fork = true;
            return;
        }
        if weights
            .iter()
            .all(|weight| *weight != WeightCalcResult::ForkChoiceDoNotUse)
        {
            let existing_weight = match self.weight_list.get(choice_point_ref) {
                None => 0u32,
                Some(w) => *w,
            };
            self.weight_list.insert(
                choice_point_ref.clone(),
                existing_weight
                    + weights
                        .iter()
                        .map(|r| match r {
                            WeightCalcResult::ForkChoiceDoNotUse => 0u32,
                            WeightCalcResult::LastSegmentDoNotUse => 0u32,
                            WeightCalcResult::ForkChoiceUseWithWeight(w) => *w as u32,
                        })
                        .sum::<u32>(),
            );
        }
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
                    all,
                    el.0.borrow().id,
                    el.1
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
    weight_calcs: Vec<WeightCalc>,
    discarded_fork_choices: DiscardedForkChoices,
}

impl Navigator {
    pub fn new(
        itinerary: Itinerary,
        rules: RouterRules,
        weight_calcs: Vec<WeightCalc>,
        reset_at_new_next: bool,
    ) -> Self {
        Self {
            walker: Walker::new(itinerary.start.clone()),
            itinerary,
            rules,
            weight_calcs,
            discarded_fork_choices: DiscardedForkChoices::new(reset_at_new_next),
        }
    }

    #[tracing::instrument(skip(self), fields(id = self.itinerary.id()))]
    pub fn generate_routes(mut self) -> NavigationResult {
        trace!("Route gen for itinerary {}", self.itinerary);

        let mut loop_counter = 0;
        loop {
            loop_counter += 1;

            let move_result = self
                .walker
                .move_forward_to_next_fork(|p| self.itinerary.is_finished(p));

            DebugWriter::write_step(
                self.itinerary.id(),
                loop_counter,
                &move_result,
                self.walker.get_route(),
            );

            if move_result == Ok(WalkerMoveResult::Finish) {
                return NavigationResult::Finished(self.walker.get_route().clone());
            }
            if let Ok(WalkerMoveResult::Fork(fork_choices)) = move_result {
                let last_point = self.walker.get_last_point();
                let discarded_choices = &self
                    .discarded_fork_choices
                    .get_discarded_choices_for_point(last_point)
                    .map_or(Vec::new(), |d| d);
                DebugWriter::write_fork_choices(
                    self.itinerary.id(),
                    loop_counter,
                    &fork_choices,
                    discarded_choices,
                );
                let fork_choices = fork_choices.exclude_segments_where_points_in(discarded_choices);

                if self.itinerary.check_set_next(last_point.clone()) {
                    self.discarded_fork_choices.set_new_next();
                }

                let fork_weights = fork_choices.clone().into_iter().fold(
                    ForkWeights::new(),
                    |mut fork_weights, fork_route_segment| {
                        if !fork_weights.discard_fork {
                            let fork_weight_calc_results = self
                                .weight_calcs
                                .iter()
                                .map(|weight_calc| {
                                    let weight_calc_result = (weight_calc.calc)(WeightCalcInput {
                                        route: self.walker.get_route(),
                                        itinerary: &self.itinerary,
                                        current_fork_segment: &fork_route_segment,
                                        walker_from_fork: Walker::new(
                                            fork_route_segment.get_end_point().clone(),
                                        ),
                                        rules: &self.rules,
                                    });
                                    DebugWriter::write_fork_choice_weight(
                                        self.itinerary.id(),
                                        loop_counter,
                                        &fork_route_segment.get_end_point().borrow().id,
                                        &weight_calc.name,
                                        &weight_calc_result,
                                    );
                                    weight_calc_result
                                })
                                .collect::<Vec<_>>();

                            fork_weights.add_calc_result(
                                fork_route_segment.get_end_point(),
                                &fork_weight_calc_results,
                            );
                        }

                        fork_weights
                    },
                );

                let chosen_fork_point = fork_weights.get_choice_id_by_index_from_heaviest(0);

                if let Some(chosen_fork_point) = chosen_fork_point {
                    self.discarded_fork_choices
                        .add_discarded_choice(last_point, &chosen_fork_point);
                    DebugWriter::write_step_result(
                        self.itinerary.id(),
                        loop_counter,
                        "ForkChoice",
                        Some(chosen_fork_point.borrow().id),
                    );
                    self.walker.set_fork_choice_point_ref(chosen_fork_point);
                } else {
                    if self
                        .walker
                        .get_route()
                        .get_junction_before_last_segment()
                        .is_none()
                    {
                        trace!("Stuck");
                        DebugWriter::write_step_result(
                            self.itinerary.id(),
                            loop_counter,
                            "Stuck",
                            None,
                        );
                        return NavigationResult::Stuck;
                    }
                    if self
                        .itinerary
                        .check_set_back(self.walker.get_last_point().clone())
                    {
                        self.discarded_fork_choices.set_prev_next();
                    }
                    self.walker.move_backwards_to_prev_fork();
                    DebugWriter::write_step_result(
                        self.itinerary.id(),
                        loop_counter,
                        "MoveBack",
                        None,
                    );
                }
            } else if move_result == Ok(WalkerMoveResult::DeadEnd) {
                DebugWriter::write_step_result(self.itinerary.id(), loop_counter, "MoveBack", None);
                if self
                    .itinerary
                    .check_set_back(self.walker.get_last_point().clone())
                {
                    self.discarded_fork_choices.set_prev_next();
                }
                self.walker.move_backwards_to_prev_fork();
            }

            if loop_counter >= self.rules.basic.step_limit.0 {
                trace!("Reached loop {loop_counter}, stopping");
                DebugWriter::write_step_result(self.itinerary.id(), loop_counter, "Stopped", None);
                return NavigationResult::Stopped;
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{
        map_data::graph::MapDataGraph,
        router::{
            itinerary::Itinerary,
            navigator::{NavigationResult, WeightCalcResult},
            rules::RouterRules,
            weights::{WeightCalc, WeightCalcInput},
        },
        test_utils::{
            graph_from_test_dataset, route_matches_ids, set_graph_static, test_dataset_1,
        },
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
                if prev_point.borrow().id == 3
                    && input.current_fork_segment.get_end_point().borrow().id == 6
                {
                    return WeightCalcResult::ForkChoiceUseWithWeight(10);
                }
                WeightCalcResult::ForkChoiceUseWithWeight(1)
            }
            set_graph_static(graph_from_test_dataset(test_dataset_1()));
            let from = MapDataGraph::get().test_get_point_ref_by_id(&1).unwrap();
            let to = MapDataGraph::get().test_get_point_ref_by_id(&7).unwrap();
            let itinerary = Itinerary::new_start_finish(from, to, Vec::new(), 0.);
            let navigator = Navigator::new(
                itinerary.clone(),
                RouterRules::default(),
                vec![WeightCalc{calc: weight, name:"weight".to_string()}],
                false
            );
            let route = match navigator.generate_routes() {
                crate::router::navigator::NavigationResult::Finished(r) => r,
                _ => {
                    assert!(false);
                    return ;
                }
            };

            assert!(route_matches_ids(route.clone(), vec![2, 3, 6, 7]));

            fn weight2(input: WeightCalcInput) -> WeightCalcResult {
                let prev_point = match input.route.get_segment_last() {
                    Some(segment) => segment.get_end_point(),
                    None => &input.itinerary.finish.clone(),
                };

                if prev_point.borrow().id == 3
                    && input.current_fork_segment.get_end_point().borrow().id == 4
                {
                    return WeightCalcResult::ForkChoiceUseWithWeight(10);
                }
                WeightCalcResult::ForkChoiceUseWithWeight(1)
            }
            let navigator = Navigator::new(
                itinerary,
                RouterRules::default(),
                vec![WeightCalc{ calc:weight2, name:"weight2".to_string() }],
                false
            );
            let route = match navigator.generate_routes() {
                crate::router::navigator::NavigationResult::Finished(r) => r,
                _ => {
                    assert!(false);
                    return ;
                }
            };

            assert!(route_matches_ids(route.clone(), vec![2, 3, 4, 8, 6, 7]));
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

                if prev_point.borrow().id == 3 {
                    if input.current_fork_segment.get_end_point().borrow().id == 5 {
                        return WeightCalcResult::ForkChoiceUseWithWeight(10);
                    }
                    if input.current_fork_segment.get_end_point().borrow().id == 6 {
                        return WeightCalcResult::ForkChoiceUseWithWeight(5);
                    }
                }
                if prev_point.borrow().id == 6
                    && input.current_fork_segment.get_end_point().borrow().id == 7
                {
                    return WeightCalcResult::ForkChoiceUseWithWeight(10);
                }
                WeightCalcResult::ForkChoiceUseWithWeight(1)
            }
            set_graph_static(graph_from_test_dataset(test_dataset_1()));
            let from = MapDataGraph::get().test_get_point_ref_by_id(&1).unwrap();
            let to = MapDataGraph::get().test_get_point_ref_by_id(&7).unwrap();
            let itinerary = Itinerary::new_start_finish(from, to, Vec::new(), 0.);
            let navigator = Navigator::new(
                itinerary,
                RouterRules::default(),
                vec![WeightCalc{ calc: weight, name:"weight".to_string() }],
                false,
            );
            let route = match navigator.generate_routes() {
                crate::router::navigator::NavigationResult::Finished(r) => r,
                _ => {
                    assert!(false);
                    return ;
                }
            };

            assert!(route_matches_ids(route.clone(), vec![2, 3, 6, 7]));
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn navigate_all_stuck_return_no_routes() {
            fn weight(_input: WeightCalcInput) -> WeightCalcResult {
                WeightCalcResult::ForkChoiceUseWithWeight(1)
            }
            set_graph_static(graph_from_test_dataset(test_dataset_1()));
            let from = MapDataGraph::get().test_get_point_ref_by_id(&1).unwrap();
            let to = MapDataGraph::get().test_get_point_ref_by_id(&11).unwrap();
            let itinerary = Itinerary::new_start_finish(from, to, Vec::new(), 0.);
            let navigator = Navigator::new(
                itinerary,
                RouterRules::default(),
                vec![WeightCalc{calc: weight, name:"weight".to_string()}],
                false,
            );

            if let NavigationResult::Finished(_) = navigator.generate_routes() {
                assert!(false);
            }
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn navigate_no_routes_with_do_not_use_weight() {
            fn weight(input: WeightCalcInput) -> WeightCalcResult {
                if input.current_fork_segment.get_end_point().borrow().id == 7 {
                    return WeightCalcResult::ForkChoiceDoNotUse;
                }
                WeightCalcResult::ForkChoiceUseWithWeight(1)
            }
            set_graph_static(graph_from_test_dataset(test_dataset_1()));
            let from = MapDataGraph::get().test_get_point_ref_by_id(&1).unwrap();
            let to = MapDataGraph::get().test_get_point_ref_by_id(&7).unwrap();
            let itinerary = Itinerary::new_start_finish(from, to, Vec::new(), 0.);
            let navigator = Navigator::new(
                itinerary,
                RouterRules::default(),
                vec![WeightCalc{ calc: weight, name:"weight".to_string()}],
                false
            );
            if let NavigationResult::Finished(_) = navigator.generate_routes() {
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
                if prev_point.borrow().id == 3
                    && input.current_fork_segment.get_end_point().borrow().id == 6
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

                if prev_point.borrow().id == 3
                    && input.current_fork_segment.get_end_point().borrow().id == 6
                {
                    return WeightCalcResult::ForkChoiceUseWithWeight(1);
                }
                WeightCalcResult::ForkChoiceUseWithWeight(6)
            }
            set_graph_static(graph_from_test_dataset(test_dataset_1()));
            let from = MapDataGraph::get().test_get_point_ref_by_id(&1).unwrap();
            let to = MapDataGraph::get().test_get_point_ref_by_id(&7).unwrap();
            let itinerary = Itinerary::new_start_finish(from, to, Vec::new(), 0.);
            let navigator = Navigator::new(
                itinerary,
                RouterRules::default(),
                vec![WeightCalc{calc: weight1, name:"weight1".to_string()}, WeightCalc{ calc: weight2, name:"weight2".to_string()}],
                false,
            );
            let route = match navigator.generate_routes() {
                crate::router::navigator::NavigationResult::Finished(r) => r,
                _ => {
                    assert!(false);
                    return ;
                }
            };
            assert!(route_matches_ids(route.clone(), vec![2, 3, 4, 8, 6, 7]));
        }
    }
}
