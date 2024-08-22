use crate::{map_data::graph::MapDataPointRef, MAP_DATA_GRAPH};
use geo::{HaversineDestination, Point};
use rayon::prelude::*;

use super::{
    itinerary::Itinerary,
    navigator::{NavigationResult, Navigator},
    route::Route,
    weights::{
        weight_check_distance_to_next, weight_heading, weight_no_loops, weight_prefer_same_road,
        weight_progress_speed,
    },
};

const ITINERARY_VARIATION_DISTANCES: [f64; 2] = [10000., 20000.];
const ITINERARY_VARIATION_DEGREES: [f64; 8] = [0., 45., 90., 135., 180., -45., -90., -135.];

pub struct Generator {
    from: MapDataPointRef,
    to: MapDataPointRef,
}

impl Generator {
    pub fn new(from: MapDataPointRef, to: MapDataPointRef) -> Self {
        Self { from, to }
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
                        let wp = MAP_DATA_GRAPH.get_closest_to_coords(wp_geo.y(), wp_geo.x());
                        wp
                    })
                    .filter_map(|maybe_wp| maybe_wp)
            })
            .flatten()
            .collect()
    }

    fn generate_itineraries(&self) -> Vec<Itinerary> {
        let to_waypoints = self.create_waypoints_around(&self.to);
        let mut itineraries = vec![Itinerary::new(
            self.from.clone(),
            self.to.clone(),
            Vec::new(),
            10.,
        )];

        to_waypoints.iter().for_each(|wp| {
            itineraries.push(Itinerary::new(
                self.from.clone(),
                self.to.clone(),
                vec![wp.clone()],
                1000.,
            ))
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
                        weight_check_distance_to_next,
                        weight_prefer_same_road,
                        weight_no_loops,
                        weight_heading,
                        // weight_progress_speed,
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
