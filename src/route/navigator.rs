use crate::map_data_graph::{MapDataGraph, MapDataPoint};

use super::walker::{RouteElement, RouteWalker};

pub struct RouteNavigator<'a> {
    map_data_graph: &'a MapDataGraph,
    walkers: [RouteWalker<'a>; 1],
    start: &'a MapDataPoint,
    end: &'a MapDataPoint,
}

impl<'a> RouteNavigator<'a> {
    pub fn new(
        map_data_graph: &'a MapDataGraph,
        start: &'a MapDataPoint,
        end: &'a MapDataPoint,
    ) -> Self {
        RouteNavigator {
            map_data_graph,
            walkers: [RouteWalker::new(map_data_graph, start, end)],
            start,
            end,
        }
    }

    pub fn generate_routes() -> [Vec<RouteElement>] {}
}
