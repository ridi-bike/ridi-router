use std::collections::HashMap;

use crate::map_data_graph::{MapDataGraph, MapDataLine, MapDataPoint};

use super::walker::{RouteElement, RouteWalker, RouteWalkerMoveResult};

pub enum WeightCalcPreviousElement<'a> {
    Start(&'a MapDataPoint),
    Step {
        line: &'a MapDataLine,
        point: &'a MapDataPoint,
    },
}

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
    weight_cache: HashMap<u64, Vec<(u64, u32)>>,
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
            weight_cache: HashMap::new(),
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
                        let cached_fork_weights = self.weight_cache.get(&last_point_id);
                        eprintln!("cached fork weights {:#?}", self.weight_cache);

                        let chosen_fork = if let Some(cached_fork_weights) = cached_fork_weights {
                            eprintln!(
                                "eval stuck - weight len {:?}, walker last {:?}",
                                cached_fork_weights.len(),
                                walker.get_route().last()
                            );
                            if cached_fork_weights.len() == 0 && walker.get_route().last() == None {
                                eprintln!("pushed to stuck {}", walker_idx);
                                stuck_walkers_idx.push(walker_idx);
                            }
                            let mut cached_fork_weights = cached_fork_weights.clone();
                            let choice = cached_fork_weights.pop();
                            self.weight_cache
                                .insert(last_point_id.clone(), cached_fork_weights);

                            choice
                        } else {
                            let mut fork_weights = fork_choices
                                .iter()
                                .map(|f| {
                                    (
                                        f.1.id,
                                        self.weight_calcs
                                            .iter()
                                            .map(|weight_calc| weight_calc(&prev_element, f))
                                            .filter_map(|weight_result| {
                                                if let WeightCalcResult::UseWithWeight(weight) =
                                                    weight_result
                                                {
                                                    return Some(weight as u32);
                                                }
                                                None
                                            })
                                            .sum::<u32>(),
                                    )
                                })
                                .collect::<Vec<_>>();
                            fork_weights.sort_by(|w1, w2| w1.1.cmp(&w2.1));
                            let choice = fork_weights.pop();
                            eprintln!(
                                "insert weight cache {:?}:{:#?}",
                                last_point_id, fork_weights
                            );
                            eprintln!("choice {:#?}", &choice);
                            self.weight_cache
                                .insert(last_point_id.clone(), fork_weights);

                            choice
                        };

                        if let Some(chosen_fork) = chosen_fork {
                            eprintln!("moving to {:#?}", chosen_fork);
                            walker.set_fork_choice_point_id(&chosen_fork.0);
                        } else {
                            eprintln!("moving back");
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

            if loop_count > 10 {
                panic!("loop count {}", loop_count);
            }
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
        // TODO
        // 1. cached weights will in a loop scenario will offer a cached weight for a way that's
        //    actually backwards. we need to check for cached weight and walker choice intersection
        //    as the walker won't offer backwards fork choices
        // 2. a point will be marked as a fork incorrectly if two ways overlap but don't actually
        //    introduce a fork - way 1 - 2 - 3 and way 3 - 4 - 5 will mark point 3 as a fork even
        //    though it's not
        let map_data = get_test_map_data_graph();
        let start = get_point_with_id(1);
        let end = get_point_with_id(11);
        let mut navigator = RouteNavigator::new(
            &map_data,
            &start,
            &end,
            vec![|_prev_element, _choices| WeightCalcResult::UseWithWeight(1)],
        );
        let routes = navigator.generate_routes();
        assert_eq!(routes.len(), 0);
    }
}
