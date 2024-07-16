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
