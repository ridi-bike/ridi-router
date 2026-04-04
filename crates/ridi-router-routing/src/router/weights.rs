use std::collections::HashMap;

use geo::{Bearing, Haversine, Point};
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

pub struct WeightCalc {
    #[allow(dead_code)]
    pub name: String,
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

fn segment_name(ctx: &RoutingContext<'_>, segment: &Segment) -> Option<String> {
    let line = ctx.line(segment.get_line());
    let tags = ctx.tag_set(&line.tags);
    ctx.tag_value(&tags.name)
}

fn segment_hw_ref(ctx: &RoutingContext<'_>, segment: &Segment) -> Option<String> {
    let line = ctx.line(segment.get_line());
    let tags = ctx.tag_set(&line.tags);
    ctx.tag_value(&tags.hw_ref)
}

fn segment_highway(ctx: &RoutingContext<'_>, segment: &Segment) -> Option<String> {
    let line = ctx.line(segment.get_line());
    let tags = ctx.tag_set(&line.tags);
    ctx.tag_value(&tags.highway)
}

fn segment_surface(ctx: &RoutingContext<'_>, segment: &Segment) -> Option<String> {
    let line = ctx.line(segment.get_line());
    let tags = ctx.tag_set(&line.tags);
    ctx.tag_value(&tags.surface)
}

fn segment_smoothness(ctx: &RoutingContext<'_>, segment: &Segment) -> Option<String> {
    let line = ctx.line(segment.get_line());
    let tags = ctx.tag_set(&line.tags);
    ctx.tag_value(&tags.smoothness)
}

fn point_near_residential(
    ctx: &RoutingContext<'_>,
    point: &crate::map_data::graph::MapDataPointRef,
) -> bool {
    ctx.point(point).residential_in_proximity
}

fn point_is_nogo(
    ctx: &RoutingContext<'_>,
    point: &crate::map_data::graph::MapDataPointRef,
) -> bool {
    ctx.point(point).nogo_area
}

pub fn weight_heading(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    trace!("weight_heading");

    let mut walker = input.walker_from_fork;
    let next_fork = match walker
        .move_forward_to_next_fork_with_context(input.ctx, |point| input.itinerary.is_finished(point))
    {
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
    let fork_point = input.ctx.point(fork_segment.get_end_point());
    let fork_point_geo = Point::new(fork_point.lon, fork_point.lat);
    let next_point = input.ctx.point(&input.itinerary.next);
    let next_point_geo = Point::new(next_point.lon, next_point.lat);

    let next_bearing = Haversine.bearing(fork_point_geo, next_point_geo);
    let fork_line = input.ctx.line(fork_segment.get_line());
    let fork_line_0 = input.ctx.point(&fork_line.points.0);
    let fork_line_1 = input.ctx.point(&fork_line.points.1);
    let fork_line_0_geo = Point::new(fork_line_0.lon, fork_line_0.lat);
    let fork_line_1_geo = Point::new(fork_line_1.lon, fork_line_1.lat);
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

pub fn weight_no_sharp_turns(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    trace!("weight_no_sharp_turns");

    if !input.rules.basic.no_sharp_turns.enabled {
        return WeightCalcResult::ForkChoiceUseWithWeight(0);
    }

    let prev_segment = input.route.get_segment_last();

    if let Some(prev_segment) = prev_segment {
        let deg_diff = (prev_segment.bearing(input.ctx)
            - input.current_fork_segment.bearing(input.ctx))
        .abs();
        if deg_diff <= input.rules.basic.no_sharp_turns.under_deg {
            return WeightCalcResult::ForkChoiceUseWithWeight(
                input.rules.basic.no_sharp_turns.priority,
            );
        }
    }
    WeightCalcResult::ForkChoiceUseWithWeight(0)
}

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

pub fn weight_check_distance_to_next(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    trace!("weight_check_distance_to_next");

    if !input.rules.basic.progression_direction.enabled {
        return WeightCalcResult::ForkChoiceUseWithWeight(0);
    }
    let check_junctions_back = input.rules.basic.progression_direction.check_junctions_back;

    let distance_to_next_current = match input.route.get_segment_last() {
        None => return WeightCalcResult::ForkChoiceUseWithWeight(0),
        Some(segment) => point_distance(input.ctx, segment.get_end_point(), &input.itinerary.next),
    };

    let check_from = input
        .itinerary
        .switched_wps_on
        .last()
        .map_or(&input.itinerary.start, |value| &value.on_point);
    let distance_to_next_junctions_back = match input
        .route
        .split_at_point(check_from)
        .get_junctions_from_end(input.ctx, check_junctions_back)
    {
        None => return WeightCalcResult::ForkChoiceUseWithWeight(0),
        Some(segment) => point_distance(input.ctx, segment.get_end_point(), &input.itinerary.next),
    };
    trace!(
        distance = distance_to_next_junctions_back,
        "distance to next"
    );

    if distance_to_next_current > distance_to_next_junctions_back {
        return WeightCalcResult::LastSegmentDoNotUse;
    }
    WeightCalcResult::ForkChoiceUseWithWeight(0)
}

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
    segment_tag: Option<String>,
) -> Option<WeightCalcResult> {
    if let Some(ref rule_tag) = rule {
        if let Some(segment_tag) = segment_tag {
            let rule_tag = rule_tag.get(&segment_tag);
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
            if let Some(tag_rule) =
                get_rule_for_tag(&input.rules.smoothness, segment_smoothness(input.ctx, segment))
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
        &input.rules.smoothness,
        segment_smoothness(input.ctx, input.current_fork_segment),
    ) {
        return result;
    }

    WeightCalcResult::ForkChoiceUseWithWeight(0)
}

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
    route_chunk: &Vec<Segment>,
    tag_rule: &Option<HashMap<String, RulesTagValueAction>>,
    tag_getter: F,
) -> bool
where
    F: Fn(&Segment) -> Option<String>,
{
    if let Some(tag_rules) = tag_rule {
        let avoid_rules = tag_rules
            .iter()
            .filter_map(|(key, rule)| match rule {
                RulesTagValueAction::Avoid => Some(key),
                _ => None,
            })
            .collect::<Vec<_>>();
        if route_chunk
            .iter()
            .filter_map(tag_getter)
            .any(|tag| avoid_rules.contains(&&tag))
        {
            return true;
        }
    }
    false
}

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

    use super::get_priority_from_headings;

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

    // TODO: Re-enable this test with tile-based test data
    // This test requires JSON file loading which has been removed
    /*
    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn weight_heading_test() {
            set_graph_static(graph_from_test_dataset(test_dataset_1()));
            let from = MapDataGraph::get()
                .test_get_point_ref_by_id(&885564366)
                .expect("did not find start point");
            let to = MapDataGraph::get()
                .test_get_point_ref_by_id(&33416714)
                .expect("did not find end point");
            let walker = Walker::new(
                from.clone(),
            );

            let fork_point = MapDataGraph::get()
                .test_get_point_ref_by_id(&81272994)
                .expect("to find fork point");

            let segment = get_route_segment(fork_point, from.clone());

            let itinerary = Itinerary::new_start_finish(from.clone(), to.clone(), Vec::new(), 0.);


            let fork_weight = weight_heading(WeightCalcInput {
                route: walker.get_route(),
                itinerary: &itinerary,
                current_fork_segment: &segment,
                walker_from_fork: Walker::new(
                    from.clone(),
                ),
                rules: &RouterRules::default()

            });
            info!("{:#?}", fork_weight);
            assert_eq!(fork_weight, WeightCalcResult::ForkChoiceUseWithWeight(176));
            let fork_point = MapDataGraph::get()
                .test_get_point_ref_by_id(&9212889586)
                .expect("to find fork point");

            let segment = get_route_segment(fork_point, from.clone());

            let fork_weight = weight_heading(WeightCalcInput {
                route: walker.get_route(),
                itinerary: &itinerary,
                current_fork_segment: &segment,
                walker_from_fork: Walker::new(
                    from.clone(),
                ),
                rules: &RouterRules::default()
            });
            info!("{:#?}", fork_weight);
            assert_eq!(fork_weight, WeightCalcResult::ForkChoiceUseWithWeight(64));
        }
    }
    */
}
