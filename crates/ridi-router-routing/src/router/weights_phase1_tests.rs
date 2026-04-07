use crate::{
    router::{
        itinerary::{Itinerary, WaypointHistoryElement},
        navigator::WeightCalcResult,
        route::{segment::Segment, Route},
        rules::RouterRules,
        walker::Walker,
        weights::{weight_check_distance_to_next, WeightCalcInput},
    },
    test_utils::{test_dataset_1, RoutingTestContext},
    RoutingContext,
};

fn segment_between(
    test_ctx: &RoutingTestContext,
    ctx: &RoutingContext<'_>,
    from_id: u64,
    to_id: u64,
) -> Segment {
    let from = test_ctx.point(from_id);
    let to = test_ctx.point(to_id);
    let line = ctx
        .point(&from)
        .lines
        .iter()
        .find(|line_ref| {
            let line = ctx.line(line_ref);
            (line.points.0 == from && line.points.1 == to)
                || (line.points.0 == to && line.points.1 == from)
        })
        .cloned()
        .unwrap_or_else(|| panic!("missing test line between {from_id} and {to_id}"));

    Segment::new(line, to)
}

fn route_from_points(
    test_ctx: &RoutingTestContext,
    ctx: &RoutingContext<'_>,
    point_ids: &[u64],
) -> Route {
    let mut route = Route::new();
    for window in point_ids.windows(2) {
        route.add_segment(ctx, segment_between(test_ctx, ctx, window[0], window[1]));
    }
    route
}

fn base_rules(check_junctions_back: usize) -> RouterRules {
    let mut rules = RouterRules::default();
    rules.basic.progression_direction.enabled = true;
    rules.basic.progression_direction.check_junctions_back = check_junctions_back;
    rules
}

fn run_weight(
    test_ctx: &RoutingTestContext,
    ctx: &RoutingContext<'_>,
    route: &Route,
    itinerary: &Itinerary,
    rules: &RouterRules,
) -> WeightCalcResult {
    let current_fork_segment = segment_between(test_ctx, ctx, 3, 5);
    weight_check_distance_to_next(WeightCalcInput {
        current_fork_segment: &current_fork_segment,
        route,
        itinerary,
        walker_from_fork: Walker::new(test_ctx.point(5)),
        rules,
        ctx,
    })
}

#[test]
fn disabled_rule_returns_zero_weight() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 6]);
    let itinerary =
        Itinerary::new_start_finish(test_ctx.point(1), test_ctx.point(7), Vec::new(), 0.0);
    let mut rules = RouterRules::default();
    rules.basic.progression_direction.enabled = false;

    assert_eq!(
        run_weight(&test_ctx, &ctx, &route, &itinerary, &rules),
        WeightCalcResult::ForkChoiceUseWithWeight(0)
    );
}

#[test]
fn empty_route_returns_zero_weight() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = Route::new();
    let itinerary =
        Itinerary::new_start_finish(test_ctx.point(1), test_ctx.point(7), Vec::new(), 0.0);
    let rules = base_rules(1);

    assert_eq!(
        run_weight(&test_ctx, &ctx, &route, &itinerary, &rules),
        WeightCalcResult::ForkChoiceUseWithWeight(0)
    );
}

#[test]
fn not_enough_junctions_back_returns_zero_weight() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 4]);
    let itinerary =
        Itinerary::new_start_finish(test_ctx.point(1), test_ctx.point(7), Vec::new(), 0.0);
    let rules = base_rules(2);

    assert_eq!(
        run_weight(&test_ctx, &ctx, &route, &itinerary, &rules),
        WeightCalcResult::ForkChoiceUseWithWeight(0)
    );
}

#[test]
fn returns_last_segment_do_not_use_when_current_is_farther_from_next() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 6, 8, 4]);
    let itinerary =
        Itinerary::new_start_finish(test_ctx.point(1), test_ctx.point(7), Vec::new(), 0.0);
    let rules = base_rules(2);

    assert_eq!(
        run_weight(&test_ctx, &ctx, &route, &itinerary, &rules),
        WeightCalcResult::LastSegmentDoNotUse
    );
}

#[test]
fn returns_zero_when_current_is_not_farther_from_next() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 4, 8]);
    let itinerary =
        Itinerary::new_start_finish(test_ctx.point(1), test_ctx.point(7), Vec::new(), 0.0);
    let rules = base_rules(2);

    assert_eq!(
        run_weight(&test_ctx, &ctx, &route, &itinerary, &rules),
        WeightCalcResult::ForkChoiceUseWithWeight(0)
    );
}

#[test]
fn switched_waypoint_boundary_limits_the_checked_suffix() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 6, 8, 4]);
    let mut itinerary =
        Itinerary::new_start_finish(test_ctx.point(1), test_ctx.point(7), Vec::new(), 0.0);
    itinerary.switched_wps_on.push(WaypointHistoryElement {
        on_point: test_ctx.point(8),
        from_point: test_ctx.point(6),
    });
    let rules = base_rules(2);

    assert_eq!(
        run_weight(&test_ctx, &ctx, &route, &itinerary, &rules),
        WeightCalcResult::ForkChoiceUseWithWeight(0)
    );
}

#[test]
fn repeated_boundary_point_uses_first_match_in_route_history() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 6, 3, 4]);
    let mut itinerary =
        Itinerary::new_start_finish(test_ctx.point(1), test_ctx.point(7), Vec::new(), 0.0);
    itinerary.switched_wps_on.push(WaypointHistoryElement {
        on_point: test_ctx.point(3),
        from_point: test_ctx.point(2),
    });
    let rules = base_rules(2);

    // This pins the current caveat: split_at_point() starts from the first matching point in the
    // route history, not from the conceptual waypoint-switch moment when the same point repeats.
    assert_eq!(
        run_weight(&test_ctx, &ctx, &route, &itinerary, &rules),
        WeightCalcResult::LastSegmentDoNotUse
    );
}
