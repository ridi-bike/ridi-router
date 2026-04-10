use std::collections::HashMap;

use geo::{Bearing, Haversine, Point};
use smartstring::alias::String as SmartString;
use tracing::{error, trace};

use crate::{
    router::rules::{RouterRules, RulesTagValueAction},
    RoutingContext,
};

use super::{
    itinerary::Itinerary,
    navigator::WeightCalcResult,
    route::{segment::Segment, Route},
    walker::{Walker, WalkerMoveResult},
};

pub struct WeightCalcInput<'a, 'ctx> {
    pub current_fork_segment: &'a Segment,
    pub route: &'a Route,
    pub itinerary: &'a Itinerary,
    pub walker_from_fork: Walker,
    pub rules: &'a RouterRules,
    pub ctx: &'a RoutingContext<'ctx>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeightCalcStage {
    RouteOnce,
    PerForkChoice,
}

pub struct WeightCalc {
    #[allow(dead_code)]
    pub name: String,
    pub stage: WeightCalcStage,
    pub calc: for<'a, 'ctx> fn(input: WeightCalcInput<'a, 'ctx>) -> WeightCalcResult,
}

fn get_priority_from_headings(bearing_next: f32, bearing_fork: f32) -> u8 {
    let adj = bearing_next.min(bearing_fork);
    let angle = bearing_next.max(bearing_fork) - adj;

    let degree_diff = if angle > 180. { 360. - angle } else { angle };

    let ratio: f32 = 255.0 / 180.0;

    255 - (degree_diff * ratio).round() as u8
}

fn point_distance(
    ctx: &RoutingContext<'_>,
    from: &crate::map_data::graph::MapDataPointRef,
    to: &crate::map_data::graph::MapDataPointRef,
) -> f32 {
    let from = ctx.point(from);
    let to = ctx.point(to);
    from.distance_between(&to)
}

fn point_distance_approx_from_coords(
    from_lat: f32,
    from_lon: f32,
    to_lat: f32,
    to_lon: f32,
) -> f32 {
    let mean_lat_rad = ((from_lat + to_lat) * 0.5).to_radians();
    let meters_per_degree_lat = 111_320.0_f32;
    let meters_per_degree_lon = meters_per_degree_lat * mean_lat_rad.cos();
    let dy_m = (from_lat - to_lat) * meters_per_degree_lat;
    let dx_m = (from_lon - to_lon) * meters_per_degree_lon;

    (dx_m * dx_m + dy_m * dy_m).sqrt()
}

fn point_distance_approx(
    ctx: &RoutingContext<'_>,
    from: &crate::map_data::graph::MapDataPointRef,
    to: &crate::map_data::graph::MapDataPointRef,
) -> f32 {
    let (from_lat, from_lon) = ctx.point_coords(from);
    let (to_lat, to_lon) = ctx.point_coords(to);

    point_distance_approx_from_coords(from_lat, from_lon, to_lat, to_lon)
}

fn should_use_exact_distance_fallback(distance_a_approx: f32, distance_b_approx: f32) -> bool {
    const EXACT_FALLBACK_RATIO: f32 = 0.03;

    let larger_distance = distance_a_approx.max(distance_b_approx);
    if larger_distance <= f32::EPSILON {
        return true;
    }

    // Keep the exact haversine fallback when the cheap planar comparison is within 3%.
    // The 3% guard came from the phase-5 approximation review: it keeps close-call
    // decisions exact while still avoiding most of the expensive distance work on the
    // longer, clearly-separated cases this phone-focused router cares about.
    (distance_a_approx - distance_b_approx).abs() <= larger_distance * EXACT_FALLBACK_RATIO
}

fn segment_name(ctx: &RoutingContext<'_>, segment: &Segment) -> Option<SmartString> {
    let line = ctx.line(segment.get_line());
    let tags = ctx.tag_set(&line.tags);
    ctx.with_tag_value(&tags.name, |value| value.cloned())
}

fn segment_hw_ref(ctx: &RoutingContext<'_>, segment: &Segment) -> Option<SmartString> {
    let line = ctx.line(segment.get_line());
    let tags = ctx.tag_set(&line.tags);
    ctx.with_tag_value(&tags.hw_ref, |value| value.cloned())
}

fn segment_highway(ctx: &RoutingContext<'_>, segment: &Segment) -> Option<SmartString> {
    let line = ctx.line(segment.get_line());
    let tags = ctx.tag_set(&line.tags);
    ctx.with_tag_value(&tags.highway, |value| value.cloned())
}

fn segment_surface(ctx: &RoutingContext<'_>, segment: &Segment) -> Option<SmartString> {
    let line = ctx.line(segment.get_line());
    let tags = ctx.tag_set(&line.tags);
    ctx.with_tag_value(&tags.surface, |value| value.cloned())
}

fn segment_smoothness(ctx: &RoutingContext<'_>, segment: &Segment) -> Option<SmartString> {
    let line = ctx.line(segment.get_line());
    let tags = ctx.tag_set(&line.tags);
    ctx.with_tag_value(&tags.smoothness, |value| value.cloned())
}

fn point_near_residential(
    ctx: &RoutingContext<'_>,
    point: &crate::map_data::graph::MapDataPointRef,
) -> bool {
    ctx.point_near_residential(point)
}

fn point_is_nogo(
    ctx: &RoutingContext<'_>,
    point: &crate::map_data::graph::MapDataPointRef,
) -> bool {
    ctx.point_is_nogo(point)
}

#[hotpath::measure]
pub fn weight_heading(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    trace!("weight_heading");

    let mut walker = input.walker_from_fork;
    let next_fork = match walker.move_forward_to_next_fork_with_context(input.ctx, |point| {
        input.itinerary.is_finished(point)
    }) {
        Ok(value) => value,
        Err(error) => {
            error!("weight calc error {:#?}", error);
            return WeightCalcResult::ForkChoiceDoNotUse;
        }
    };
    let _ = match next_fork {
        WalkerMoveResult::DeadEnd => return WeightCalcResult::ForkChoiceDoNotUse,
        WalkerMoveResult::Finish => return WeightCalcResult::ForkChoiceUseWithWeight(255),
        WalkerMoveResult::Fork(forks) => forks,
    };
    let fork_segment = match walker.get_route().get_segment_last() {
        Some(last_segment) => last_segment,
        None => input.current_fork_segment,
    };
    let (fork_lat, fork_lon) = input.ctx.point_coords(fork_segment.get_end_point());
    let fork_point_geo = Point::new(fork_lon, fork_lat);
    let (next_lat, next_lon) = input.ctx.point_coords(&input.itinerary.next);
    let next_point_geo = Point::new(next_lon, next_lat);

    let next_bearing = Haversine.bearing(fork_point_geo, next_point_geo);
    let fork_line = input.ctx.line(fork_segment.get_line());
    let (fork_line_0_lat, fork_line_0_lon) = input.ctx.point_coords(&fork_line.points.0);
    let (fork_line_1_lat, fork_line_1_lon) = input.ctx.point_coords(&fork_line.points.1);
    let fork_line_0_geo = Point::new(fork_line_0_lon, fork_line_0_lat);
    let fork_line_1_geo = Point::new(fork_line_1_lon, fork_line_1_lat);
    let fork_bearing = if &fork_line.points.1 == fork_segment.get_end_point() {
        Haversine.bearing(fork_line_0_geo, fork_line_1_geo)
    } else {
        Haversine.bearing(fork_line_1_geo, fork_line_0_geo)
    };

    WeightCalcResult::ForkChoiceUseWithWeight(get_priority_from_headings(
        next_bearing,
        fork_bearing,
    ))
}

#[hotpath::measure]
pub fn weight_prefer_same_road(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    trace!("weight_prefer_same_road");
    if !input.rules.basic.prefer_same_road.enabled {
        return WeightCalcResult::ForkChoiceUseWithWeight(0);
    }
    let current_ref = input
        .route
        .get_segment_last()
        .and_then(|segment| segment_hw_ref(input.ctx, segment));
    let current_name = input
        .route
        .get_segment_last()
        .and_then(|segment| segment_name(input.ctx, segment));
    let fork_ref = segment_hw_ref(input.ctx, input.current_fork_segment);
    let fork_name = segment_name(input.ctx, input.current_fork_segment);

    if (current_ref.is_some() && fork_ref.is_some() && current_ref == fork_ref)
        || (current_name.is_some() && fork_name.is_some() && current_name == fork_name)
    {
        return WeightCalcResult::ForkChoiceUseWithWeight(
            input.rules.basic.prefer_same_road.priority,
        );
    }

    WeightCalcResult::ForkChoiceUseWithWeight(0)
}

#[hotpath::measure]
pub fn weight_no_loops(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    trace!("weight_no_loops");
    if input
        .route
        .has_looped(input.ctx, input.itinerary.get_point_loop_check_since())
    {
        return WeightCalcResult::LastSegmentDoNotUse;
    }

    WeightCalcResult::ForkChoiceUseWithWeight(0)
}

#[hotpath::measure]
pub fn weight_no_sharp_turns(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    trace!("weight_no_sharp_turns");

    if !input.rules.basic.no_sharp_turns.enabled {
        return WeightCalcResult::ForkChoiceUseWithWeight(0);
    }

    let prev_segment = input.route.get_segment_last();

    if let Some(prev_segment) = prev_segment {
        let deg_diff =
            (prev_segment.bearing(input.ctx) - input.current_fork_segment.bearing(input.ctx)).abs();
        if deg_diff <= input.rules.basic.no_sharp_turns.under_deg {
            return WeightCalcResult::ForkChoiceUseWithWeight(
                input.rules.basic.no_sharp_turns.priority,
            );
        }
    }
    WeightCalcResult::ForkChoiceUseWithWeight(0)
}

#[hotpath::measure]
pub fn weight_no_short_detours(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    trace!("weight_no_short_detours");
    if !input.rules.basic.no_short_detours.enabled
        || point_near_residential(input.ctx, input.current_fork_segment.get_end_point())
    {
        return WeightCalcResult::ForkChoiceUseWithWeight(0);
    }

    let hw_ref = segment_hw_ref(input.ctx, input.current_fork_segment);
    let hw_name = segment_name(input.ctx, input.current_fork_segment);
    if input.route.is_back_on_road_within_distance(
        input.ctx,
        hw_ref,
        hw_name,
        input.rules.basic.no_short_detours.min_detour_len_m,
    ) {
        return WeightCalcResult::LastSegmentDoNotUse;
    }

    WeightCalcResult::ForkChoiceUseWithWeight(0)
}

#[hotpath::measure]
pub fn weight_check_distance_to_next(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    trace!("weight_check_distance_to_next");

    if !input.rules.basic.progression_direction.enabled {
        return WeightCalcResult::ForkChoiceUseWithWeight(0);
    }
    let check_junctions_back = input.rules.basic.progression_direction.check_junctions_back;

    let current_point = match input.route.get_segment_last() {
        None => return WeightCalcResult::ForkChoiceUseWithWeight(0),
        Some(segment) => segment.get_end_point(),
    };

    let check_from = input
        .itinerary
        .switched_wps_on
        .last()
        .map_or(&input.itinerary.start, |value| &value.on_point);
    let junctions_back_point = match input.route.nth_junction_from_end_since_point(
        input.ctx,
        check_from,
        check_junctions_back,
    ) {
        None => return WeightCalcResult::ForkChoiceUseWithWeight(0),
        Some(segment) => segment.get_end_point(),
    };

    let distance_to_next_current_approx =
        point_distance_approx(input.ctx, current_point, &input.itinerary.next);
    let distance_to_next_junctions_back_approx =
        point_distance_approx(input.ctx, junctions_back_point, &input.itinerary.next);
    let use_exact_fallback = should_use_exact_distance_fallback(
        distance_to_next_current_approx,
        distance_to_next_junctions_back_approx,
    );
    let (distance_to_next_current, distance_to_next_junctions_back) = if use_exact_fallback {
        (
            point_distance(input.ctx, current_point, &input.itinerary.next),
            point_distance(input.ctx, junctions_back_point, &input.itinerary.next),
        )
    } else {
        (
            distance_to_next_current_approx,
            distance_to_next_junctions_back_approx,
        )
    };
    trace!(
        used_exact_fallback = use_exact_fallback,
        current_distance = distance_to_next_current,
        historical_distance = distance_to_next_junctions_back,
        "distance to next"
    );

    if distance_to_next_current > distance_to_next_junctions_back {
        return WeightCalcResult::LastSegmentDoNotUse;
    }
    WeightCalcResult::ForkChoiceUseWithWeight(0)
}

#[hotpath::measure]
pub fn weight_progress_speed(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    trace!("weight_progress_speed");

    if !input.rules.basic.progression_speed.enabled {
        return WeightCalcResult::ForkChoiceUseWithWeight(0);
    }

    let check_steps_back = input.rules.basic.progression_speed.check_steps_back;

    let current_point = match input.route.get_segment_last() {
        None => return WeightCalcResult::ForkChoiceUseWithWeight(0),
        Some(segment) => segment.get_end_point(),
    };

    let total_distance = point_distance(input.ctx, &input.itinerary.start, &input.itinerary.next);
    let point_steps_back = match input.route.get_segments_from_end(check_steps_back) {
        None => return WeightCalcResult::ForkChoiceUseWithWeight(0),
        Some(segment) => segment.get_end_point().clone(),
    };

    let average_distance_per_segment = total_distance / (input.route.get_segment_count() as f32);

    let distance_last_points = point_distance(input.ctx, &point_steps_back, current_point);
    let average_distance_last_points = distance_last_points / (check_steps_back as f32);

    if average_distance_last_points
        < average_distance_per_segment
            * input
                .rules
                .basic
                .progression_speed
                .last_step_distance_below_avg_with_ratio
    {
        return WeightCalcResult::LastSegmentDoNotUse;
    }

    WeightCalcResult::ForkChoiceUseWithWeight(0)
}

fn get_rule_for_tag(
    rule: &Option<HashMap<String, RulesTagValueAction>>,
    segment_tag: Option<SmartString>,
) -> Option<WeightCalcResult> {
    if let Some(ref rule_tag) = rule {
        if let Some(segment_tag) = segment_tag {
            let rule_tag = rule_tag.get(segment_tag.as_str());
            if let Some(rule_tag) = rule_tag {
                return Some(match rule_tag {
                    RulesTagValueAction::Avoid => WeightCalcResult::ForkChoiceDoNotUse,
                    RulesTagValueAction::Priority { value } => {
                        WeightCalcResult::ForkChoiceUseWithWeight(*value)
                    }
                });
            }
        }
    }
    None
}

fn is_last_point_near_residential(input: &WeightCalcInput<'_, '_>) -> bool {
    match input.route.get_segment_last() {
        None => point_near_residential(input.ctx, &input.itinerary.start),
        Some(segment) => point_near_residential(input.ctx, segment.get_end_point()),
    }
}

#[hotpath::measure]
pub fn weight_rules_highway(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    trace!("weight_rules_highway");

    if is_last_point_near_residential(&input) {
        return WeightCalcResult::ForkChoiceUseWithWeight(0);
    }

    if input
        .route
        .get_route_chunk_since_junction_before_last(input.ctx)
        .iter()
        .any(|segment| {
            if let Some(tag_rule) =
                get_rule_for_tag(&input.rules.highway, segment_highway(input.ctx, segment))
            {
                if tag_rule == WeightCalcResult::ForkChoiceDoNotUse {
                    return true;
                }
            }
            false
        })
    {
        return WeightCalcResult::LastSegmentDoNotUse;
    }

    if let Some(result) = get_rule_for_tag(
        &input.rules.highway,
        segment_highway(input.ctx, input.current_fork_segment),
    ) {
        return result;
    }

    WeightCalcResult::ForkChoiceUseWithWeight(0)
}

#[hotpath::measure]
pub fn weight_rules_surface(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    trace!("weight_rules_surface");

    if is_last_point_near_residential(&input) {
        return WeightCalcResult::ForkChoiceUseWithWeight(0);
    }

    if input
        .route
        .get_route_chunk_since_junction_before_last(input.ctx)
        .iter()
        .any(|segment| {
            if let Some(tag_rule) =
                get_rule_for_tag(&input.rules.surface, segment_surface(input.ctx, segment))
            {
                if tag_rule == WeightCalcResult::ForkChoiceDoNotUse {
                    return true;
                }
            }
            false
        })
    {
        return WeightCalcResult::LastSegmentDoNotUse;
    }

    if let Some(result) = get_rule_for_tag(
        &input.rules.surface,
        segment_surface(input.ctx, input.current_fork_segment),
    ) {
        return result;
    }

    WeightCalcResult::ForkChoiceUseWithWeight(0)
}

#[hotpath::measure]
pub fn weight_rules_smoothness(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    trace!("weight_rules_smoothness");

    if is_last_point_near_residential(&input) {
        return WeightCalcResult::ForkChoiceUseWithWeight(0);
    }

    if input
        .route
        .get_route_chunk_since_junction_before_last(input.ctx)
        .iter()
        .any(|segment| {
            if let Some(tag_rule) = get_rule_for_tag(
                &input.rules.smoothness,
                segment_smoothness(input.ctx, segment),
            ) {
                if tag_rule == WeightCalcResult::ForkChoiceDoNotUse {
                    return true;
                }
            }
            false
        })
    {
        return WeightCalcResult::LastSegmentDoNotUse;
    }

    if let Some(result) = get_rule_for_tag(
        &input.rules.smoothness,
        segment_smoothness(input.ctx, input.current_fork_segment),
    ) {
        return result;
    }

    WeightCalcResult::ForkChoiceUseWithWeight(0)
}

#[hotpath::measure]
pub fn weight_avoid_nogo_areas(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    trace!("weight_avoid_nogo_areas");
    if point_is_nogo(input.ctx, input.current_fork_segment.get_end_point()) {
        return WeightCalcResult::ForkChoiceDoNotUse;
    }

    if let Some(segment) = input.route.get_segment_last() {
        if point_is_nogo(input.ctx, segment.get_end_point()) {
            return WeightCalcResult::LastSegmentDoNotUse;
        }
    } else if point_is_nogo(input.ctx, &input.itinerary.start) {
        return WeightCalcResult::LastSegmentDoNotUse;
    }
    WeightCalcResult::ForkChoiceUseWithWeight(0)
}

fn was_on_avoid<F>(
    route_chunk: &[Segment],
    tag_rule: &Option<HashMap<String, RulesTagValueAction>>,
    tag_getter: F,
) -> bool
where
    F: Fn(&Segment) -> Option<SmartString>,
{
    if let Some(tag_rules) = tag_rule {
        let avoid_rules = tag_rules
            .iter()
            .filter_map(|(key, rule)| match rule {
                RulesTagValueAction::Avoid => Some(key.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        if route_chunk
            .iter()
            .filter_map(tag_getter)
            .any(|tag| avoid_rules.contains(&tag.as_str()))
        {
            return true;
        }
    }
    false
}

#[hotpath::measure]
pub fn weight_check_avoid_rules(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    trace!("weight_check_avoid_rules");

    let last_chunk = input
        .route
        .get_route_chunk_since_junction_before_last(input.ctx);
    if was_on_avoid(&last_chunk, &input.rules.highway, |segment| {
        segment_highway(input.ctx, segment)
    }) {
        return WeightCalcResult::LastSegmentDoNotUse;
    }
    if was_on_avoid(&last_chunk, &input.rules.surface, |segment| {
        segment_surface(input.ctx, segment)
    }) {
        return WeightCalcResult::LastSegmentDoNotUse;
    }
    if was_on_avoid(&last_chunk, &input.rules.smoothness, |segment| {
        segment_smoothness(input.ctx, segment)
    }) {
        return WeightCalcResult::LastSegmentDoNotUse;
    }

    WeightCalcResult::ForkChoiceUseWithWeight(0)
}

#[cfg(test)]
mod test {

    use geo::{Distance, Haversine, Point};

    use super::{
        get_priority_from_headings, point_distance_approx_from_coords,
        should_use_exact_distance_fallback,
    };

    #[test]
    fn get_prio_from_headings() {
        let tests = vec![
            (0., 0., 255),
            (180., 0., 0),
            (90., 0., 127),
            (0., 180., 0),
            (0., 90., 127),
            (0., 45., 191),
            (0., 135., 64),
            (15., 60., 191),
            (60., 15., 191),
            (15., 330., 191),
            (330., 15., 191),
            (0., 315., 191),
            (1., 316., 191),
            (180., 316., 62),
            (316., 180., 62),
        ];
        for test in tests {
            println!("test: {}-{}: {}", test.0, test.1, test.2);
            let res = get_priority_from_headings(test.0, test.1);
            assert_eq!(test.2, res);
        }
    }

    fn exact_distance_m(from: (f32, f32), to: (f32, f32)) -> f32 {
        Haversine.distance(Point::new(from.1, from.0), Point::new(to.1, to.0))
    }

    fn approx_distance_m(from: (f32, f32), to: (f32, f32)) -> f32 {
        point_distance_approx_from_coords(from.0, from.1, to.0, to.1)
    }

    #[test]
    fn guarded_distance_comparison_falls_back_and_matches_exact_on_close_call() {
        let current = (-80.0, -170.0);
        let historical = (-80.0, -165.0);
        let target = (-80.0, 15.0);
        let current_approx = approx_distance_m(current, target);
        let historical_approx = approx_distance_m(historical, target);
        let current_exact = exact_distance_m(current, target);
        let historical_exact = exact_distance_m(historical, target);

        let guarded_order = if should_use_exact_distance_fallback(current_approx, historical_approx)
        {
            current_exact > historical_exact
        } else {
            current_approx > historical_approx
        };

        assert_ne!(
            current_approx > historical_approx,
            current_exact > historical_exact
        );
        assert!(should_use_exact_distance_fallback(
            current_approx,
            historical_approx,
        ));
        assert_eq!(guarded_order, current_exact > historical_exact);
    }

    #[test]
    fn guarded_distance_comparison_skips_exact_when_approx_gap_is_clear() {
        let current = (56.95, 24.1);
        let historical = (56.2, 23.2);
        let target = (57.3, 25.3);
        let current_approx = approx_distance_m(current, target);
        let historical_approx = approx_distance_m(historical, target);
        let current_exact = exact_distance_m(current, target);
        let historical_exact = exact_distance_m(historical, target);

        assert!(!should_use_exact_distance_fallback(
            current_approx,
            historical_approx,
        ));
        assert_eq!(
            current_approx > historical_approx,
            current_exact > historical_exact
        );
    }
}

#[cfg(test)]
#[path = "weights_phase1_tests.rs"]
mod phase1_tests;
