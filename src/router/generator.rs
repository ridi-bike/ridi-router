use crate::{
    hints::RouterHints,
    map_data::graph::{MapDataGraph, MapDataPointRef},
};
use geo::{HaversineDestination, Point};
use rayon::prelude::*;

use super::{
    itinerary::Itinerary,
    navigator::{NavigationResult, Navigator},
    route::Route,
    weights::{
        weight_check_distance_to_next, weight_heading, weight_hints_highway,
        weight_hints_smoothness, weight_hints_surface, weight_no_loops, weight_prefer_same_road,
    },
};

const ITINERARY_VARIATION_DISTANCES: [f32; 2] = [10000., 20000.];
const ITINERARY_VARIATION_DEGREES: [f32; 8] = [0., 45., 90., 135., 180., -45., -90., -135.];

pub struct Generator {
    start: MapDataPointRef,
    finish: MapDataPointRef,
    hints: RouterHints,
}

impl Generator {
    pub fn new(start: MapDataPointRef, finish: MapDataPointRef, hints: RouterHints) -> Self {
        Self {
            start,
            finish,
            hints,
        }
    }

    fn create_waypoints_around(&self, point: &MapDataPointRef) -> Vec<MapDataPointRef> {
        let point_geo = Point::new(point.borrow().lon, point.borrow().lat);
        ITINERARY_VARIATION_DEGREES
            .iter()
            .map(|bearing| {
                ITINERARY_VARIATION_DISTANCES
                    .iter()
                    .map(|distance| {
                        let wp_geo = point_geo.haversine_destination(*bearing, *distance);
                        let wp = MapDataGraph::get().get_closest_to_coords(wp_geo.y(), wp_geo.x());
                        wp
                    })
                    .filter_map(|maybe_wp| maybe_wp)
            })
            .flatten()
            .collect()
    }

    fn generate_itineraries(&self) -> Vec<Itinerary> {
        let from_waypoints = self.create_waypoints_around(&self.start);
        let to_waypoints = self.create_waypoints_around(&self.finish);
        let mut itineraries = vec![Itinerary::new(
            self.start.clone(),
            self.finish.clone(),
            Vec::new(),
            10.,
        )];

        from_waypoints.iter().for_each(|from_wp| {
            to_waypoints.iter().for_each(|to_wp| {
                itineraries.push(Itinerary::new(
                    self.start.clone(),
                    self.finish.clone(),
                    vec![from_wp.clone(), to_wp.clone()],
                    1000.,
                ))
            })
        });
        itineraries
    }

    pub fn generate_routes(self) -> Vec<Route> {
        let itineraries = self.generate_itineraries();
        itineraries
            .into_par_iter()
            .map(|itinerary| {
                Navigator::new(
                    itinerary,
                    vec![
                        Box::new(|input| weight_check_distance_to_next(input)),
                        Box::new(|input| weight_prefer_same_road(input)),
                        Box::new(|input| weight_no_loops(input)),
                        Box::new(|input| weight_heading(input)),
                        {
                            let hints = self.hints.clone();
                            Box::new(move |input| weight_hints_highway(input, &hints))
                        },
                        {
                            let hints = self.hints.clone();
                            Box::new(move |input| weight_hints_surface(input, &hints))
                        },
                        {
                            let hints = self.hints.clone();
                            Box::new(move |input| weight_hints_smoothness(input, &hints))
                        },
                    ],
                )
                .generate_routes()
            })
            .filter_map(|nav_route| match nav_route {
                NavigationResult::Stuck => None,
                NavigationResult::Finished(route) => Some(route),
                NavigationResult::Stopped(route) => Some(route),
            })
            .collect::<Vec<_>>()
    }
}
