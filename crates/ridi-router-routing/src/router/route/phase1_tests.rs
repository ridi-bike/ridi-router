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

fn end_point_id(segment: Option<&Segment>, ctx: &RoutingContext<'_>) -> Option<u64> {
    segment.map(|segment| ctx.point(segment.get_end_point()).id)
}

#[test]
fn nth_junction_since_missing_boundary_scans_from_start() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 6, 8, 4]);
    let boundary = test_ctx.point(11);

    let from_start = route.nth_junction_from_end_since_idx(&ctx, 0, 2);
    let planned = route.nth_junction_from_end_since_point(&ctx, &boundary, 2);

    assert_eq!(end_point_id(from_start, &ctx), end_point_id(planned, &ctx));
}

#[test]
fn nth_junction_since_boundary_at_start_uses_boundary_index() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 6, 8, 4]);
    let boundary = test_ctx.point(2);
    let since_idx = route.route_index_last_for_point(&boundary).unwrap();

    let from_boundary = route.nth_junction_from_end_since_idx(&ctx, since_idx, 2);
    let planned = route.nth_junction_from_end_since_point(&ctx, &boundary, 2);

    assert_eq!(
        end_point_id(from_boundary, &ctx),
        end_point_id(planned, &ctx)
    );
}

#[test]
fn nth_junction_since_boundary_in_middle_uses_boundary_index() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 6, 8, 4]);
    let boundary = test_ctx.point(6);
    let since_idx = route.route_index_last_for_point(&boundary).unwrap();

    let from_boundary = route.nth_junction_from_end_since_idx(&ctx, since_idx, 2);
    let planned = route.nth_junction_from_end_since_point(&ctx, &boundary, 2);

    assert_eq!(
        end_point_id(from_boundary, &ctx),
        end_point_id(planned, &ctx)
    );
}

#[test]
fn nth_junction_since_boundary_returns_none_when_suffix_is_too_short() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 4]);
    let boundary = test_ctx.point(2);

    let planned = route.nth_junction_from_end_since_point(&ctx, &boundary, 2);

    assert_eq!(end_point_id(planned, &ctx), None);
}

#[test]
fn repeated_boundary_point_uses_latest_route_occurrence() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 6, 3, 4]);
    let boundary = test_ctx.point(3);
    let since_idx = route.route_index_last_for_point(&boundary).unwrap();

    let from_latest_boundary = route.nth_junction_from_end_since_idx(&ctx, since_idx, 2);
    let planned = route.nth_junction_from_end_since_point(&ctx, &boundary, 2);

    assert_eq!(since_idx, 3);
    assert_eq!(
        end_point_id(from_latest_boundary, &ctx),
        end_point_id(planned, &ctx)
    );
    assert_eq!(end_point_id(planned, &ctx), None);
}

#[test]
fn route_index_last_for_point_uses_detector_history() {
    let test_ctx = RoutingTestContext::new(test_dataset_1());
    let ctx = test_ctx.resolver();
    let route = route_from_points(&test_ctx, &ctx, &[1, 2, 3, 6, 3, 4]);

    assert_eq!(
        route.route_index_last_for_point(&test_ctx.point(3)),
        Some(3)
    );
    assert_eq!(route.route_index_last_for_point(&test_ctx.point(11)), None);
}
