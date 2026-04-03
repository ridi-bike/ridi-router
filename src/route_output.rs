use serde::{Deserialize, Serialize};

use crate::router::{generator::RouteWithStats, route::RouteStats};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputedRoute {
    pub coords: Vec<(f32, f32)>,
    pub stats: RouteStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteComputation {
    pub routes: Vec<ComputedRoute>,
}

impl From<RouteWithStats> for ComputedRoute {
    fn from(route: RouteWithStats) -> Self {
        Self {
            coords: route
                .route
                .into_iter()
                .map(|segment| {
                    (
                        segment.get_end_point().get().lat,
                        segment.get_end_point().get().lon,
                    )
                })
                .collect(),
            stats: route.stats,
        }
    }
}

impl From<Vec<RouteWithStats>> for RouteComputation {
    fn from(routes: Vec<RouteWithStats>) -> Self {
        Self {
            routes: routes.into_iter().map(ComputedRoute::from).collect(),
        }
    }
}
