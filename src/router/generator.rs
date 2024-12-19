use std::collections::HashMap;

use crate::{
    map_data::graph::{MapDataGraph, MapDataPointRef},
    router::{clustering::Clustering, rules::RouterRules},
};
use geo::{HaversineDestination, Point};
use rayon::prelude::*;
use tracing::info;

use super::{
    itinerary::Itinerary,
    navigator::{NavigationResult, Navigator},
    route::{Route, RouteStats},
    weights::{
        weight_check_distance_to_next, weight_heading, weight_no_loops, weight_prefer_same_road,
        weight_progress_speed, weight_rules_highway, weight_rules_smoothness, weight_rules_surface,
    },
};

const ITINERARY_VARIATION_DISTANCES: [f32; 2] = [10000., 20000.];
const ITINERARY_VARIATION_DEGREES: [f32; 8] = [0., 45., 90., 135., 180., -45., -90., -135.];

#[derive(Debug, Clone)]
pub struct RouteWithStats {
    pub stats: RouteStats,
    pub route: Route,
}

pub struct Generator {
    start: MapDataPointRef,
    finish: MapDataPointRef,
    rules: RouterRules,
}

impl Generator {
    pub fn new(start: MapDataPointRef, finish: MapDataPointRef, rules: RouterRules) -> Self {
        Self {
            start,
            finish,
            rules,
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

    #[tracing::instrument(skip(self))]
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

    #[tracing::instrument(skip(self))]
    pub fn generate_routes(self) -> Vec<RouteWithStats> {
        let itineraries = self.generate_itineraries();
        info!("Created {} itineraries", itineraries.len());
        let routes = itineraries
            .into_par_iter()
            .map(|itinerary| {
                Navigator::new(
                    itinerary,
                    self.rules.clone(),
                    vec![
                        weight_progress_speed,
                        weight_check_distance_to_next,
                        weight_prefer_same_road,
                        weight_no_loops,
                        weight_heading,
                        weight_rules_highway,
                        weight_rules_surface,
                        weight_rules_smoothness,
                    ],
                )
                .generate_routes()
            })
            .filter_map(|nav_route| match nav_route {
                NavigationResult::Stuck => None,
                NavigationResult::Finished(route) => Some(route),
                NavigationResult::Stopped(route) => Some(route),
            })
            .collect::<Vec<_>>();

        let clustering = Clustering::generate(&routes);

        let mut cluster_best: HashMap<i32, RouteWithStats> = HashMap::new();
        routes.iter().enumerate().for_each(|(idx, route)| {
            let mut stats = route.calc_stats();
            let approx_route = &clustering.approximated_routes[idx];
            stats.cluster = Some(clustering.labels[idx] as usize);
            stats.approximated_route = approx_route.iter().map(|p| (p[0], p[1])).collect();
            let route_with_stats = RouteWithStats {
                stats,
                route: route.clone(),
            };

            if let Some(current_best) = cluster_best.get(&clustering.labels[idx]) {
                if current_best.stats.score < route_with_stats.stats.score {
                    cluster_best.insert(clustering.labels[idx], route_with_stats);
                }
            } else {
                cluster_best.insert(clustering.labels[idx], route_with_stats);
            }
        });

        cluster_best.into_iter().map(|el| el.1).collect()
    }
}
