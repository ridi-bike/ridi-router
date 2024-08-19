use std::rc::Rc;

use crate::map_data_graph::{MapDataGraph, MapDataPointRef};

use super::{
    itinerary::Itinerary,
    navigator::Navigator,
    route::Route,
    weights::{
        weight_check_distance_to_next, weight_heading, weight_no_loops, weight_prefer_same_road,
        weight_progress_speed,
    },
};

pub struct Generator<'a> {
    map_data: &'a MapDataGraph,
    from: MapDataPointRef,
    to: MapDataPointRef,
}

impl<'a> Generator<'a> {
    pub fn new(map_data: &'a MapDataGraph, from: MapDataPointRef, to: MapDataPointRef) -> Self {
        Self { map_data, from, to }
    }

    fn generate_itineraries(&self) -> Vec<Itinerary> {
        vec![Itinerary::new(
            Rc::clone(&self.from),
            Rc::clone(&self.to),
            Vec::new(),
            10.,
        )]
    }

    pub fn generate_routes(self) -> Vec<Route> {
        let itineraries = self.generate_itineraries();
        itineraries
            .into_iter()
            .map(|itinerary| {
                Navigator::new(
                    &self.map_data,
                    itinerary,
                    vec![
                        weight_check_distance_to_next,
                        weight_prefer_same_road,
                        weight_no_loops,
                        weight_heading,
                        weight_progress_speed,
                    ],
                )
                .generate_routes()
            })
            .filter_map(|nav_route| match nav_route {
                super::navigator::NavigatorRoute::Stuck(_) => None,
                super::navigator::NavigatorRoute::Finished(route) => Some(route),
            })
            .collect::<Vec<_>>()
    }
}
