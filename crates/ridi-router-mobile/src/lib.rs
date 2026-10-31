use std::path::{Path, PathBuf};

use ridi_router_routing::{RouteRequest, RoutingExecutor, RoutingExecutorConfig};
use serde::Serialize;

uniffi::include_scaffolding!("ridi_router_mobile");

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RidiRouterErrorCode {
    InvalidRequestJson,
    InvalidTilesDir,
    RoutingOpenFailed,
    RouteGenerationFailed,
    ResponseSerializationFailed,
    Unknown,
}

#[derive(Debug, Serialize)]
struct RidiRouterError {
    code: RidiRouterErrorCode,
    message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum RidiRouterResult<T: Serialize> {
    Ok { ok: AlwaysTrue, value: T },
    Err { ok: AlwaysFalse, error: RidiRouterError },
}

#[derive(Debug)]
struct AlwaysTrue;

#[derive(Debug)]
struct AlwaysFalse;

impl Serialize for AlwaysTrue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bool(true)
    }
}

impl Serialize for AlwaysFalse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bool(false)
    }
}

pub fn generate_route_json(tiles_dir: String, request_json: String) -> String {
    match generate_route_json_inner(&tiles_dir, &request_json) {
        Ok(json) => json,
        Err(error) => serialize_error(error),
    }
}

fn generate_route_json_inner(tiles_dir: &str, request_json: &str) -> Result<String, RidiRouterError> {
    let tiles_dir = validate_tiles_dir(tiles_dir)?;

    let request = serde_json::from_str::<RouteRequest>(request_json).map_err(|error| {
        RidiRouterError::new(
            RidiRouterErrorCode::InvalidRequestJson,
            "Failed to parse request JSON",
            Some(error.to_string()),
        )
    })?;

    let executor = RoutingExecutor::open(RoutingExecutorConfig {
        tiles_dir: tiles_dir.clone(),
    })
    .map_err(|error| {
        RidiRouterError::new(
            RidiRouterErrorCode::RoutingOpenFailed,
            "Failed to open routing tiles",
            Some(error.to_string()),
        )
    })?;

    let computation = executor.generate(request, None).map_err(|error| {
        RidiRouterError::new(
            RidiRouterErrorCode::RouteGenerationFailed,
            "Failed to generate route",
            Some(error.to_string()),
        )
    })?;

    serialize_result(&RidiRouterResult::Ok {
        ok: AlwaysTrue,
        value: computation,
    })
}

fn validate_tiles_dir(tiles_dir: &str) -> Result<PathBuf, RidiRouterError> {
    let path = Path::new(tiles_dir);

    let invalid = |details: String| {
        RidiRouterError::new(
            RidiRouterErrorCode::InvalidTilesDir,
            "Invalid tiles directory",
            Some(details),
        )
    };

    if tiles_dir.trim().is_empty() {
        return Err(invalid("tiles_dir is empty".to_string()));
    }

    if !path.is_absolute() {
        return Err(invalid("tiles_dir must be an absolute path".to_string()));
    }

    if !path.is_dir() {
        return Err(invalid(format!("tiles_dir is not a directory: {tiles_dir}")));
    }

    let manifest_path = path.join("manifest.json");
    if !manifest_path.is_file() {
        return Err(invalid(format!(
            "tiles_dir does not contain manifest.json: {}",
            manifest_path.display()
        )));
    }

    Ok(path.to_path_buf())
}

fn serialize_result<T: Serialize>(result: &RidiRouterResult<T>) -> Result<String, RidiRouterError> {
    serde_json::to_string(result).map_err(|error| {
        RidiRouterError::new(
            RidiRouterErrorCode::ResponseSerializationFailed,
            "Failed to serialize routing response",
            Some(error.to_string()),
        )
    })
}

fn serialize_error(error: RidiRouterError) -> String {
    let fallback = || {
        r#"{"ok":false,"error":{"code":"unknown","message":"Failed to serialize routing error"}}"#
            .to_string()
    };

    serde_json::to_string(&RidiRouterResult::<()>::Err {
        ok: AlwaysFalse,
        error,
    })
    .unwrap_or_else(|_| fallback())
}

impl RidiRouterError {
    fn new(code: RidiRouterErrorCode, message: &'static str, details: Option<String>) -> Self {
        Self {
            code,
            message,
            details,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ridi_router_test_support::rmdf::{
        create_linear_single_tile_fixture, unique_test_dir, SYNTHETIC_FINISH_LAT,
        SYNTHETIC_FINISH_LON, SYNTHETIC_START_LAT, SYNTHETIC_START_LON,
    };
    use serde_json::Value;

    fn parse_result(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    fn valid_request_json() -> String {
        serde_json::json!({
            "mode": {
                "StartFinish": {
                    "start": { "lat": SYNTHETIC_START_LAT, "lon": SYNTHETIC_START_LON },
                    "finish": { "lat": SYNTHETIC_FINISH_LAT, "lon": SYNTHETIC_FINISH_LON }
                }
            },
            "rules": {
                "basic": { "step_limit": 50 },
                "highway": null,
                "surface": null,
                "smoothness": null,
                "generation": {
                    "waypoint_generation": {
                        "start_finish": {
                            "variation_distances_m": [],
                            "variation_bearing_deg": []
                        }
                    },
                    "route_generation_retry": {
                        "trigger_min_route_count": 1,
                        "round_trip_adjustment_bearing_deg": [],
                        "avoid_residential": [false]
                    }
                }
            }
        })
        .to_string()
    }

    #[test]
    fn invalid_request_json_returns_error_envelope() {
        let fixture = create_linear_single_tile_fixture("mobile-invalid-request");
        let result = parse_result(&generate_route_json(
            fixture.dir.display().to_string(),
            "not json".to_string(),
        ));

        assert_eq!(result["ok"], false);
        assert_eq!(result["error"]["code"], "invalid_request_json");
    }

    #[test]
    fn invalid_tiles_dir_returns_error_envelope() {
        let result = parse_result(&generate_route_json(
            "relative/path".to_string(),
            valid_request_json(),
        ));

        assert_eq!(result["ok"], false);
        assert_eq!(result["error"]["code"], "invalid_tiles_dir");
    }

    #[test]
    fn missing_manifest_returns_invalid_tiles_dir() {
        let dir = unique_test_dir("mobile-missing-manifest");
        std::fs::create_dir_all(&dir).unwrap();

        let result = parse_result(&generate_route_json(
            dir.display().to_string(),
            valid_request_json(),
        ));

        assert_eq!(result["ok"], false);
        assert_eq!(result["error"]["code"], "invalid_tiles_dir");
    }

    #[test]
    fn routing_open_failure_returns_error_envelope() {
        let dir = unique_test_dir("mobile-open-failure");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manifest.json"), b"not json").unwrap();

        let result = parse_result(&generate_route_json(
            dir.display().to_string(),
            valid_request_json(),
        ));

        assert_eq!(result["ok"], false);
        assert_eq!(result["error"]["code"], "routing_open_failed");
    }

    #[test]
    fn route_generation_failure_returns_error_envelope() {
        let fixture = create_linear_single_tile_fixture("mobile-generation-failure");
        let request = serde_json::json!({
            "mode": {
                "StartFinish": {
                    "start": { "lat": 0.0, "lon": 0.0 },
                    "finish": { "lat": SYNTHETIC_FINISH_LAT, "lon": SYNTHETIC_FINISH_LON }
                }
            },
            "rules": serde_json::from_str::<Value>(&valid_request_json()).unwrap()["rules"].clone()
        })
        .to_string();

        let result = parse_result(&generate_route_json(
            fixture.dir.display().to_string(),
            request,
        ));

        assert_eq!(result["ok"], false);
        assert_eq!(result["error"]["code"], "route_generation_failed");
    }

    #[test]
    fn valid_request_returns_success_envelope() {
        let fixture = create_linear_single_tile_fixture("mobile-success");
        let result = parse_result(&generate_route_json(
            fixture.dir.display().to_string(),
            valid_request_json(),
        ));

        assert_eq!(result["ok"], true);
        assert!(result["value"].is_object());
    }
}
