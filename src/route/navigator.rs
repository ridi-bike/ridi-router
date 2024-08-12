use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    rc::Rc,
};

use crate::{
    debug_writer::{DebugLoggerFileSink, DebugLoggerVoidSink},
    map_data_graph::{MapDataGraph, MapDataPointRef},
};

use super::{
    walker::{RouteWalker, RouteWalkerMoveResult},
    weights::{WeightCalc, WeightCalcInput},
    Route,
};

#[derive(Debug, Clone, PartialEq)]
pub enum WeightCalcResult {
    UseWithWeight(u8),
    DoNotUse,
}

#[derive(Debug)]
pub struct DiscardedForkChoices {
    choices: HashMap<u64, HashSet<u64>>,
}
impl DiscardedForkChoices {
    pub fn new() -> Self {
        Self {
            choices: HashMap::new(),
        }
    }

    pub fn add_discarded_choice(&mut self, point_id: &u64, choice_point_id: &u64) -> () {
        let existing_choices = self.choices.get(point_id);
        if let Some(mut existing_choices) = existing_choices.cloned() {
            existing_choices.insert(choice_point_id.clone());
            self.choices.insert(point_id.clone(), existing_choices);
        } else if existing_choices.is_none() {
            let mut ids = HashSet::new();
            ids.insert(choice_point_id.clone());
            self.choices.insert(point_id.clone(), ids);
        }
    }

    pub fn get_discarded_choices_for_pont(&self, point_id: &u64) -> Option<Vec<u64>> {
        match self.choices.get(point_id) {
            None => None,
            Some(ids) => Some(ids.clone().into_iter().collect()),
        }
    }
}

#[derive(Clone)]
pub struct ForkWeights {
    weight_list: HashMap<u64, u32>,
}

impl ForkWeights {
    pub fn new() -> Self {
        Self {
            weight_list: HashMap::new(),
        }
    }
    pub fn add_calc_result(
        &mut self,
        choice_point_id: &u64,
        weights: &Vec<WeightCalcResult>,
    ) -> () {
        if weights
            .iter()
            .all(|weight| *weight != WeightCalcResult::DoNotUse)
        {
            let existing_weight = match self.weight_list.get(choice_point_id) {
                None => 0u32,
                Some(w) => w.clone(),
            };
            self.weight_list.insert(
                choice_point_id.clone(),
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

    fn get_choices_sorted_by_weight(&self) -> Vec<(&u64, &u32)> {
        let mut vec = self.weight_list.iter().collect::<Vec<_>>();
        vec.sort_by(|v, v2| v2.1.cmp(v.1));
        vec
    }

    pub fn get_choice_id_by_index_from_heaviest(&self, idx: usize) -> Option<u64> {
        let vec = self.get_choices_sorted_by_weight();
        vec.get(idx).map(|w| w.0).copied()
    }

    pub fn get_number_of_top_choices(&self) -> usize {
        let top_weight = match self.get_choice_id_by_index_from_heaviest(0) {
            None => return 0,
            Some(ch) => match self.weight_list.get(&ch) {
                None => return 0,
                Some(w) => *w,
            },
        };

        self.get_choices_sorted_by_weight()
            .iter()
            .fold(0, |acc, w| {
                if *w.1 >= ((top_weight as f64) * 0.7) as u32 {
                    return acc + 1;
                }
                acc
            })
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

pub struct RouteNavigator<'a> {
    map_data_graph: &'a MapDataGraph,
    walkers: Vec<RouteWalker<'a>>,
    weight_calcs: Vec<WeightCalc>,
    start: MapDataPointRef,
    end: MapDataPointRef,
    discarded_fork_choices: DiscardedForkChoices,
}

impl<'a> RouteNavigator<'a> {
    pub fn new(
        map_data_graph: &'a MapDataGraph,
        start: MapDataPointRef,
        end: MapDataPointRef,
        weight_calcs: Vec<WeightCalc>,
    ) -> Self {
        RouteNavigator {
            map_data_graph,
            walkers: vec![RouteWalker::new(
                map_data_graph,
                Rc::clone(&start),
                Rc::clone(&end),
                Box::new(DebugLoggerFileSink::new(
                    1,
                    Rc::clone(&start),
                    Rc::clone(&end),
                )),
                // Box::new(DebugLoggerVoidSink::default()),
            )],
            start,
            end,
            weight_calcs,
            discarded_fork_choices: DiscardedForkChoices::new(),
        }
    }

    pub fn generate_routes(&mut self) -> Vec<Route> {
        let mut stuck_walkers_idx = Vec::new();
        let mut new_walkers = Vec::new();
        loop {
            while let Some(new_walker) = new_walkers.pop() {
                self.walkers.push(new_walker);
            }
            self.walkers
                .iter_mut()
                .enumerate()
                .for_each(|(walker_idx, walker)| {
                    walker.debug_logger.log_step();

                    let move_result = walker.move_forward_to_next_fork();
                    if let Ok(move_result) = &move_result {
                        walker
                            .debug_logger
                            .log_move(move_result, &walker.get_route().clone());
                    }

                    if move_result == Ok(RouteWalkerMoveResult::Finish) {
                        return ();
                    }
                    if let Ok(RouteWalkerMoveResult::Fork(fork_choices)) = move_result {
                        walker
                            .debug_logger
                            .log(format!("choices: {:#?}", fork_choices));
                        let (fork_choices, last_point_id) = {
                            let last_element = walker.get_route().get_segment_last();
                            let last_point_id = match last_element {
                                None => self.start.borrow().id,
                                Some(route_segment) => route_segment.get_end_point().borrow().id,
                            };
                            walker.debug_logger.log(format!(
                                "discarded choices: {:#?}",
                                self.discarded_fork_choices
                                    .get_discarded_choices_for_pont(&last_point_id)
                            ));
                            (
                                fork_choices.exclude_segments_where_points_in(
                                    &self
                                        .discarded_fork_choices
                                        .get_discarded_choices_for_pont(&last_point_id)
                                        .map_or(Vec::new(), |d| {
                                            d.iter()
                                                .filter_map(|p| {
                                                    self.map_data_graph.get_point_by_id(&p)
                                                })
                                                .collect()
                                        }),
                                ),
                                last_point_id,
                            )
                        };
                        let fork_weights = fork_choices.clone().into_iter().fold(
                            ForkWeights::new(),
                            |mut fork_weights, fork_route_segment| {
                                let fork_weight_calc_results = self
                                    .weight_calcs
                                    .iter()
                                    .map(|weight_calc| {
                                        weight_calc(WeightCalcInput {
                                            route: walker.get_route(),
                                            start_point: Rc::clone(&self.start),
                                            end_point: Rc::clone(&self.end),
                                            eval_fork_segment: &fork_route_segment,
                                            all_fork_segments: &fork_choices,
                                            fork_walker: RouteWalker::new(
                                                self.map_data_graph,
                                                Rc::clone(&fork_route_segment.get_end_point()),
                                                Rc::clone(&self.end),
                                                Box::new(DebugLoggerVoidSink::default()),
                                            ),
                                        })
                                    })
                                    .collect::<Vec<_>>();
                                walker.debug_logger.log(format!(
                                    "weight: {:#?}\n\t{:#?}",
                                    fork_route_segment.get_end_point().borrow().id,
                                    fork_weight_calc_results.clone(),
                                ));

                                fork_weights.add_calc_result(
                                    &fork_route_segment.get_end_point().borrow().id,
                                    &fork_weight_calc_results,
                                );

                                walker.debug_logger.log(format!(
                                    "calc result: {:#?}\n\t{:#?}",
                                    fork_route_segment.get_end_point().borrow().id,
                                    &fork_weight_calc_results,
                                ));
                                fork_weights
                            },
                        );
                        walker
                            .debug_logger
                            .log(format!("all weights: {:#?}", &fork_weights));

                        let mut choice_idx = 0;
                        if fork_weights.get_number_of_top_choices() == 0 {
                            let move_back_segment_list = walker.move_backwards_to_prev_fork();
                            let last_segment =
                                walker.get_route().get_segment_last().map(|s| s.clone());
                            walker.debug_logger.log(format!(
                                "fork action go back:\n\tlast:\n\t\t{:#?}\n\tfork:\n\t\t{:#?}",
                                last_segment, move_back_segment_list
                            ));
                            if walker.get_route().get_junction_before_last_segment() == None {
                                stuck_walkers_idx.push(walker_idx);
                            }
                        }
                        while choice_idx < fork_weights.get_number_of_top_choices() {
                            let chosen_fork_point = fork_weights
                                .get_choice_id_by_index_from_heaviest(choice_idx)
                                .map(|pid| self.map_data_graph.get_point_by_id(&pid))
                                .flatten();

                            if let Some(chosen_fork_point) = chosen_fork_point {
                                self.discarded_fork_choices.add_discarded_choice(
                                    &last_point_id,
                                    &chosen_fork_point.borrow().id,
                                );
                                walker.debug_logger.log(format!(
                                    "fork action choice: {:#?}",
                                    chosen_fork_point.borrow().id
                                ));
                                if choice_idx == 0 {
                                    walker.set_fork_choice_point_id(chosen_fork_point);
                                } else {
                                    let mut split_walker = walker.split();
                                    split_walker.set_fork_choice_point_id(chosen_fork_point);
                                    new_walkers.push(split_walker)
                                }
                            } else {
                                let move_back_segment_list = walker.move_backwards_to_prev_fork();
                                let last_segment =
                                    walker.get_route().get_segment_last().map(|s| s.clone());
                                walker.debug_logger.log(format!(
                                    "fork action go back:\n\tlast:\n\t\t{:#?}\n\tfork:\n\t\t{:#?}",
                                    last_segment, move_back_segment_list
                                ));
                                if walker.get_route().get_junction_before_last_segment() == None {
                                    stuck_walkers_idx.push(walker_idx);
                                }
                            }
                            choice_idx += 1;
                        }
                    } else if move_result == Ok(RouteWalkerMoveResult::DeadEnd) {
                        let move_back_segment_list = walker.move_backwards_to_prev_fork();
                        let last_segment = walker.get_route().get_segment_last().map(|s| s.clone());
                        walker.debug_logger.log(format!(
                            "fork action go back:\n\tlast:\n\t\t{:#?}\n\tfork:\n\t\t{:#?}",
                            last_segment, move_back_segment_list
                        ));
                    }
                });
            while let Some(&walker_idx) = stuck_walkers_idx.last() {
                stuck_walkers_idx.pop();
                self.walkers.remove(walker_idx);
            }
            if self.walkers.len() == 0
                || self
                    .walkers
                    .iter_mut()
                    .all(|w| w.move_forward_to_next_fork() == Ok(RouteWalkerMoveResult::Finish))
            {
                break;
            }
        }

        self.walkers.iter().map(|w| w.get_route().clone()).collect()
    }
}

#[cfg(test)]
mod test {
    use std::rc::Rc;

    use crate::{
        route::{navigator::WeightCalcResult, weights::WeightCalcInput},
        test_utils::{get_test_map_data_graph, route_matches_ids},
    };

    use super::RouteNavigator;

    #[test]
    fn navigate_pick_best() {
        fn weight(input: WeightCalcInput) -> WeightCalcResult {
            let prev_point = match input.route.get_segment_last() {
                Some(segment) => segment.get_end_point(),
                None => &input.end_point,
            };
            if prev_point.borrow().id == 3
                && input.eval_fork_segment.get_end_point().borrow().id == 6
            {
                return WeightCalcResult::UseWithWeight(10);
            }
            WeightCalcResult::UseWithWeight(1)
        }
        let map_data = get_test_map_data_graph();
        let start = map_data.get_point_by_id(&1).unwrap();
        let end = map_data.get_point_by_id(&7).unwrap();
        let mut navigator =
            RouteNavigator::new(&map_data, Rc::clone(&start), Rc::clone(&end), vec![weight]);
        let routes = navigator.generate_routes();
        let route = routes.get(0);
        let route = if let Some(r) = route {
            r
        } else {
            assert!(false);
            return ();
        };

        assert!(route_matches_ids(route.clone(), vec![2, 3, 6, 7]));

        fn weight2(input: WeightCalcInput) -> WeightCalcResult {
            let prev_point = match input.route.get_segment_last() {
                Some(segment) => segment.get_end_point(),
                None => &input.end_point,
            };

            if prev_point.borrow().id == 3
                && input.eval_fork_segment.get_end_point().borrow().id == 4
            {
                return WeightCalcResult::UseWithWeight(10);
            }
            WeightCalcResult::UseWithWeight(1)
        }
        let mut navigator = RouteNavigator::new(&map_data, start, end, vec![weight2]);
        let routes = navigator.generate_routes();
        let route = routes.get(0);
        let route = if let Some(r) = route {
            r
        } else {
            assert!(false);
            return ();
        };

        assert!(route_matches_ids(route.clone(), vec![2, 3, 4, 8, 6, 7]));
    }

    #[test]
    fn navigate_dead_end_pick_next_best() {
        fn weight(input: WeightCalcInput) -> WeightCalcResult {
            let prev_point = match input.route.get_segment_last() {
                Some(segment) => segment.get_end_point(),
                None => &input.end_point,
            };

            if prev_point.borrow().id == 3 {
                if input.eval_fork_segment.get_end_point().borrow().id == 5 {
                    return WeightCalcResult::UseWithWeight(10);
                }
                if input.eval_fork_segment.get_end_point().borrow().id == 6 {
                    return WeightCalcResult::UseWithWeight(5);
                }
            }
            if prev_point.borrow().id == 6
                && input.eval_fork_segment.get_end_point().borrow().id == 7
            {
                return WeightCalcResult::UseWithWeight(10);
            }
            WeightCalcResult::UseWithWeight(1)
        }
        let map_data = get_test_map_data_graph();
        let start = map_data.get_point_by_id(&1).unwrap();
        let end = map_data.get_point_by_id(&7).unwrap();
        let mut navigator = RouteNavigator::new(&map_data, start, end, vec![weight]);
        let routes = navigator.generate_routes();
        let route = routes.get(0);
        let route = if let Some(r) = route {
            r
        } else {
            assert!(false);
            return ();
        };

        assert!(route_matches_ids(route.clone(), vec![2, 3, 6, 7]));
    }

    #[test]
    fn navigate_all_stuck_return_no_routes() {
        fn weight(_input: WeightCalcInput) -> WeightCalcResult {
            WeightCalcResult::UseWithWeight(1)
        }
        let map_data = get_test_map_data_graph();
        let start = map_data.get_point_by_id(&1).unwrap();
        let end = map_data.get_point_by_id(&11).unwrap();
        let mut navigator = RouteNavigator::new(&map_data, start, end, vec![weight]);
        let routes = navigator.generate_routes();
        assert_eq!(routes.len(), 0);
    }

    #[test]
    fn navigate_no_routes_with_do_not_use_weight() {
        fn weight(input: WeightCalcInput) -> WeightCalcResult {
            if input.eval_fork_segment.get_end_point().borrow().id == 7 {
                return WeightCalcResult::DoNotUse;
            }
            WeightCalcResult::UseWithWeight(1)
        }
        let map_data = get_test_map_data_graph();
        let start = map_data.get_point_by_id(&1).unwrap();
        let end = map_data.get_point_by_id(&7).unwrap();
        let mut navigator = RouteNavigator::new(&map_data, start, end, vec![weight]);
        let routes = navigator.generate_routes();
        assert_eq!(routes.len(), 0);
    }

    #[test]
    fn navigate_on_weight_sum() {
        fn weight1(input: WeightCalcInput) -> WeightCalcResult {
            let prev_point = match input.route.get_segment_last() {
                Some(segment) => segment.get_end_point(),
                None => &input.end_point,
            };
            if prev_point.borrow().id == 3
                && input.eval_fork_segment.get_end_point().borrow().id == 6
            {
                return WeightCalcResult::UseWithWeight(10);
            }
            WeightCalcResult::UseWithWeight(6)
        }
        fn weight2(input: WeightCalcInput) -> WeightCalcResult {
            let prev_point = match input.route.get_segment_last() {
                Some(segment) => segment.get_end_point(),
                None => &input.end_point,
            };

            if prev_point.borrow().id == 3
                && input.eval_fork_segment.get_end_point().borrow().id == 6
            {
                return WeightCalcResult::UseWithWeight(1);
            }
            WeightCalcResult::UseWithWeight(6)
        }
        let map_data = get_test_map_data_graph();
        let start = map_data.get_point_by_id(&1).unwrap();
        let end = map_data.get_point_by_id(&7).unwrap();
        let mut navigator = RouteNavigator::new(&map_data, start, end, vec![weight1, weight2]);
        let routes = navigator.generate_routes();
        let route = routes.get(0);
        let route = if let Some(r) = route {
            r
        } else {
            assert!(false);
            return ();
        };
        assert!(route_matches_ids(route.clone(), vec![2, 3, 4, 8, 6, 7]));
    }
}
