use std::collections::{HashMap, HashSet};

use crate::map_data_graph::{MapDataGraph, MapDataLine, MapDataPoint};

use super::walker::{Route, RouteSegment, RouteWalker, RouteWalkerMoveResult};

pub enum WeightCalcPreviousSegment<'a> {
    Start {
        point: &'a MapDataPoint,
    },
    Step {
        line: &'a MapDataLine,
        end_point: &'a MapDataPoint,
    },
}

impl WeightCalcPreviousSegment<'_> {
    pub fn get_point(&self) -> &MapDataPoint {
        match self {
            WeightCalcPreviousSegment::Start { point } => point,
            WeightCalcPreviousSegment::Step { line: _, end_point } => end_point,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum WeightCalcResult {
    UseWithWeight(u8),
    DoNotUse,
}

type ForkWeightCalc =
    fn(prev_segment: &WeightCalcPreviousSegment, to: &RouteSegment) -> WeightCalcResult;

#[derive(Debug)]
struct DiscardedForkChoices {
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

struct ForkWeights {
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

    pub fn get_choice_id_by_index_from_heaviest(&self, idx: usize) -> Option<u64> {
        let mut vec = self.weight_list.iter().collect::<Vec<_>>();
        vec.sort_by(|v, v2| v2.1.cmp(v.1));
        vec.get(idx).map(|w| w.0).copied()
    }
}

pub struct RouteNavigator<'a> {
    map_data_graph: &'a MapDataGraph,
    walkers: Vec<RouteWalker<'a>>,
    weight_calcs: Vec<ForkWeightCalc>,
    start: &'a MapDataPoint,
    end: &'a MapDataPoint,
    discarded_fork_choices: DiscardedForkChoices,
}

impl<'a> RouteNavigator<'a> {
    pub fn new(
        map_data_graph: &'a MapDataGraph,
        start: &'a MapDataPoint,
        end: &'a MapDataPoint,
        weight_calcs: Vec<ForkWeightCalc>,
    ) -> Self {
        RouteNavigator {
            map_data_graph,
            walkers: vec![RouteWalker::new(map_data_graph, start, end)],
            start,
            end,
            weight_calcs,
            discarded_fork_choices: DiscardedForkChoices::new(),
        }
    }

    pub fn generate_routes(&mut self) -> Vec<Route> {
        let mut stuck_walkers_idx = Vec::new();
        loop {
            self.walkers
                .iter_mut()
                .enumerate()
                .for_each(|(walker_idx, walker)| {
                    let move_result = walker.move_forward_to_next_fork();
                    if move_result == Ok(RouteWalkerMoveResult::Finish) {
                        return ();
                    }
                    if let Ok(RouteWalkerMoveResult::Fork(fork_choices)) = move_result {
                        let last_element = walker.get_route().get_segment_last();
                        let (prev_segment, last_point_id) = match last_element {
                            None => (
                                WeightCalcPreviousSegment::Start { point: &self.start },
                                &self.start.id,
                            ),
                            Some(route_segment) => (
                                WeightCalcPreviousSegment::Step {
                                    line: route_segment.get_line(),
                                    end_point: route_segment.get_end_point(),
                                },
                                &route_segment.get_end_point().id,
                            ),
                        };
                        let fork_choices = fork_choices.exclude_segments_where_point_ids_in(
                            &self
                                .discarded_fork_choices
                                .get_discarded_choices_for_pont(&last_point_id)
                                .map_or(Vec::new(), |d| d.clone()),
                        );
                        let fork_weights = fork_choices.into_iter().fold(
                            ForkWeights::new(),
                            |mut fork_weights, fork_route_segment| {
                                let fork_weight_calc_results = self
                                    .weight_calcs
                                    .iter()
                                    .map(|weight_calc| {
                                        weight_calc(&prev_segment, &fork_route_segment)
                                    })
                                    .collect::<Vec<_>>();
                                fork_weights.add_calc_result(
                                    &fork_route_segment.get_end_point().id,
                                    &fork_weight_calc_results,
                                );
                                fork_weights
                            },
                        );
                        let chosen_fork_point_id =
                            fork_weights.get_choice_id_by_index_from_heaviest(0);

                        if let Some(chosen_fork_point_id) = chosen_fork_point_id {
                            self.discarded_fork_choices
                                .add_discarded_choice(last_point_id, &chosen_fork_point_id);
                            walker.set_fork_choice_point_id(&chosen_fork_point_id);
                        } else {
                            if walker.get_route().get_fork_before_last_segment() == None {
                                stuck_walkers_idx.push(walker_idx);
                            }
                            walker.move_backwards_to_prev_fork();
                        }
                    } else if move_result == Ok(RouteWalkerMoveResult::DeadEnd) {
                        walker.move_backwards_to_prev_fork();
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
    use crate::{
        route::navigator::{WeightCalcPreviousSegment, WeightCalcResult},
        test_utils::{get_point_with_id, get_test_map_data_graph, route_matches_ids},
    };

    use super::RouteNavigator;

    #[test]
    fn navigate_pick_best() {
        let map_data = get_test_map_data_graph();
        let start = get_point_with_id(1);
        let end = get_point_with_id(7);
        let mut navigator = RouteNavigator::new(
            &map_data,
            &start,
            &end,
            vec![|prev_segment, choices| {
                let from_point = match prev_segment {
                    WeightCalcPreviousSegment::Start { point } => point,
                    WeightCalcPreviousSegment::Step { line: _, end_point } => end_point,
                };
                if from_point.id == 3 && choices.get_end_point().id == 6 {
                    return WeightCalcResult::UseWithWeight(10);
                }
                WeightCalcResult::UseWithWeight(1)
            }],
        );
        let routes = navigator.generate_routes();
        let route = routes.get(0);
        let route = if let Some(r) = route {
            r
        } else {
            assert!(false);
            return ();
        };

        assert!(route_matches_ids(route.clone(), vec![2, 3, 6, 7]));

        let mut navigator = RouteNavigator::new(
            &map_data,
            &start,
            &end,
            vec![|prev_segment, choices| {
                let from_point = match prev_segment {
                    WeightCalcPreviousSegment::Start { point } => point,
                    WeightCalcPreviousSegment::Step { line: _, end_point } => end_point,
                };
                if from_point.id == 3 && choices.get_end_point().id == 4 {
                    return WeightCalcResult::UseWithWeight(10);
                }
                WeightCalcResult::UseWithWeight(1)
            }],
        );
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
        let map_data = get_test_map_data_graph();
        let start = get_point_with_id(1);
        let end = get_point_with_id(7);
        let mut navigator = RouteNavigator::new(
            &map_data,
            &start,
            &end,
            vec![|prev_segment, choices| {
                let from_point = match prev_segment {
                    WeightCalcPreviousSegment::Start { point } => point,
                    WeightCalcPreviousSegment::Step { line: _, end_point } => end_point,
                };
                if from_point.id == 3 {
                    if choices.get_end_point().id == 5 {
                        return WeightCalcResult::UseWithWeight(10);
                    }
                    if choices.get_end_point().id == 6 {
                        return WeightCalcResult::UseWithWeight(5);
                    }
                }
                if from_point.id == 6 && choices.get_end_point().id == 7 {
                    return WeightCalcResult::UseWithWeight(10);
                }
                WeightCalcResult::UseWithWeight(1)
            }],
        );
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
        let map_data = get_test_map_data_graph();
        let start = get_point_with_id(1);
        let end = get_point_with_id(11);
        let mut navigator = RouteNavigator::new(
            &map_data,
            &start,
            &end,
            vec![|_prev_segment, _choice| WeightCalcResult::UseWithWeight(1)],
        );
        let routes = navigator.generate_routes();
        assert_eq!(routes.len(), 0);
    }

    #[test]
    fn navigate_no_routes_with_do_not_use_weight() {
        let map_data = get_test_map_data_graph();
        let start = get_point_with_id(1);
        let end = get_point_with_id(7);
        let mut navigator = RouteNavigator::new(
            &map_data,
            &start,
            &end,
            vec![|_prev_segment, choice| {
                if choice.get_end_point().id == 7 {
                    return WeightCalcResult::DoNotUse;
                }
                WeightCalcResult::UseWithWeight(1)
            }],
        );
        let routes = navigator.generate_routes();
        assert_eq!(routes.len(), 0);
    }

    #[test]
    fn navigate_on_weight_sum() {
        let map_data = get_test_map_data_graph();
        let start = get_point_with_id(1);
        let end = get_point_with_id(7);
        let mut navigator = RouteNavigator::new(
            &map_data,
            &start,
            &end,
            vec![
                |prev_segment, choice| {
                    let prev_point = match prev_segment {
                        WeightCalcPreviousSegment::Start { point } => point,
                        WeightCalcPreviousSegment::Step { line: _, end_point } => end_point,
                    };
                    if prev_point.id == 3 && choice.get_end_point().id == 6 {
                        return WeightCalcResult::UseWithWeight(10);
                    }
                    WeightCalcResult::UseWithWeight(6)
                },
                |prev_segment, choice| {
                    let prev_point = match prev_segment {
                        WeightCalcPreviousSegment::Start { point } => point,
                        WeightCalcPreviousSegment::Step { line: _, end_point } => end_point,
                    };
                    if prev_point.id == 3 && choice.get_end_point().id == 6 {
                        return WeightCalcResult::UseWithWeight(1);
                    }
                    WeightCalcResult::UseWithWeight(6)
                },
            ],
        );
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
