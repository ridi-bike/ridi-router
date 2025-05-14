use std::{collections::HashMap, ops::Sub};

use crate::{
    debug::writer::DebugWriter,
    map_data::graph::{MapDataGraph, MapDataPointRef},
    router::{
        clustering::Clustering,
        rules::RouterRules,
        weights::{
            WeightCalcDistanceToNext, WeightCalcHeading, WeightCalcNoLoops, WeightCalcNoSharpTurns,
            WeightCalcNoShortDetour, WeightCalcPreferSameRoad, WeightCalcProgressSpeed,
            WeightCalcRulesHighway, WeightCalcRulesSmoothness, WeightCalcRulesSurface,
        },
    },
};
use geo::{Destination, Haversine, Point};
use rayon::prelude::*;
use tracing::trace;

use super::{
    itinerary::Itinerary,
    navigator::{NavigationResult, Navigator},
    route::{Route, RouteStats},
};

const START_FINISH_VARIATION_DISTANCES: [f32; 3] = [10000., 20000., 30000.];
const START_FINISH_VARIATION_DEGREES: [f32; 8] = [0., 45., 90., 135., 180., 225., 270., 315.];
const ROUND_TRIP_DISTANCE_RATIOS: [f32; 4] = [1.0, 0.8, 0.6, 0.4];
const ROUND_TRIP_BEARING_VARIATION: [f32; 4] = [-25., -10., 10., 25.];

#[derive(Debug, Clone)]
pub struct RouteWithStats {
    pub stats: RouteStats,
    pub route: Route,
}

pub struct Generator {
    start: MapDataPointRef,
    finish: MapDataPointRef,
    round_trip: Option<(f32, u32)>,
    rules: RouterRules,
}

impl Generator {
    pub fn new(
        start: MapDataPointRef,
        finish: MapDataPointRef,
        round_trip: Option<(f32, u32)>,
        rules: RouterRules,
    ) -> Self {
        Self {
            start,
            finish,
            round_trip,
            rules,
        }
    }

    fn create_waypoints_around(
        &self,
        point: &MapDataPointRef,
        bearing: &f32,
    ) -> Vec<MapDataPointRef> {
        let point_geo = Point::new(point.borrow().lon, point.borrow().lat);
        START_FINISH_VARIATION_DEGREES
            .iter()
            .filter(|deg| deg.sub(bearing).abs() > 20.)
            .flat_map(|bearing| {
                START_FINISH_VARIATION_DISTANCES
                    .iter()
                    .filter_map(|distance| {
                        let wp_geo = Haversine::destination(point_geo, *bearing, *distance);

                        MapDataGraph::get().get_closest_to_coords(
                            wp_geo.y(),
                            wp_geo.x(),
                            &self.rules,
                        )
                    })
            })
            .collect()
    }

    #[tracing::instrument(skip(self))]
    fn generate_itineraries(&self) -> Vec<Itinerary> {
        if let Some(round_trip) = self.round_trip {
            let start_geo = Point::new(self.start.borrow().lon, self.start.borrow().lat);

            return ROUND_TRIP_DISTANCE_RATIOS
                .iter()
                .flat_map(|side_left_ratio| {
                    let bearing = round_trip.0;
                    ROUND_TRIP_DISTANCE_RATIOS
                        .iter()
                        .flat_map(|tip_ratio| {
                            ROUND_TRIP_DISTANCE_RATIOS
                                .iter()
                                .flat_map(|side_right_ratio| {
                                    ROUND_TRIP_BEARING_VARIATION
                                        .iter()
                                        .filter_map(|bearing_variation| {
                                            let dist = round_trip.1 as f32 / 5.;
                                            let tip_geo = Haversine::destination(
                                                start_geo,
                                                bearing + bearing_variation,
                                                dist * tip_ratio,
                                            );

                                            let tip_point = match MapDataGraph::get()
                                                .get_closest_to_coords(
                                                    tip_geo.y(),
                                                    tip_geo.x(),
                                                    &self.rules,
                                                ) {
                                                None => return None,
                                                Some(p) => p,
                                            };

                                            let side_left_geo = Haversine::destination(
                                                start_geo,
                                                bearing + bearing_variation - 45.,
                                                dist * side_left_ratio,
                                            );

                                            let side_left_point = match MapDataGraph::get()
                                                .get_closest_to_coords(
                                                    side_left_geo.y(),
                                                    side_left_geo.x(),
                                                    &self.rules,
                                                ) {
                                                None => return None,
                                                Some(p) => p,
                                            };

                                            let side_right_geo = Haversine::destination(
                                                start_geo,
                                                bearing + bearing_variation + 45.,
                                                dist * side_right_ratio,
                                            );

                                            let side_right_point = match MapDataGraph::get()
                                                .get_closest_to_coords(
                                                    side_right_geo.y(),
                                                    side_right_geo.x(),
                                                    &self.rules,
                                                ) {
                                                None => return None,
                                                Some(p) => p,
                                            };

                                            Some(Itinerary::new_round_trip(
                                                self.start.clone(),
                                                self.finish.clone(),
                                                vec![side_left_point, tip_point, side_right_point],
                                                3000.,
                                            ))
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
        }
        let from_waypoints =
            self.create_waypoints_around(&self.start, &self.finish.borrow().bearing(&self.start));
        let to_waypoints =
            self.create_waypoints_around(&self.finish, &self.start.borrow().bearing(&self.finish));
        let mut itineraries = vec![Itinerary::new_start_finish(
            self.start.clone(),
            self.finish.clone(),
            Vec::new(),
            3000.,
        )];

        from_waypoints.iter().for_each(|from_wp| {
            to_waypoints.iter().for_each(|to_wp| {
                itineraries.push(Itinerary::new_start_finish(
                    self.start.clone(),
                    self.finish.clone(),
                    vec![from_wp.clone(), to_wp.clone()],
                    3000.,
                ))
            })
        });
        itineraries
    }

    #[tracing::instrument(skip(self))]
    pub fn generate_routes(self) -> Vec<RouteWithStats> {
        let itineraries = self.generate_itineraries();

        DebugWriter::write_itineraries(&itineraries);

        trace!("Created {} itineraries", itineraries.len());
        let routes = itineraries
            .into_par_iter()
            .map(|itinerary| {
                Navigator::new(
                    itinerary,
                    self.rules.clone(),
                    vec![
                        Box::new(WeightCalcNoSharpTurns),
                        Box::new(WeightCalcNoShortDetour),
                        Box::new(WeightCalcProgressSpeed),
                        Box::new(WeightCalcDistanceToNext),
                        Box::new(WeightCalcPreferSameRoad),
                        Box::new(WeightCalcNoLoops),
                        Box::new(WeightCalcHeading),
                        Box::new(WeightCalcRulesHighway),
                        Box::new(WeightCalcRulesSurface),
                        Box::new(WeightCalcRulesSmoothness),
                    ],
                    self.round_trip.is_some(),
                )
                .generate_routes()
            })
            .filter_map(|nav_route| match nav_route {
                NavigationResult::Stuck => None,
                NavigationResult::Finished(route) => Some(route),
                NavigationResult::Stopped => None,
            })
            .collect::<Vec<_>>();

        let clustering = match Clustering::generate(&routes) {
            None => return Vec::new(),
            Some(c) => c,
        };

        let mut cluster_best: HashMap<i32, RouteWithStats> = HashMap::new();
        let mut noise = Vec::new();
        let _routes: Vec<_> = routes
            .iter()
            .enumerate()
            .map(|(idx, route)| {
                let mut stats = route.calc_stats();
                let approx_route = &clustering.approximated_routes[idx];
                stats.cluster = Some(clustering.labels[idx] as usize);
                stats.approximated_route = approx_route.iter().map(|p| (p[0], p[1])).collect();
                let route_with_stats = RouteWithStats {
                    stats,
                    route: route.clone(),
                };

                let label = clustering.labels[idx];
                if label != -1 {
                    if let Some(current_best) = cluster_best.get(&label) {
                        if current_best.stats.score < route_with_stats.stats.score {
                            cluster_best.insert(label, route_with_stats.clone());
                        }
                    } else {
                        cluster_best.insert(label, route_with_stats.clone());
                    }
                } else {
                    noise.push(route_with_stats.clone());
                }
                route_with_stats
            })
            .collect();

        trace!(route_count = routes.len(), "routes");
        trace!(noise_count = noise.len(), "noise");

        let mut best_routes = cluster_best.into_iter().map(|el| el.1).collect::<Vec<_>>();
        noise.sort_by(|a, b| b.stats.score.total_cmp(&a.stats.score));

        let noise_count = if best_routes.len() > 10 { 3 } else { 10 };
        best_routes.append(&mut noise[..noise.len().min(noise_count)].to_vec());
        best_routes
    }
}
