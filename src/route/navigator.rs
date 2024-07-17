use std::collections::HashMap;

use crate::map_data_graph::{MapDataGraph, MapDataLine, MapDataPoint};

use super::walker::{RouteElement, RouteWalker, RouteWalkerMoveResult};

#[derive(Debug, PartialEq)]
pub enum RouterNavigatorError {
    NoChoicesForFork,
}

type ForkWeightCalc =
    fn(from_point: &MapDataPoint, from_line: Option<&MapDataLine>, to: &RouteElement) -> i8;

pub struct RouteNavigator<'a> {
    map_data_graph: &'a MapDataGraph,
    walkers: Vec<RouteWalker<'a>>,
    weights: Vec<ForkWeightCalc>,
    start: &'a MapDataPoint,
    end: &'a MapDataPoint,
    weight_cache: HashMap<u64, Vec<(u64, i32)>>,
}

impl<'a> RouteNavigator<'a> {
    pub fn new(
        map_data_graph: &'a MapDataGraph,
        start: &'a MapDataPoint,
        end: &'a MapDataPoint,
        weights: Vec<ForkWeightCalc>,
    ) -> Self {
        RouteNavigator {
            map_data_graph,
            walkers: vec![RouteWalker::new(map_data_graph, start, end)],
            start,
            end,
            weights,
            weight_cache: HashMap::new(),
        }
    }

    pub fn generate_routes(&mut self) -> Result<Vec<Vec<RouteElement>>, RouterNavigatorError> {
        loop {
            self.walkers.iter_mut().for_each(|walker| {
                let move_result = walker.move_forward_to_next_fork();
                if move_result == Ok(RouteWalkerMoveResult::Finish) {
                    return ();
                }
                if let Ok(RouteWalkerMoveResult::Fork(fork_choices)) = &move_result {
                    let last_element = walker.get_route().last();
                    let (last_line, last_point) = match last_element {
                        None => (None, self.start),
                        Some((line, point)) => (Some(line), point),
                    };
                    let cached_weights = self.weight_cache.get(&last_point.id);

                    let chosen_fork = if let Some(cached_weights) = cached_weights {
                        let mut cached_weights = cached_weights.clone();
                        let choice = cached_weights.pop();
                        self.weight_cache.insert(last_point.id, cached_weights);

                        choice
                    } else {
                        let mut fork_weights = fork_choices
                            .iter()
                            .map(|f| {
                                (
                                    f.1.id,
                                    self.weights
                                        .iter()
                                        .map(|w| w(last_point, last_line, f) as i32)
                                        .sum::<i32>(),
                                )
                            })
                            .collect::<Vec<_>>();
                        fork_weights.sort_by(|w1, w2| w1.1.cmp(&w2.1));
                        let chosen_fork = fork_weights.pop();
                        self.weight_cache.insert(last_point.id, fork_weights);

                        chosen_fork
                    };

                    if let Some(chosen_fork) = chosen_fork {
                        walker.set_fork_choice_point_id(&chosen_fork.0);
                    } else {
                        walker.move_backwards_to_prev_fork();
                    }
                } else if move_result == Ok(RouteWalkerMoveResult::DeadEnd) {
                    walker.move_backwards_to_prev_fork();
                }
            });
            if self
                .walkers
                .iter_mut()
                .all(|w| w.move_forward_to_next_fork() == Ok(RouteWalkerMoveResult::Finish))
            {
                break;
            }
        }

        Ok(self.walkers.iter().map(|w| w.get_route().clone()).collect())
    }
}

#[cfg(test)]
mod test {
    use crate::test_utils::{get_point_with_id, get_test_map_data_graph, route_matches_ids};

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
            vec![|from_point, _to_point, choices| {
                if from_point.id == 3 && choices.1.id == 6 {
                    return 10;
                }
                if from_point.id == 6 && choices.1.id == 7 {
                    return 10;
                }
                0
            }],
        );
        let routes = navigator.generate_routes();
        let routes = if let Ok(r) = routes {
            r
        } else {
            assert!(false);
            return ();
        };
        let route = routes.get(0);
        let route = if let Some(r) = route {
            r
        } else {
            assert!(false);
            return ();
        };

        eprint!("{:#?}", route);
        assert!(route_matches_ids(route.clone(), vec![2, 3, 6, 7]));
    }
}
