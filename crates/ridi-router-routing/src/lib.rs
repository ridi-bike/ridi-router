#[cfg(test)]
mod generated_tiles_phase4_tests;
mod map_data;
mod rmdf;
mod route_output;
mod router;
mod routing_api;
pub(crate) mod routing_context;
#[cfg(test)]
mod test_utils;

pub use route_output::{ComputedRoute, RouteComputation};
pub use router::route::{RouteStatElement, RouteStats};
pub use router::rules::{RouterRules, RulesTagValueAction};
pub use routing_api::{
    Coords, RouteMode, RouteRequest, RoutingError, RoutingExecutor, RoutingExecutorConfig,
    RoutingGenerationError, RoutingOpenError,
};
pub(crate) use routing_context::RoutingContext;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_public_api_does_not_expose_cli_types() {
        let cargo_toml =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
        assert!(!cargo_toml.contains("clap.workspace"));
        assert!(!cargo_toml.contains("gpx.workspace"));
        assert!(!cargo_toml.contains("json-tools"));
    }

    #[test]
    fn routing_library_does_not_require_rule_file_path_input() {
        let _request = RouteRequest {
            mode: RouteMode::StartFinish {
                start: Coords { lat: 1.0, lon: 2.0 },
                finish: Coords { lat: 3.0, lon: 4.0 },
            },
            rules: RouterRules::default(),
        };
    }
}
