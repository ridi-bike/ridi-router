use serde::{Deserialize, Serialize};

use crate::{
    router::{generator::RouteWithStats, route::RouteStats},
    RoutingContext,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputedRoute {
    pub coords: Vec<(f32, f32)>,
    pub stats: RouteStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteComputation {
    pub routes: Vec<ComputedRoute>,
}

#[hotpath::measure_all]
impl ComputedRoute {
    fn from_route(ctx: &RoutingContext<'_>, route: RouteWithStats) -> Self {
        Self {
            coords: route
                .route
                .into_iter()
                .map(|segment| ctx.point_coords(segment.get_end_point()))
                .collect(),
            stats: route.stats,
        }
    }
}

#[hotpath::measure_all]
impl RouteComputation {
    pub(crate) fn from_routes(ctx: &RoutingContext<'_>, routes: Vec<RouteWithStats>) -> Self {
        Self {
            routes: routes
                .into_iter()
                .map(|route| ComputedRoute::from_route(ctx, route))
                .collect(),
        }
    }
}
