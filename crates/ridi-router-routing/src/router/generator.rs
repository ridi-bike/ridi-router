use std::{collections::HashMap, ops::Sub, time::Instant};

use crate::{
    map_data::graph::MapDataPointRef,
    progress::{
        point_snapshot, GenerationPassMetadata, ItineraryMetadataPayload, RoutingProgressEvent,
        RoutingProgressReporter, SnapPointMetadata, WaypointSelectionMetadata,
    },
    router::{clustering::Clustering, rules::RouterRules, weights::weight_check_avoid_rules},
    routing_api::{Coords, RouteMode},
    RoutingContext,
};
use geo::{Bearing, Destination, Distance, Haversine, Point};
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
        WeightCalc, WeightCalcStage,
    },
};

pub const WP_LOOKUP_ALLOWED_HWS: [&str; 6] = [
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

#[derive(Debug, Clone)]
struct GeneratedWaypoint {
    requested: Coords,
    snapped: MapDataPointRef,
}

#[derive(Debug, Clone)]
pub struct GeneratedItinerary {
    pub id: u64,
    pub itinerary: Itinerary,
    pub metadata: Option<ItineraryMetadataPayload>,
}

pub struct Generator {
    start: MapDataPointRef,
    finish: MapDataPointRef,
    round_trip: Option<(f32, u32)>,
    rules: RouterRules,
}

#[hotpath::measure_all]
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

    fn point_bearing(
        ctx: &RoutingContext<'_>,
        from: &MapDataPointRef,
        to: &MapDataPointRef,
    ) -> f32 {
        let (from_lat, from_lon) = ctx.point_coords(from);
        let (to_lat, to_lon) = ctx.point_coords(to);

        Haversine.bearing(Point::new(from_lon, from_lat), Point::new(to_lon, to_lat))
    }

    fn create_waypoints_around(
        &self,
        ctx: &RoutingContext<'_>,
        point: &MapDataPointRef,
        bearing: &f32,
        avoid_residential: bool,
    ) -> Vec<GeneratedWaypoint> {
        let (point_lat, point_lon) = ctx.point_coords(point);
        let point_geo = Point::new(point_lon, point_lat);

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
                        let snapped = ctx.closest_to_coords(
                            wp_geo.y(),
                            wp_geo.x(),
                            &self.rules,
                            avoid_residential,
                            Some(&WP_LOOKUP_ALLOWED_HWS),
                        )?;
                        Some(GeneratedWaypoint {
                            requested: Coords {
                                lat: wp_geo.y(),
                                lon: wp_geo.x(),
                            },
                            snapped,
                        })
                    })
            })
            .collect()
    }

    fn next_itinerary_id(next_id: &mut u64) -> u64 {
        let id = *next_id;
        *next_id += 1;
        id
    }

    #[tracing::instrument(skip(self, ctx, next_id))]
    fn generate_itineraries(
        &self,
        ctx: &RoutingContext<'_>,
        avoid_residential: bool,
        round_trip_bearing_adjustment: Option<f32>,
        emit_metadata: bool,
        next_id: &mut u64,
    ) -> Vec<GeneratedItinerary> {
        let generation_pass = GenerationPassMetadata {
            avoid_residential,
            round_trip_bearing_adjustment,
        };

        if let Some(round_trip) = self.round_trip {
            let (start_lat, start_lon) = ctx.point_coords(&self.start);
            let start_geo = Point::new(start_lon, start_lat);

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
                                            let tip_point = ctx.closest_to_coords(
                                                tip_geo.y(),
                                                tip_geo.x(),
                                                &self.rules,
                                                avoid_residential,
                                                Some(&WP_LOOKUP_ALLOWED_HWS),
                                            )?;

                                            let side_left_geo = Haversine.destination(
                                                start_geo,
                                                bearing + bearing_variation - 45.,
                                                dist * side_left_ratio,
                                            );
                                            let side_left_point = ctx.closest_to_coords(
                                                side_left_geo.y(),
                                                side_left_geo.x(),
                                                &self.rules,
                                                avoid_residential,
                                                Some(&WP_LOOKUP_ALLOWED_HWS),
                                            )?;

                                            let side_right_geo = Haversine.destination(
                                                start_geo,
                                                bearing + bearing_variation + 45.,
                                                dist * side_right_ratio,
                                            );
                                            let side_right_point = ctx.closest_to_coords(
                                                side_right_geo.y(),
                                                side_right_geo.x(),
                                                &self.rules,
                                                avoid_residential,
                                                Some(&WP_LOOKUP_ALLOWED_HWS),
                                            )?;

                                            let id = Self::next_itinerary_id(next_id);
                                            let itinerary = Itinerary::new_round_trip(
                                                self.start.clone(),
                                                self.finish.clone(),
                                                vec![
                                                    side_left_point.clone(),
                                                    tip_point.clone(),
                                                    side_right_point.clone(),
                                                ],
                                                3000.,
                                            );
                                            let metadata = emit_metadata.then(|| ItineraryMetadataPayload {
                                                itinerary_id: id,
                                                generation_pass: generation_pass.clone(),
                                                start: point_snapshot(ctx, &self.start),
                                                finish: point_snapshot(ctx, &self.finish),
                                                waypoints: itinerary
                                                    .waypoints
                                                    .iter()
                                                    .map(|point| point_snapshot(ctx, point))
                                                    .collect(),
                                                waypoint_selection: WaypointSelectionMetadata::RoundTripGenerated {
                                                    bearing: round_trip.0,
                                                    adjusted_bearing: bearing,
                                                    target_distance_m: round_trip.1,
                                                    side_left_requested: Coords { lat: side_left_geo.y(), lon: side_left_geo.x() },
                                                    side_left_snapped: point_snapshot(ctx, &side_left_point),
                                                    tip_requested: Coords { lat: tip_geo.y(), lon: tip_geo.x() },
                                                    tip_snapped: point_snapshot(ctx, &tip_point),
                                                    side_right_requested: Coords { lat: side_right_geo.y(), lon: side_right_geo.x() },
                                                    side_right_snapped: point_snapshot(ctx, &side_right_point),
                                                },
                                            });
                                            Some(GeneratedItinerary { id, itinerary, metadata })
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
        }

        let start_to_finish_bearing = Self::point_bearing(ctx, &self.start, &self.finish);
        let finish_to_start_bearing = Self::point_bearing(ctx, &self.finish, &self.start);
        let from_waypoints = self.create_waypoints_around(
            ctx,
            &self.start,
            &start_to_finish_bearing,
            avoid_residential,
        );
        let to_waypoints = self.create_waypoints_around(
            ctx,
            &self.finish,
            &finish_to_start_bearing,
            avoid_residential,
        );
        let start_finish_distance_m = {
            let (start_lat, start_lon) = ctx.point_coords(&self.start);
            let (finish_lat, finish_lon) = ctx.point_coords(&self.finish);
            Haversine.distance(
                Point::new(start_lon, start_lat),
                Point::new(finish_lon, finish_lat),
            )
        };

        let direct_id = Self::next_itinerary_id(next_id);
        let direct_itinerary =
            Itinerary::new_start_finish(self.start.clone(), self.finish.clone(), Vec::new(), 3000.);
        let mut itineraries = vec![GeneratedItinerary {
            id: direct_id,
            metadata: emit_metadata.then(|| ItineraryMetadataPayload {
                itinerary_id: direct_id,
                generation_pass: generation_pass.clone(),
                start: point_snapshot(ctx, &self.start),
                finish: point_snapshot(ctx, &self.finish),
                waypoints: Vec::new(),
                waypoint_selection: WaypointSelectionMetadata::DirectStartFinish,
            }),
            itinerary: direct_itinerary,
        }];

        from_waypoints.iter().for_each(|from_wp| {
            to_waypoints.iter().for_each(|to_wp| {
                let id = Self::next_itinerary_id(next_id);
                let itinerary = Itinerary::new_start_finish(
                    self.start.clone(),
                    self.finish.clone(),
                    vec![from_wp.snapped.clone(), to_wp.snapped.clone()],
                    3000.,
                );
                let metadata = emit_metadata.then(|| ItineraryMetadataPayload {
                    itinerary_id: id,
                    generation_pass: generation_pass.clone(),
                    start: point_snapshot(ctx, &self.start),
                    finish: point_snapshot(ctx, &self.finish),
                    waypoints: itinerary
                        .waypoints
                        .iter()
                        .map(|point| point_snapshot(ctx, point))
                        .collect(),
                    waypoint_selection: WaypointSelectionMetadata::StartFinishGenerated {
                        from_waypoint_requested: from_wp.requested,
                        from_waypoint_snapped: point_snapshot(ctx, &from_wp.snapped),
                        to_waypoint_requested: to_wp.requested,
                        to_waypoint_snapped: point_snapshot(ctx, &to_wp.snapped),
                        bearing_from_start_to_finish: start_to_finish_bearing,
                        bearing_from_finish_to_start: finish_to_start_bearing,
                        distance_m: start_finish_distance_m,
                    },
                });
                itineraries.push(GeneratedItinerary {
                    id,
                    itinerary,
                    metadata,
                })
            })
        });
        itineraries
    }

    #[tracing::instrument(skip(self, ctx, itineraries))]
    pub fn dedupe_itineraries(
        &self,
        ctx: &RoutingContext<'_>,
        itineraries: Vec<GeneratedItinerary>,
    ) -> Result<Vec<GeneratedItinerary>, GeneratorError> {
        let mut points = Vec::new();
        let mut point_source_indices = Vec::new();
        for (idx, itinerary) in itineraries
            .iter()
            .enumerate()
            .filter(|(_, i)| !i.itinerary.waypoints.is_empty())
        {
            point_source_indices.push(idx);
            points.push(
                itinerary
                    .itinerary
                    .waypoints
                    .iter()
                    .flat_map(|point_ref| {
                        let (lat, lon) = ctx.point_coords(point_ref);
                        [lat, lon]
                    })
                    .collect(),
            );
        }

        if points.len() < 2 {
            return Ok(itineraries);
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
            deduped_itineraries_map.insert(*label, itineraries[point_source_indices[idx]].clone());
        });

        let mut deduped_itineraries = deduped_itineraries_map.into_values().collect::<Vec<_>>();
        deduped_itineraries.append(
            &mut itineraries
                .into_iter()
                .filter(|i| i.itinerary.waypoints.is_empty())
                .collect(),
        );

        Ok(deduped_itineraries)
    }

    #[tracing::instrument(skip(self, ctx, progress, snapping_metadata))]
    pub fn generate_routes(
        self,
        ctx: &RoutingContext<'_>,
        progress: RoutingProgressReporter,
        request_mode: RouteMode,
        request_rules: RouterRules,
        snapping_metadata: Option<(SnapPointMetadata, SnapPointMetadata)>,
    ) -> Result<Vec<RouteWithStats>, GeneratorError> {
        let route_generation_start = Instant::now();
        let mut routes: Vec<Route> = Vec::new();
        let mut next_itinerary_id = 1u64;
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
                let itineraries = hotpath::measure_block!("generator.generate_itineraries", {
                    self.generate_itineraries(
                        ctx,
                        *avoid_residential,
                        Some(adjustment),
                        progress.enabled(),
                        &mut next_itinerary_id,
                    )
                });
                let itineraries = hotpath::measure_block!("generator.dedupe_itineraries", {
                    self.dedupe_itineraries(ctx, itineraries)?
                });
                let itinerary_count = itineraries.len();

                let route_gen_start_instant = Instant::now();
                let graph = ctx.graph();

                let mut routes_new = hotpath::measure_block!("generator.navigate_itineraries", {
                    itineraries
                        .into_par_iter()
                        .map(|generated_itinerary| {
                            let task_ctx = RoutingContext::new(graph);
                            let mut itinerary_progress =
                                progress.for_itinerary(generated_itinerary.id);
                            if itinerary_progress.enabled() {
                                itinerary_progress.emit(
                                    RoutingProgressEvent::RoutingRequestMetadata {
                                        mode: request_mode.clone(),
                                        rules: Box::new(request_rules.clone()),
                                    },
                                );
                                if let Some((start, finish)) = snapping_metadata.clone() {
                                    itinerary_progress.emit(
                                        RoutingProgressEvent::SnappingMetadata {
                                            start: Box::new(start),
                                            finish: Box::new(finish),
                                        },
                                    );
                                }
                                if let Some(metadata) = generated_itinerary.metadata.clone() {
                                    itinerary_progress.emit(metadata.into_event());
                                }
                            }
                            Navigator::new_with_progress(
                                generated_itinerary.itinerary,
                                self.rules.clone(),
                                vec![
                                    WeightCalc {
                                        name: "weight_avoid_nogo_areas".to_string(),
                                        stage: WeightCalcStage::PerForkChoice,
                                        calc: weight_avoid_nogo_areas,
                                    },
                                    WeightCalc {
                                        name: "weight_no_sharp_turns".to_string(),
                                        stage: WeightCalcStage::PerForkChoice,
                                        calc: weight_no_sharp_turns,
                                    },
                                    WeightCalc {
                                        name: "weight_no_short_detours".to_string(),
                                        stage: WeightCalcStage::PerForkChoice,
                                        calc: weight_no_short_detours,
                                    },
                                    WeightCalc {
                                        name: "weight_progress_speed".to_string(),
                                        stage: WeightCalcStage::RouteOnce,
                                        calc: weight_progress_speed,
                                    },
                                    WeightCalc {
                                        name: "weight_check_distance_to_next".to_string(),
                                        stage: WeightCalcStage::RouteOnce,
                                        calc: weight_check_distance_to_next,
                                    },
                                    WeightCalc {
                                        name: "weight_prefer_same_road".to_string(),
                                        stage: WeightCalcStage::PerForkChoice,
                                        calc: weight_prefer_same_road,
                                    },
                                    WeightCalc {
                                        name: "weight_no_loops".to_string(),
                                        stage: WeightCalcStage::RouteOnce,
                                        calc: weight_no_loops,
                                    },
                                    WeightCalc {
                                        name: "weight_heading".to_string(),
                                        stage: WeightCalcStage::PerForkChoice,
                                        calc: weight_heading,
                                    },
                                    WeightCalc {
                                        name: "weight_rules_highway".to_string(),
                                        stage: WeightCalcStage::PerForkChoice,
                                        calc: weight_rules_highway,
                                    },
                                    WeightCalc {
                                        name: "weight_rules_surface".to_string(),
                                        stage: WeightCalcStage::PerForkChoice,
                                        calc: weight_rules_surface,
                                    },
                                    WeightCalc {
                                        name: "weight_rules_smoothness".to_string(),
                                        stage: WeightCalcStage::PerForkChoice,
                                        calc: weight_rules_smoothness,
                                    },
                                    WeightCalc {
                                        name: "weight_check_avoid_rules".to_string(),
                                        stage: WeightCalcStage::RouteOnce,
                                        calc: weight_check_avoid_rules,
                                    },
                                ],
                                self.round_trip.is_some(),
                                itinerary_progress,
                            )
                            .generate_routes_with_context(&task_ctx)
                        })
                        .filter_map(|nav_route| match nav_route {
                            NavigationResult::Stuck => None,
                            NavigationResult::Finished(route) => Some(route),
                            NavigationResult::Stopped => None,
                        })
                        .collect::<Vec<_>>()
                });

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

        let clustering = hotpath::measure_block!("generator.cluster_routes", {
            Clustering::generate(ctx, &routes)
        });
        let clustering = match clustering {
            None => return Ok(Vec::new()),
            Some(c) => c,
        };

        let best_routes = hotpath::measure_block!("generator.score_routes", {
            let mut cluster_best: HashMap<i32, RouteWithStats> = HashMap::new();
            let mut noise = Vec::new();
            let _routes: Vec<_> = routes
                .iter()
                .enumerate()
                .map(|(idx, route)| {
                    let mut stats = route.calc_stats(ctx, &self.rules);
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
        });

        let route_generation_duration_secs = route_generation_start.elapsed().as_secs();
        info!(route_generation_duration_secs, "Route generation finished");

        Ok(best_routes)
    }
}
