use std::collections::HashMap;

use geo::{Bearing, Haversine, Point};
use tracing::{error, trace};

use crate::router::rules::{RouterRules, RulesTagValueAction};

use super::{
    itinerary::Itinerary,
    navigator::WeightCalcResult,
    route::{segment::Segment, segment_list::SegmentList, Route},
    walker::{Walker, WalkerMoveResult},
};

pub struct WeightCalcInput<'a> {
    pub current_fork_segment: &'a Segment,
    pub route: &'a Route,
    pub all_fork_segments: &'a SegmentList,
    pub itinerary: &'a Itinerary,
    pub walker_from_fork: Walker,
    pub rules: &'a RouterRules,
}

pub type WeightCalc = fn(input: WeightCalcInput) -> WeightCalcResult;

pub fn weight_heading(input: WeightCalcInput) -> WeightCalcResult {
    trace!("weight_heading");

    let mut walker = input.walker_from_fork;
    let next_fork = match walker.move_forward_to_next_fork(|p| input.itinerary.is_finished(p)) {
        Ok(v) => v,
        Err(e) => {
            error!("weight calc error {:#?}", e);
            return WeightCalcResult::DoNotUse;
        }
    };
    let _ = match next_fork {
        WalkerMoveResult::DeadEnd => return WeightCalcResult::DoNotUse,
        WalkerMoveResult::Finish => return WeightCalcResult::UseWithWeight(255),
        WalkerMoveResult::Fork(f) => f,
    };
    let fork_segment = match walker.get_route().get_segment_last() {
        Some(last_segment) => last_segment,
        None => input.current_fork_segment,
    };
    let fork_point_geo = Point::new(
        fork_segment.get_end_point().borrow().lon,
        fork_segment.get_end_point().borrow().lat,
    );
    let next_point_geo = Point::new(
        input.itinerary.get_next().borrow().lon,
        input.itinerary.get_next().borrow().lat,
    );

    let next_bearing = Haversine::bearing(fork_point_geo, next_point_geo);
    let fork_line_one_geo = Point::new(
        fork_segment.get_line().borrow().points.0.borrow().lon,
        fork_segment.get_line().borrow().points.0.borrow().lat,
    );
    let fork_line_two_geo = Point::new(
        fork_segment.get_line().borrow().points.1.borrow().lon,
        fork_segment.get_line().borrow().points.1.borrow().lat,
    );
    let fork_bearing = if &fork_segment.get_line().borrow().points.1 == fork_segment.get_end_point()
    {
        Haversine::bearing(fork_line_one_geo, fork_line_two_geo)
    } else {
        Haversine::bearing(fork_line_two_geo, fork_line_one_geo)
    };

    let degree_offset_from_next = (fork_bearing - next_bearing).abs();

    let ratio: f32 = 255.0 / 180.0;

    WeightCalcResult::UseWithWeight(255 - (degree_offset_from_next / ratio).round() as u8)
}

pub fn weight_prefer_same_road(input: WeightCalcInput) -> WeightCalcResult {
    trace!("weight_prefer_same_road");
    if !input.rules.basic.prefer_same_road.enabled {
        return WeightCalcResult::UseWithWeight(0);
    }
    let current_ref = input
        .route
        .get_segment_last()
        .map_or(None, |s| s.get_line().borrow().tags.borrow().hw_ref());
    let current_name = input
        .route
        .get_segment_last()
        .map_or(None, |s| s.get_line().borrow().tags.borrow().name());
    let fork_ref = input
        .current_fork_segment
        .get_line()
        .borrow()
        .tags
        .borrow()
        .hw_ref();
    let fork_name = input
        .current_fork_segment
        .get_line()
        .borrow()
        .tags
        .borrow()
        .name();

    if (current_ref.is_some() && fork_ref.is_some() && current_ref == fork_ref)
        || (current_name.is_some() && fork_name.is_some() && current_name == fork_name)
    {
        return WeightCalcResult::UseWithWeight(input.rules.basic.prefer_same_road.priority);
    }

    WeightCalcResult::UseWithWeight(0)
}

pub fn weight_no_loops(input: WeightCalcInput) -> WeightCalcResult {
    trace!("weight_no_loops");
    if input.route.has_looped() {
        return WeightCalcResult::DoNotUse;
    }

    WeightCalcResult::UseWithWeight(0)
}

pub fn weight_no_sharp_turns(input: WeightCalcInput) -> WeightCalcResult {
    trace!("weight_no_sharp_turns");

    if !input.rules.basic.no_sharp_turns.enabled {
        return WeightCalcResult::UseWithWeight(0);
    }

    let prev_segment = input.route.get_segment_last();

    if let Some(prev_segment) = prev_segment {
        let deg_diff =
            (prev_segment.get_bearing() - input.current_fork_segment.get_bearing()).abs();
        if deg_diff <= input.rules.basic.no_sharp_turns.under_deg {
            return WeightCalcResult::UseWithWeight(input.rules.basic.no_sharp_turns.priority);
        }
    }
    WeightCalcResult::UseWithWeight(0)
}

pub fn weight_no_short_detours(input: WeightCalcInput) -> WeightCalcResult {
    trace!("weight_no_short_detours");
    if !input.rules.basic.no_short_detours.enabled {
        return WeightCalcResult::UseWithWeight(0);
    }

    let hw_ref = input
        .current_fork_segment
        .get_line()
        .borrow()
        .tags
        .borrow()
        .hw_ref()
        .cloned();
    let hw_name = input
        .current_fork_segment
        .get_line()
        .borrow()
        .tags
        .borrow()
        .name()
        .cloned();
    if input.route.is_back_on_road_within_distance(
        hw_ref,
        hw_name,
        input.rules.basic.no_short_detours.min_detour_len_m,
    ) {
        return WeightCalcResult::DoNotUse;
    }

    WeightCalcResult::UseWithWeight(0)
}

pub fn weight_check_distance_to_next(input: WeightCalcInput) -> WeightCalcResult {
    trace!("weight_check_distance_to_next");

    if !input.rules.basic.progression_direction.enabled {
        return WeightCalcResult::UseWithWeight(0);
    }
    let check_junctions_back = input.rules.basic.progression_direction.check_junctions_back;

    let distance_to_next_current = match input.route.get_segment_last() {
        None => return WeightCalcResult::UseWithWeight(0),
        Some(segment) => segment
            .get_end_point()
            .borrow()
            .distance_between(&input.itinerary.get_next()),
    };

    let distance_to_next_junctions_back =
        match input.route.get_junctions_from_end(check_junctions_back) {
            None => return WeightCalcResult::UseWithWeight(0),
            Some(segment) => segment
                .get_end_point()
                .borrow()
                .distance_between(&input.itinerary.get_next()),
        };
    trace!(
        distance = distance_to_next_junctions_back,
        "distance to next"
    );

    if distance_to_next_current > distance_to_next_junctions_back {
        return WeightCalcResult::DoNotUse;
    }
    WeightCalcResult::UseWithWeight(0)
}

pub fn weight_progress_speed(input: WeightCalcInput) -> WeightCalcResult {
    trace!("weight_progress_speed");

    if !input.rules.basic.progression_speed.enabled {
        return WeightCalcResult::UseWithWeight(0);
    }

    let check_steps_back = input.rules.basic.progression_speed.check_steps_back;

    let current_point = match input.route.get_segment_last() {
        None => return WeightCalcResult::UseWithWeight(0),
        Some(segment) => segment.get_end_point(),
    };

    let total_distance = input
        .itinerary
        .get_start()
        .borrow()
        .distance_between(&input.itinerary.get_next());
    let point_steps_back = match input.route.get_segments_from_end(check_steps_back) {
        None => return WeightCalcResult::UseWithWeight(0),
        Some(segment) => segment.get_end_point().clone(),
    };

    let average_distance_per_segment = total_distance / (input.route.get_segment_count() as f32);

    let distance_last_points = point_steps_back.borrow().distance_between(&current_point);
    let average_distance_last_points = distance_last_points / (check_steps_back as f32);

    if average_distance_last_points
        < average_distance_per_segment
            * input
                .rules
                .basic
                .progression_speed
                .last_step_distance_below_avg_with_ratio
    {
        return WeightCalcResult::DoNotUse;
    }

    WeightCalcResult::UseWithWeight(0)
}

fn get_rule_for_tag(
    rule: &Option<HashMap<String, RulesTagValueAction>>,
    segment_tag: Option<&smartstring::alias::String>,
) -> Option<WeightCalcResult> {
    if let Some(ref rule_tag) = rule {
        if let Some(segment_tag) = segment_tag {
            let rule_tag = rule_tag.get(&segment_tag.to_string());
            if let Some(rule_tag) = rule_tag {
                return Some(match rule_tag {
                    RulesTagValueAction::Avoid => WeightCalcResult::DoNotUse,
                    RulesTagValueAction::Priority { value } => {
                        WeightCalcResult::UseWithWeight(*value)
                    }
                });
            }
        }
    }
    None
}

pub fn weight_rules_highway(input: WeightCalcInput) -> WeightCalcResult {
    trace!("weight_rules_highway");

    if let Some(res) = get_rule_for_tag(
        &input.rules.highway,
        input
            .current_fork_segment
            .get_line()
            .borrow()
            .tags
            .borrow()
            .highway(),
    ) {
        return res;
    }

    WeightCalcResult::UseWithWeight(0)
}

pub fn weight_rules_surface(input: WeightCalcInput) -> WeightCalcResult {
    trace!("weight_rules_surface");

    if let Some(res) = get_rule_for_tag(
        &input.rules.surface,
        input
            .current_fork_segment
            .get_line()
            .borrow()
            .tags
            .borrow()
            .surface(),
    ) {
        return res;
    }

    WeightCalcResult::UseWithWeight(0)
}

pub fn weight_rules_smoothness(input: WeightCalcInput) -> WeightCalcResult {
    trace!("weight_rules_smoothness");

    if let Some(res) = get_rule_for_tag(
        &input.rules.smoothness,
        input
            .current_fork_segment
            .get_line()
            .borrow()
            .tags
            .borrow()
            .smoothness(),
    ) {
        return res;
    }

    WeightCalcResult::UseWithWeight(0)
}

#[cfg(test)]
mod test {

    use std::path::PathBuf;

    use rusty_fork::rusty_fork_test;
    use tracing::info;

    use crate::{
        map_data::graph::{MapDataGraph, MapDataPointRef},
        router::{
            itinerary::Itinerary,
            navigator::WeightCalcResult,
            route::{segment::Segment, segment_list::SegmentList},
            rules::RouterRules,
            walker::Walker,
        },
        test_utils::{graph_from_test_file, set_graph_static},
    };

    use super::{weight_heading, WeightCalcInput};

    fn get_route_segment(
        end_point: MapDataPointRef,
        opposite_point_for_line: MapDataPointRef,
    ) -> Segment {
        let end_point_borrowed = end_point.borrow();
        let line = end_point_borrowed
            .lines
            .iter()
            .find(|line| {
                let line = line.borrow();
                (line.points.0 == end_point && line.points.1 == opposite_point_for_line)
                    || (line.points.1 == end_point && line.points.0 == opposite_point_for_line)
            })
            .expect("line to be found");

        Segment::new(line.clone(), end_point.clone())
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn weight_heading_test() {
            set_graph_static(graph_from_test_file(&PathBuf::from("test-data/sigulda-100.json")));
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
                all_fork_segments: &SegmentList::from(vec![]),
                current_fork_segment: &segment,
                walker_from_fork: Walker::new(
                    from.clone(),
                ),
                rules: &RouterRules::default()

            });
            info!("{:#?}", fork_weight);
            assert_eq!(fork_weight, WeightCalcResult::UseWithWeight(215));

            let fork_point = MapDataGraph::get()
                .test_get_point_ref_by_id(&9212889586)
                .expect("to find fork point");

            let segment = get_route_segment(fork_point, from.clone());

            let fork_weight = weight_heading(WeightCalcInput {
                route: walker.get_route(),
                itinerary: &itinerary,
                all_fork_segments: &SegmentList::from(vec![]),
                current_fork_segment: &segment,
                walker_from_fork: Walker::new(
                    from.clone(),
                ),
                rules: &RouterRules::default()
            });
            info!("{:#?}", fork_weight);
            assert_eq!(fork_weight, WeightCalcResult::UseWithWeight(96));
        }
    }
}
