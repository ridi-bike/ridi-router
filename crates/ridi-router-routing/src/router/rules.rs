use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "action", rename_all = "lowercase", deny_unknown_fields)]
pub enum RulesTagValueAction {
    Avoid,
    Priority { value: u8 },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BasicRuleStepLimit(pub u32);

impl Default for BasicRuleStepLimit {
    fn default() -> Self {
        Self(60000)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BasicRulePreferSameRoad {
    pub enabled: bool,
    pub priority: u8,
}

impl Default for BasicRulePreferSameRoad {
    fn default() -> Self {
        Self {
            enabled: true,
            priority: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BasicRuleProgressDirection {
    pub enabled: bool,
    pub check_junctions_back: usize,
}

impl Default for BasicRuleProgressDirection {
    fn default() -> Self {
        Self {
            enabled: true,
            check_junctions_back: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BasicRuleProgressSpeed {
    pub enabled: bool,
    pub check_steps_back: usize,
    pub last_step_distance_below_avg_with_ratio: f32,
}

impl Default for BasicRuleProgressSpeed {
    fn default() -> Self {
        Self {
            enabled: false,
            check_steps_back: 1000,
            last_step_distance_below_avg_with_ratio: 1.3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BasicRuleNoShortDetour {
    pub enabled: bool,
    pub min_detour_len_m: f32,
}

impl Default for BasicRuleNoShortDetour {
    fn default() -> Self {
        Self {
            enabled: true,
            min_detour_len_m: 5000.,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BasicRuleNoSharpTurns {
    pub enabled: bool,
    pub under_deg: f32,
    pub priority: u8,
}

impl Default for BasicRuleNoSharpTurns {
    fn default() -> Self {
        Self {
            enabled: true,
            under_deg: 150.,
            priority: 60,
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BasicRules {
    #[serde(default)]
    pub step_limit: BasicRuleStepLimit,

    #[serde(default)]
    pub prefer_same_road: BasicRulePreferSameRoad,

    #[serde(default)]
    pub progression_direction: BasicRuleProgressDirection,

    #[serde(default)]
    pub progression_speed: BasicRuleProgressSpeed,

    #[serde(default)]
    pub no_short_detours: BasicRuleNoShortDetour,

    #[serde(default)]
    pub no_sharp_turns: BasicRuleNoSharpTurns,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenerationRulesStartFinish {
    /// Fixed waypoint distances from start and finish. When set, these are used instead of
    /// `variation_distance_ratios`.
    #[serde(default)]
    pub variation_distances_m: Vec<f32>,
    /// Waypoint distances from start and finish as fractions of the start-finish distance.
    #[serde(default = "default_start_finish_variation_distance_ratios")]
    pub variation_distance_ratios: Vec<f32>,
    /// Lower bound for ratio-based waypoint distances, so very short trips still get waypoints
    /// far enough out to reach other roads.
    #[serde(default = "default_start_finish_variation_distance_min_m")]
    pub variation_distance_min_m: f32,
    #[serde(default)]
    pub variation_bearing_deg: Vec<f32>,
}

fn default_start_finish_variation_distance_ratios() -> Vec<f32> {
    vec![0.1, 0.2, 0.3]
}

fn default_start_finish_variation_distance_min_m() -> f32 {
    2000.
}

impl Default for GenerationRulesStartFinish {
    fn default() -> Self {
        Self {
            variation_distances_m: Vec::new(),
            variation_distance_ratios: default_start_finish_variation_distance_ratios(),
            variation_distance_min_m: default_start_finish_variation_distance_min_m(),
            variation_bearing_deg: vec![0., 45., 90., 135., 180., 225., 270., 315.],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenerationRulesRoundTrip {
    #[serde(default)]
    pub variation_distance_ratios: Vec<f32>,
    #[serde(default)]
    pub variation_bearing_deg: Vec<f32>,
}

impl Default for GenerationRulesRoundTrip {
    fn default() -> Self {
        Self {
            variation_distance_ratios: vec![1.0, 0.8, 0.6, 0.4],
            variation_bearing_deg: vec![-25., -10., 10., 25.],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenerationRulesRetry {
    #[serde(default)]
    pub trigger_min_route_count: usize,
    #[serde(default)]
    pub round_trip_adjustment_bearing_deg: Vec<f32>,
    #[serde(default)]
    pub avoid_residential: Vec<bool>,
}

impl Default for GenerationRulesRetry {
    fn default() -> Self {
        Self {
            trigger_min_route_count: 50,
            round_trip_adjustment_bearing_deg: vec![-135., -90., -45., 45., 90., 135.],
            avoid_residential: vec![true, false],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenerationRulesWaypoints {
    #[serde(default)]
    pub start_finish: GenerationRulesStartFinish,
    #[serde(default)]
    pub round_trip: GenerationRulesRoundTrip,
    /// Itineraries whose snapped waypoints are all within this fraction of the trip size
    /// (start-finish distance, or round-trip target distance) of each other are routed once.
    #[serde(default = "default_dedupe_tolerance_ratio")]
    pub dedupe_tolerance_ratio: f32,
    /// Lower bound for the itinerary dedupe tolerance.
    #[serde(default = "default_dedupe_tolerance_min_m")]
    pub dedupe_tolerance_min_m: f32,
}

fn default_dedupe_tolerance_ratio() -> f32 {
    0.02
}

fn default_dedupe_tolerance_min_m() -> f32 {
    300.
}

impl Default for GenerationRulesWaypoints {
    fn default() -> Self {
        Self {
            start_finish: GenerationRulesStartFinish::default(),
            round_trip: GenerationRulesRoundTrip::default(),
            dedupe_tolerance_ratio: default_dedupe_tolerance_ratio(),
            dedupe_tolerance_min_m: default_dedupe_tolerance_min_m(),
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenerationRules {
    #[serde(default)]
    pub waypoint_generation: GenerationRulesWaypoints,
    #[serde(default)]
    pub route_generation_retry: GenerationRulesRetry,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RouterRules {
    #[serde(default)]
    pub basic: BasicRules,
    pub highway: Option<HashMap<String, RulesTagValueAction>>,
    pub surface: Option<HashMap<String, RulesTagValueAction>>,
    pub smoothness: Option<HashMap<String, RulesTagValueAction>>,
    #[serde(default)]
    pub generation: GenerationRules,
}

#[cfg(feature = "rule-schema-writer")]
#[allow(dead_code)]
pub fn generate_json_schema(dest: &std::path::Path) -> anyhow::Result<()> {
    let schema = schemars::schema_for!(RouterRules);
    let file = std::fs::File::create(dest)?;
    serde_json::to_writer_pretty(file, &schema)?;
    Ok(())
}
