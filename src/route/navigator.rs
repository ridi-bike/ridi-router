use std::collections::{HashMap, HashSet};

use crate::map_data_graph::{MapDataGraph, MapDataLine, MapDataPoint};

use super::walker::{RouteElement, RouteWalker, RouteWalkerMoveResult};

pub enum WeightCalcPreviousElement<'a> {
    Start(&'a MapDataPoint),
    Step {
        line: &'a MapDataLine,
        point: &'a MapDataPoint,
    },
}

#[derive(Debug, PartialEq)]
pub enum WeightCalcResult {
    UseWithWeight(u8),
    DoNotUse,
}

type ForkWeightCalc =
    fn(prev_element: &WeightCalcPreviousElement, to: &RouteElement) -> WeightCalcResult;

pub struct RouteNavigator<'a> {
    map_data_graph: &'a MapDataGraph,
    walkers: Vec<RouteWalker<'a>>,
    weight_calcs: Vec<ForkWeightCalc>,
    start: &'a MapDataPoint,
    end: &'a MapDataPoint,
    discarded_fork_choices: HashSet<(u64, u64)>,
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
            discarded_fork_choices: HashSet::new(),
        }
    }

    pub fn generate_routes(&mut self) -> Vec<Vec<RouteElement>> {
        let mut loop_count = 0;
        let mut stuck_walkers_idx = Vec::new();
        loop {
            loop_count += 1;
            self.walkers
                .iter_mut()
                .enumerate()
                .for_each(|(walker_idx, walker)| {
                    let move_result = walker.move_forward_to_next_fork();
                    eprintln!("move result {:#?}", move_result);
                    if move_result == Ok(RouteWalkerMoveResult::Finish) {
                        return ();
                    }
                    if let Ok(RouteWalkerMoveResult::Fork(fork_choices)) = &move_result {
                        let last_element = walker.get_route().last();
                        let (prev_element, last_point_id) = match last_element {
                            None => (
                                WeightCalcPreviousElement::Start(&self.start),
                                &self.start.id,
                            ),
                            Some((line, point)) => {
                                (WeightCalcPreviousElement::Step { line, point }, &point.id)
                            }
                        };
                        // let cached_fork_weights = self.weight_cache.get(&last_point_id);
                        // eprintln!("cached fork weights {:#?}", self.weight_cache);
                        //
                        // let chosen_fork = if let Some(cached_fork_weights) = cached_fork_weights {
                        //     let mut cached_fork_weights = cached_fork_weights.clone();
                        //     let choice = cached_fork_weights.pop();
                        //     self.weight_cache
                        //         .insert(last_point_id.clone(), cached_fork_weights.clone());
                        //
                        //     eprintln!(
                        //         "eval stuck - weight len {:#?}, walker last {:#?}",
                        //         &cached_fork_weights.len(),
                        //         walker.get_route().last()
                        //     );
                        //     let cached_fork_weights = cached_fork_weights
                        //         .iter()
                        //         .filter_map(|weight| {
                        //             if fork_choices.iter().any(|c| c.1.id == weight.0) {
                        //                 return Some(*weight);
                        //             }
                        //             None
                        //         })
                        //         .collect::<Vec<_>>();
                        //     if cached_fork_weights.len() == 0
                        //         && walker
                        //             .get_route()
                        //             .iter()
                        //             .find(|r| r.1.fork == true && r.1.id != *last_point_id)
                        //             == None
                        //     {
                        //         eprintln!("pushed to stuck {}", walker_idx);
                        //         stuck_walkers_idx.push(walker_idx);
                        //     }
                        //
                        //     choice
                        // } else {
                        let mut fork_weights = fork_choices
                            .iter()
                            .filter(|f| {
                                !self
                                    .discarded_fork_choices
                                    .contains(&(*last_point_id, f.1.id))
                            })
                            .map(|f| {
                                (
                                    f.1.id,
                                    self.weight_calcs
                                        .iter()
                                        .map(|weight_calc| weight_calc(&prev_element, f)),
                                )
                            })
                            .filter_map(|(p_id, mut ws)| {
                                if ws.any(|w| w == WeightCalcResult::DoNotUse) {
                                    return None;
                                }
                                let ws_sum: u32 = ws.into_iter().fold(0u32, |acc, val| {
                                    if let WeightCalcResult::UseWithWeight(w) = val {
                                        return acc + w as u32;
                                    }
                                    0
                                });
                                Some((p_id, ws_sum))
                            })
                            .collect::<Vec<_>>();
                        eprintln!(
                            "unsorted weights for {:?} -  {:#?}",
                            last_point_id, fork_weights
                        );
                        fork_weights.sort_by(|w1, w2| w1.1.cmp(&w2.1));
                        // let choice = fork_weights.pop();
                        let chosen_fork = fork_weights.pop();
                        // eprintln!(
                        //     "insert weight cache {:?}:{:#?}",
                        //     last_point_id, fork_weights
                        // );
                        //     eprintln!("choice {:#?}", &choice);
                        //     self.weight_cache
                        //         .insert(last_point_id.clone(), fork_weights);
                        //
                        //     choice
                        // };

                        if let Some(chosen_fork) = chosen_fork {
                            eprintln!("moving to {:#?}", chosen_fork);
                            self.discarded_fork_choices
                                .insert((*last_point_id, chosen_fork.0));
                            walker.set_fork_choice_point_id(&chosen_fork.0);
                        } else {
                            eprintln!("moving back");
                            if walker
                                .get_route()
                                .iter()
                                .find(|r| r.1.fork == true && r.1.id != *last_point_id)
                                == None
                            {
                                eprintln!("pushed to stuck {}", walker_idx);
                                stuck_walkers_idx.push(walker_idx);
                            }
                            walker.move_backwards_to_prev_fork();
                        }
                    } else if move_result == Ok(RouteWalkerMoveResult::DeadEnd) {
                        walker.move_backwards_to_prev_fork();
                    }
                });
            while let Some(&walker_idx) = stuck_walkers_idx.last() {
                eprintln!("popped walker {walker_idx}");
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

            if loop_count > 20 {
                panic!("panic: loop count {}", loop_count);
            }
            eprintln!("loop num {}", loop_count);
        }

        self.walkers.iter().map(|w| w.get_route().clone()).collect()
    }
}

#[cfg(test)]
mod test {
    use crate::{
        route::navigator::{WeightCalcPreviousElement, WeightCalcResult},
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
            vec![|prev_element, choices| {
                let from_point = match prev_element {
                    WeightCalcPreviousElement::Start(point) => point,
                    WeightCalcPreviousElement::Step { line: _, point } => point,
                };
                if from_point.id == 3 && choices.1.id == 6 {
                    return WeightCalcResult::UseWithWeight(10);
                }
                if from_point.id == 6 && choices.1.id == 7 {
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
    fn navigate_dead_end_pick_next_best() {
        let map_data = get_test_map_data_graph();
        let start = get_point_with_id(1);
        let end = get_point_with_id(7);
        let mut navigator = RouteNavigator::new(
            &map_data,
            &start,
            &end,
            vec![|prev_element, choices| {
                let from_point = match prev_element {
                    WeightCalcPreviousElement::Start(point) => point,
                    WeightCalcPreviousElement::Step { line: _, point } => point,
                };
                if from_point.id == 3 {
                    if choices.1.id == 5 {
                        return WeightCalcResult::UseWithWeight(10);
                    }
                    if choices.1.id == 6 {
                        return WeightCalcResult::UseWithWeight(5);
                    }
                }
                if from_point.id == 6 && choices.1.id == 7 {
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
            vec![|_prev_element, _choice| WeightCalcResult::UseWithWeight(1)],
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
            vec![|_prev_element, choice| {
                if choice.1.id == 7 {
                    return WeightCalcResult::DoNotUse;
                }
                WeightCalcResult::UseWithWeight(1)
            }],
        );
        let routes = navigator.generate_routes();
        eprintln!("routes {:#?}", routes);
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
                |prev_element, choice| {
                    let prev_point = match prev_element {
                        WeightCalcPreviousElement::Step { line: _, point } => point,
                        WeightCalcPreviousElement::Start(point) => point,
                    };
                    if prev_point.id == 3 && choice.1.id == 6 {
                        return WeightCalcResult::UseWithWeight(10);
                    }
                    WeightCalcResult::UseWithWeight(6)
                },
                |prev_element, choice| {
                    let prev_point = match prev_element {
                        WeightCalcPreviousElement::Step { line: _, point } => point,
                        WeightCalcPreviousElement::Start(point) => point,
                    };
                    if prev_point.id == 3 && choice.1.id == 6 {
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
        eprintln!("route {:#?}", route);
        assert!(route_matches_ids(route.clone(), vec![2, 3, 4, 8, 6, 7]));
    }
}
