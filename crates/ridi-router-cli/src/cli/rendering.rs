use crate::router_runner::RouterRunnerError;

pub fn render_user_error(error: &RouterRunnerError) -> String {
    match error {
        RouterRunnerError::OutputDirectory { error } => format!("Output error: {error}"),
        RouterRunnerError::Rules { error } => format!("Rule file error: {error}"),
        RouterRunnerError::Coords { .. } => format!("Invalid coordinates: {error}"),
        RouterRunnerError::InvalidTileGenerationArgs { .. } => {
            format!("Tile generation argument error: {error}")
        }
        RouterRunnerError::Routing { error } => format!("Routing error: {error}"),
        RouterRunnerError::TileGeneration { error } => format!("Tile generation error: {error}"),
        RouterRunnerError::ResultWrite { error } => {
            format!("Failed to write route output: {error}")
        }
        #[cfg(feature = "rule-schema-writer")]
        RouterRunnerError::RuleSchema { error } => format!("Rule schema error: {error}"),
        #[cfg(feature = "rmdf-viewer")]
        RouterRunnerError::RmdfViewer { error } => format!("RMDF viewer error: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ridi_router_routing::{RoutingError, RoutingOpenError};
    use ridi_router_tiles::TileGenerationError;

    use super::render_user_error;
    use crate::router_runner::RouterRunnerError;

    #[test]
    fn cli_human_error_rendering_wraps_typed_library_errors() {
        let routing_error = RouterRunnerError::Routing {
            error: RoutingError::Open(RoutingOpenError::ConflictingTilesDir {
                existing_tiles_dir: PathBuf::from("/tiles/a"),
                requested_tiles_dir: PathBuf::from("/tiles/b"),
            }),
        };

        let rendered = render_user_error(&routing_error);
        assert!(rendered.starts_with("Routing error:"));
        assert!(rendered.contains("/tiles/a"));
        assert!(rendered.contains("/tiles/b"));
    }

    #[test]
    fn cli_human_error_rendering_wraps_typed_tile_generation_errors() {
        let error = RouterRunnerError::TileGeneration {
            error: TileGenerationError::InputMissing {
                path: PathBuf::from("/tmp/missing.osm.pbf"),
            },
        };

        let rendered = render_user_error(&error);
        assert!(rendered.starts_with("Tile generation error:"));
        assert!(rendered.contains("missing.osm.pbf"));
    }
}
