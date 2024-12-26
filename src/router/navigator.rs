use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
};

use tracing::{info, trace};

use crate::{
    debug_writer::DebugWriter, map_data::graph::MapDataPointRef, router::rules::RouterRules,
};

use super::{
    itinerary::Itinerary,
    route::Route,
    walker::{Walker, WalkerMoveResult},
    weights::{WeightCalc, WeightCalcInput},
};

#[derive(Debug, Clone, PartialEq)]
pub enum WeightCalcResult {
    UseWithWeight(u8),
    DoNotUse,
}

#[derive(Debug)]
pub struct DiscardedForkChoices {
    choices: HashMap<MapDataPointRef, HashSet<MapDataPointRef>>,
}
impl DiscardedForkChoices {
    pub fn new() -> Self {
        Self {
            choices: HashMap::new(),
        }
    }

    pub fn add_discarded_choice(
        &mut self,
        point_ref: &MapDataPointRef,
        choice_point_ref: &MapDataPointRef,
    ) -> () {
        let existing_choices = self.choices.get(point_ref);
        if let Some(mut existing_choices) = existing_choices.cloned() {
            existing_choices.insert(choice_point_ref.clone());
            self.choices.insert(point_ref.clone(), existing_choices);
        } else if existing_choices.is_none() {
            let mut ids = HashSet::new();
            ids.insert(choice_point_ref.clone());
            self.choices.insert(point_ref.clone(), ids);
        }
    }

    pub fn get_discarded_choices_for_point(
        &self,
        point_ref: &MapDataPointRef,
    ) -> Option<Vec<MapDataPointRef>> {
        match self.choices.get(point_ref) {
            None => None,
            Some(ids) => Some(ids.clone().into_iter().collect()),
        }
    }
}

#[derive(Clone)]
pub struct ForkWeights {
    weight_list: HashMap<MapDataPointRef, u32>,
}

impl ForkWeights {
    pub fn new() -> Self {
        Self {
            weight_list: HashMap::new(),
        }
    }
    pub fn add_calc_result(
        &mut self,
        choice_point_ref: &MapDataPointRef,
        weights: &Vec<WeightCalcResult>,
    ) -> () {
        if weights
            .iter()
            .all(|weight| *weight != WeightCalcResult::DoNotUse)
        {
            let existing_weight = match self.weight_list.get(choice_point_ref) {
                None => 0u32,
                Some(w) => w.clone(),
            };
            self.weight_list.insert(
                choice_point_ref.clone(),
                existing_weight
                    + weights
                        .into_iter()
                        .map(|r| match r {
                            WeightCalcResult::DoNotUse => 0u32,
                            WeightCalcResult::UseWithWeight(w) => w.clone() as u32,
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
    Stopped(Route),
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
    pub fn new(itinerary: Itinerary, rules: RouterRules, weight_calcs: Vec<WeightCalc>) -> Self {
        Self {
            walker: Walker::new(itinerary.start.clone()),
            itinerary,
            rules,
            weight_calcs,
            discarded_fork_choices: DiscardedForkChoices::new(),
        }
    }

    #[tracing::instrument(skip(self), fields(id = self.itinerary.id()))]
    pub fn generate_routes(mut self) -> NavigationResult {
        info!("Route gen for itinerary {}", self.itinerary);

        let mut loop_counter = 0;
        loop {
            loop_counter += 1;

            let move_result = self
                .walker
                .move_forward_to_next_fork(|p| self.itinerary.is_finished(p));

            DebugWriter::write_step(self.itinerary.id(), loop_counter, &move_result);

            if move_result == Ok(WalkerMoveResult::Finish) {
                return NavigationResult::Finished(self.walker.get_route().clone());
            }
            if let Ok(WalkerMoveResult::Fork(fork_choices)) = move_result {
                let (fork_choices, last_point) = {
                    let last_point = self.walker.get_last_point();
                    (
                        fork_choices.exclude_segments_where_points_in(
                            &self
                                .discarded_fork_choices
                                .get_discarded_choices_for_point(&last_point)
                                .map_or(Vec::new(), |d| d),
                        ),
                        last_point,
                    )
                };

                self.itinerary.check_set_next(last_point.clone());

                let fork_weights = fork_choices.clone().into_iter().fold(
                    ForkWeights::new(),
                    |mut fork_weights, fork_route_segment| {
                        let fork_weight_calc_results = self
                            .weight_calcs
                            .iter()
                            .map(|weight_calc| {
                                let weight_calc_result = weight_calc(WeightCalcInput {
                                    route: self.walker.get_route(),
                                    itinerary: &self.itinerary,
                                    current_fork_segment: &fork_route_segment,
                                    all_fork_segments: &fork_choices,
                                    walker_from_fork: Walker::new(
                                        fork_route_segment.get_end_point().clone(),
                                    ),
                                    rules: &self.rules,
                                });
                                trace!(result = debug(&weight_calc_result), "Weight calc");
                                weight_calc_result
                            })
                            .collect::<Vec<_>>();

                        fork_weights.add_calc_result(
                            &fork_route_segment.get_end_point(),
                            &fork_weight_calc_results,
                        );

                        fork_weights
                    },
                );

                trace!(fork_weights = debug(&fork_weights), "Fork weights");
                let chosen_fork_point = fork_weights.get_choice_id_by_index_from_heaviest(0);

                if let Some(chosen_fork_point) = chosen_fork_point {
                    self.discarded_fork_choices
                        .add_discarded_choice(&last_point, &chosen_fork_point);
                    self.walker.set_fork_choice_point_ref(chosen_fork_point);
                } else {
                    self.walker.move_backwards_to_prev_fork();
                    if self.walker.get_route().get_junction_before_last_segment() == None {
                        info!("Stuck");
                        self.walker.get_route().write_debug();
                        return NavigationResult::Stuck;
                    }
                }
            } else if move_result == Ok(WalkerMoveResult::DeadEnd) {
                self.walker.move_backwards_to_prev_fork();
            }

            if loop_counter >= 1000000 {
                info!("Reached loop {loop_counter}, stopping");
                return NavigationResult::Stopped(self.walker.get_route().clone());
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
            weights::WeightCalcInput,
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
                    return WeightCalcResult::UseWithWeight(10);
                }
                WeightCalcResult::UseWithWeight(1)
            }
            set_graph_static(graph_from_test_dataset(test_dataset_1()));
            let from = MapDataGraph::get().test_get_point_ref_by_id(&1).unwrap();
            let to = MapDataGraph::get().test_get_point_ref_by_id(&7).unwrap();
            let itinerary = Itinerary::new_start_finish(from, to, Vec::new(), 0.);
            let mut navigator = Navigator::new(itinerary.clone(), RouterRules::default(), vec![weight]);
            let route = match navigator.generate_routes() {
                crate::router::navigator::NavigationResult::Finished(r) => r,
                _ => {
                    assert!(false);
                    return ();
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
                    return WeightCalcResult::UseWithWeight(10);
                }
                WeightCalcResult::UseWithWeight(1)
            }
            let navigator = Navigator::new(itinerary,RouterRules::default(), vec![weight2]);
            let route = match navigator.generate_routes() {
                crate::router::navigator::NavigationResult::Finished(r) => r,
                _ => {
                    assert!(false);
                    return ();
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
                        return WeightCalcResult::UseWithWeight(10);
                    }
                    if input.current_fork_segment.get_end_point().borrow().id == 6 {
                        return WeightCalcResult::UseWithWeight(5);
                    }
                }
                if prev_point.borrow().id == 6
                    && input.current_fork_segment.get_end_point().borrow().id == 7
                {
                    return WeightCalcResult::UseWithWeight(10);
                }
                WeightCalcResult::UseWithWeight(1)
            }
            set_graph_static(graph_from_test_dataset(test_dataset_1()));
            let from = MapDataGraph::get().test_get_point_ref_by_id(&1).unwrap();
            let to = MapDataGraph::get().test_get_point_ref_by_id(&7).unwrap();
            let itinerary = Itinerary::new_start_finish(from, to, Vec::new(), 0.);
            let navigator = Navigator::new(itinerary, RouterRules::default(), vec![weight]);
            let route = match navigator.generate_routes() {
                crate::router::navigator::NavigationResult::Finished(r) => r,
                _ => {
                    assert!(false);
                    return ();
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
                WeightCalcResult::UseWithWeight(1)
            }
            set_graph_static(graph_from_test_dataset(test_dataset_1()));
            let from = MapDataGraph::get().test_get_point_ref_by_id(&1).unwrap();
            let to = MapDataGraph::get().test_get_point_ref_by_id(&11).unwrap();
            let itinerary = Itinerary::new_start_finish(from, to, Vec::new(), 0.);
            let navigator = Navigator::new(itinerary, RouterRules::default(), vec![weight]);

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
                    return WeightCalcResult::DoNotUse;
                }
                WeightCalcResult::UseWithWeight(1)
            }
            set_graph_static(graph_from_test_dataset(test_dataset_1()));
            let from = MapDataGraph::get().test_get_point_ref_by_id(&1).unwrap();
            let to = MapDataGraph::get().test_get_point_ref_by_id(&7).unwrap();
            let itinerary = Itinerary::new_start_finish(from, to, Vec::new(), 0.);
            let navigator = Navigator::new(itinerary, RouterRules::default(), vec![weight]);
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
                    return WeightCalcResult::UseWithWeight(10);
                }
                WeightCalcResult::UseWithWeight(6)
            }
            fn weight2(input: WeightCalcInput) -> WeightCalcResult {
                let prev_point = match input.route.get_segment_last() {
                    Some(segment) => segment.get_end_point(),
                    None => &input.itinerary.finish.clone(),
                };

                if prev_point.borrow().id == 3
                    && input.current_fork_segment.get_end_point().borrow().id == 6
                {
                    return WeightCalcResult::UseWithWeight(1);
                }
                WeightCalcResult::UseWithWeight(6)
            }
            set_graph_static(graph_from_test_dataset(test_dataset_1()));
            let from = MapDataGraph::get().test_get_point_ref_by_id(&1).unwrap();
            let to = MapDataGraph::get().test_get_point_ref_by_id(&7).unwrap();
            let itinerary = Itinerary::new_start_finish(from, to, Vec::new(), 0.);
            let navigator = Navigator::new(itinerary, RouterRules::default(), vec![weight1, weight2]);
            let route = match navigator.generate_routes() {
                crate::router::navigator::NavigationResult::Finished(r) => r,
                _ => {
                    assert!(false);
                    return ();
                }
            };
            assert!(route_matches_ids(route.clone(), vec![2, 3, 4, 8, 6, 7]));
        }
    }
}
