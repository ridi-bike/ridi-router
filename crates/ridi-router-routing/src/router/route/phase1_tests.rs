use crate::{
    router::route::{segment::Segment, Route},
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

fn planned_nth_junction_from_end_since_point(
    route: &Route,
    ctx: &RoutingContext<'_>,
    boundary: &crate::map_data::graph::MapDataPointRef,
    num_of_junctions: usize,
) -> Option<Segment> {
    let since_idx = route
        .route_segments
        .iter()
        .position(|segment| segment.get_end_point() == boundary)
        .unwrap_or(0);

    if route.route_segments.len().saturating_sub(since_idx) < num_of_junctions + 1 {
        return None;
    }

    let mut junction_count = 0;
    for segment in route.route_segments[since_idx..].iter().rev() {
        if ctx.point(segment.get_end_point()).is_junction() {
            junction_count += 1;
        }
        if junction_count == num_of_junctions {
            return Some(segment.clone());
        }
    }

    None
}

fn end_point_id(segment: Option<&Segment>, ctx: &RoutingContext<'_>) -> Option<u64> {
    segment.map(|segment| ctx.point(segment.get_end_point()).id)
}

#[test]
fn route_query_equivalence_when_boundary_is_missing() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 6, 8, 4]);
    let boundary = test_ctx.point(11);

    let current = route
        .split_at_point(&boundary)
        .get_junctions_from_end(&ctx, 2);
    let planned = planned_nth_junction_from_end_since_point(&route, &ctx, &boundary, 2);

    assert_eq!(
        end_point_id(current.as_ref(), &ctx),
        end_point_id(planned.as_ref(), &ctx)
    );
}

#[test]
fn route_query_equivalence_when_boundary_is_at_start() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 6, 8, 4]);
    let boundary = test_ctx.point(2);

    let current = route
        .split_at_point(&boundary)
        .get_junctions_from_end(&ctx, 2);
    let planned = planned_nth_junction_from_end_since_point(&route, &ctx, &boundary, 2);

    assert_eq!(
        end_point_id(current.as_ref(), &ctx),
        end_point_id(planned.as_ref(), &ctx)
    );
}

#[test]
fn route_query_equivalence_when_boundary_is_in_the_middle() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 6, 8, 4]);
    let boundary = test_ctx.point(6);

    let current = route
        .split_at_point(&boundary)
        .get_junctions_from_end(&ctx, 2);
    let planned = planned_nth_junction_from_end_since_point(&route, &ctx, &boundary, 2);

    assert_eq!(
        end_point_id(current.as_ref(), &ctx),
        end_point_id(planned.as_ref(), &ctx)
    );
}

#[test]
fn route_query_equivalence_when_there_are_not_enough_junctions() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 4]);
    let boundary = test_ctx.point(2);

    let current = route
        .split_at_point(&boundary)
        .get_junctions_from_end(&ctx, 2);
    let planned = planned_nth_junction_from_end_since_point(&route, &ctx, &boundary, 2);

    assert_eq!(
        end_point_id(current.as_ref(), &ctx),
        end_point_id(planned.as_ref(), &ctx)
    );
}

#[test]
fn route_query_equivalence_preserves_repeated_boundary_point_behavior() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 6, 3, 4]);
    let boundary = test_ctx.point(3);

    let current = route
        .split_at_point(&boundary)
        .get_junctions_from_end(&ctx, 2);
    let planned = planned_nth_junction_from_end_since_point(&route, &ctx, &boundary, 2);

    // The future helper must preserve the current split_at_point() caveat and use the first
    // matching boundary point when the same point appears multiple times in the route history.
    assert_eq!(
        end_point_id(current.as_ref(), &ctx),
        end_point_id(planned.as_ref(), &ctx)
    );
    assert_eq!(end_point_id(planned.as_ref(), &ctx), Some(6));
}
