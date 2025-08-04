use std::{collections::HashMap, ops::Sub, time::Instant};

use crate::{
    debug::writer::DebugWriter,
    map_data::graph::{MapDataGraph, MapDataPointRef},
    router::{clustering::Clustering, rules::RouterRules, weights::weight_check_avoid_rules},
};
use geo::{Destination, Haversine, Point};
use hdbscan::{Hdbscan, HdbscanError, HdbscanHyperParams};
use rayon::prelude::*;
use tracing::{error, info, trace};

use super::{
    itinerary::Itinerary,
    navigator::{NavigationResult, Navigator},
    route::{Route, RouteStats},
    weights::{
        weight_avoid_nogo_areas, weight_check_distance_to_next, weight_heading, weight_no_loops,
        weight_no_sharp_turns, weight_no_short_detours, weight_prefer_same_road,
        weight_progress_speed, weight_rules_highway, weight_rules_smoothness, weight_rules_surface,
        WeightCalc,
    },
};

pub const WP_LOOKUP_ALLOWED_HWS: [&'static str; 6] = [
    "motorway",
    "trunk",
    "primary",
    "secondary",
    "tertiary",
    "unclassified",
];

#[derive(Debug, thiserror::Error)]
pub enum GeneratorError {
    #[error("Hdbscan error: {error}")]
    Hdbscan { error: HdbscanError },
}

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
        avoid_residential: bool,
    ) -> Vec<MapDataPointRef> {
        let point_geo = Point::new(point.borrow().lon, point.borrow().lat);
        self.rules
            .generation
            .waypoint_generation
            .start_finish
            .variation_bearing_deg
            .iter()
            .filter(|deg| deg.sub(bearing).abs() > 20.)
            .flat_map(|bearing| {
                self.rules
                    .generation
                    .waypoint_generation
                    .start_finish
                    .variation_distances_m
                    .iter()
                    .filter_map(|distance| {
                        let wp_geo = Haversine.destination(point_geo, *bearing, *distance);

                        MapDataGraph::get().get_closest_to_coords(
                            wp_geo.y(),
                            wp_geo.x(),
                            &self.rules,
                            avoid_residential,
                            Some(&WP_LOOKUP_ALLOWED_HWS),
                        )
                    })
            })
            .collect()
    }

    #[tracing::instrument(skip(self))]
    fn generate_itineraries(
        &self,
        avoid_residential: bool,
        round_trip_bearing_adjustment: Option<f32>,
    ) -> Vec<Itinerary> {
        if let Some(round_trip) = self.round_trip {
            let start_geo = Point::new(self.start.borrow().lon, self.start.borrow().lat);

            return self
                .rules
                .generation
                .waypoint_generation
                .round_trip
                .variation_distance_ratios
                .iter()
                .flat_map(|side_left_ratio| {
                    let bearing_adjusted =
                        round_trip.0 + round_trip_bearing_adjustment.unwrap_or(0.);
                    let bearing = if bearing_adjusted < 0. {
                        360. - bearing_adjusted.abs()
                    } else {
                        bearing_adjusted
                    };
                    self.rules
                        .generation
                        .waypoint_generation
                        .round_trip
                        .variation_distance_ratios
                        .iter()
                        .flat_map(|tip_ratio| {
                            self.rules
                                .generation
                                .waypoint_generation
                                .round_trip
                                .variation_distance_ratios
                                .iter()
                                .flat_map(|side_right_ratio| {
                                    self.rules
                                        .generation
                                        .waypoint_generation
                                        .round_trip
                                        .variation_bearing_deg
                                        .iter()
                                        .filter_map(|bearing_variation| {
                                            let dist = round_trip.1 as f32 / 5.;
                                            let tip_geo = Haversine.destination(
                                                start_geo,
                                                bearing + bearing_variation,
                                                dist * tip_ratio,
                                            );

                                            let tip_point = match MapDataGraph::get()
                                                .get_closest_to_coords(
                                                    tip_geo.y(),
                                                    tip_geo.x(),
                                                    &self.rules,
                                                    avoid_residential,
                                                    Some(&WP_LOOKUP_ALLOWED_HWS),
                                                ) {
                                                None => return None,
                                                Some(p) => p,
                                            };

                                            let side_left_geo = Haversine.destination(
                                                start_geo,
                                                bearing + bearing_variation - 45.,
                                                dist * side_left_ratio,
                                            );

                                            let side_left_point = match MapDataGraph::get()
                                                .get_closest_to_coords(
                                                    side_left_geo.y(),
                                                    side_left_geo.x(),
                                                    &self.rules,
                                                    avoid_residential,
                                                    Some(&WP_LOOKUP_ALLOWED_HWS),
                                                ) {
                                                None => return None,
                                                Some(p) => p,
                                            };

                                            let side_right_geo = Haversine.destination(
                                                start_geo,
                                                bearing + bearing_variation + 45.,
                                                dist * side_right_ratio,
                                            );

                                            let side_right_point = match MapDataGraph::get()
                                                .get_closest_to_coords(
                                                    side_right_geo.y(),
                                                    side_right_geo.x(),
                                                    &self.rules,
                                                    avoid_residential,
                                                    Some(&WP_LOOKUP_ALLOWED_HWS),
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
        let from_waypoints = self.create_waypoints_around(
            &self.start,
            &self.finish.borrow().bearing(&self.start),
            avoid_residential,
        );
        let to_waypoints = self.create_waypoints_around(
            &self.finish,
            &self.start.borrow().bearing(&self.finish),
            avoid_residential,
        );
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

    #[tracing::instrument(skip(self, itineraries))]
    pub fn dedupe_itineraries(
        &self,
        itineraries: Vec<Itinerary>,
    ) -> Result<Vec<Itinerary>, GeneratorError> {
        let mut points = Vec::new();
        for itinerary in itineraries.iter().filter(|i| i.waypoints.len() > 0) {
            points.push(
                itinerary
                    .waypoints
                    .iter()
                    .map(|p| {
                        let point = p.borrow();
                        vec![point.lat, point.lon]
                    })
                    .flatten()
                    .collect(),
            );
        }
        if points.is_empty() {
            return Ok(vec![]);
        }
        let params = HdbscanHyperParams::builder()
            .epsilon(0.01)
            .min_cluster_size(2)
            .build();
        let alg = Hdbscan::new(&points, params);
        let labels = match alg.cluster() {
            Ok(l) => l,
            Err(e) => return Err(GeneratorError::Hdbscan { error: e }),
        };

        let mut deduped_itineraries_map = HashMap::new();
        labels.iter().enumerate().for_each(|(idx, label)| {
            deduped_itineraries_map.insert(*label, itineraries[idx].clone());
        });

        let mut deduped_itineraries = deduped_itineraries_map.into_values().collect::<Vec<_>>();
        deduped_itineraries.append(
            &mut itineraries
                .into_iter()
                .filter(|i| i.waypoints.len() == 0)
                .collect(),
        );

        Ok(deduped_itineraries)
    }

    #[tracing::instrument(skip(self))]
    pub fn generate_routes(self) -> Result<Vec<RouteWithStats>, GeneratorError> {
        let route_generation_start = Instant::now();
        let mut routes: Vec<Route> = Vec::new();
        'outer: for avoid_residential in self
            .rules
            .generation
            .route_generation_retry
            .avoid_residential
            .iter()
        {
            // no adjustment by default, only for round trip
            let mut adjustments = vec![0.];
            if self.round_trip.is_some() {
                adjustments.append(
                    &mut self
                        .rules
                        .generation
                        .route_generation_retry
                        .round_trip_adjustment_bearing_deg
                        .clone(),
                );
            }
            for adjustment in adjustments {
                if routes.len()
                    >= self
                        .rules
                        .generation
                        .route_generation_retry
                        .trigger_min_route_count
                {
                    break 'outer;
                }
                let itineraries = self.generate_itineraries(*avoid_residential, Some(adjustment));
                let itineraries = self.dedupe_itineraries(itineraries)?;
                let itinerary_count = itineraries.len();

                DebugWriter::write_itineraries(&itineraries);

                let route_gen_start_instant = Instant::now();

                let mut routes_new = itineraries
                    .into_par_iter()
                    .map(|itinerary| {
                        Navigator::new(
                            itinerary,
                            self.rules.clone(),
                            vec![
                                WeightCalc {
                                    name: "weight_avoid_nogo_areas".to_string(),
                                    calc: weight_avoid_nogo_areas,
                                },
                                WeightCalc {
                                    name: "weight_no_sharp_turns".to_string(),
                                    calc: weight_no_sharp_turns,
                                },
                                WeightCalc {
                                    name: "weight_no_short_detours".to_string(),
                                    calc: weight_no_short_detours,
                                },
                                WeightCalc {
                                    name: "weight_progress_speed".to_string(),
                                    calc: weight_progress_speed,
                                },
                                WeightCalc {
                                    name: "weight_check_distance_to_next".to_string(),
                                    calc: weight_check_distance_to_next,
                                },
                                WeightCalc {
                                    name: "weight_prefer_same_road".to_string(),
                                    calc: weight_prefer_same_road,
                                },
                                WeightCalc {
                                    name: "weight_no_loops".to_string(),
                                    calc: weight_no_loops,
                                },
                                WeightCalc {
                                    name: "weight_heading".to_string(),
                                    calc: weight_heading,
                                },
                                WeightCalc {
                                    name: "weight_rules_highway".to_string(),
                                    calc: weight_rules_highway,
                                },
                                WeightCalc {
                                    name: "weight_rules_surface".to_string(),
                                    calc: weight_rules_surface,
                                },
                                WeightCalc {
                                    name: "weight_rules_smoothness".to_string(),
                                    calc: weight_rules_smoothness,
                                },
                                WeightCalc {
                                    name: "weight_check_avoid_rules".to_string(),
                                    calc: weight_check_avoid_rules,
                                },
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

                let route_gen_duration_secs = route_gen_start_instant.elapsed().as_secs();
                info!(
                    route_gen_duration_secs,
                    itinerary_count,
                    routes_count = routes_new.len(),
                    adjustment,
                    avoid_residential,
                    "Routes from itineraries"
                );
                routes.append(&mut routes_new);
            }
        }

        let clustering = match Clustering::generate(&routes) {
            None => return Ok(Vec::new()),
            Some(c) => c,
        };

        let mut cluster_best: HashMap<i32, RouteWithStats> = HashMap::new();
        let mut noise = Vec::new();
        let _routes: Vec<_> = routes
            .iter()
            .enumerate()
            .map(|(idx, route)| {
                let mut stats = route.calc_stats(&self.rules);
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

        let route_generation_duration_secs = route_generation_start.elapsed().as_secs();
        info!(route_generation_duration_secs, "Route generation finished");

        Ok(best_routes)
    }
}
