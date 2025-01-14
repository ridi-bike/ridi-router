use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, BufRead, IsTerminal};
use std::{path::PathBuf, str::Utf8Error};

#[derive(Debug, thiserror::Error)]
pub enum RulesError {
    #[error("Failed to read rules file: {error}")]
    FileRead { error: io::Error },

    #[error("Failed to parse file as UTF-8: {error}")]
    FileParse { error: Utf8Error },

    #[error("Failed to parse JSON: {error}")]
    JsonParse { error: serde_json::Error },

    #[error("Failed to read from stdin: {error}")]
    StdinRead { error: io::Error },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "lowercase", deny_unknown_fields)]
pub enum RulesTagValueAction {
    Avoid,
    Priority { value: u8 },
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
            check_junctions_back: 50,
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

#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RouterRules {
    #[serde(default)]
    pub basic: BasicRules,
    pub highway: Option<HashMap<String, RulesTagValueAction>>,
    pub surface: Option<HashMap<String, RulesTagValueAction>>,
    pub smoothness: Option<HashMap<String, RulesTagValueAction>>,
}

impl RouterRules {
    #[tracing::instrument]
    pub fn read_from_file(file: PathBuf) -> Result<Self, RulesError> {
        let file = std::fs::read(file).map_err(|error| RulesError::FileRead { error })?;
        let text =
            std::str::from_utf8(&file[..]).map_err(|error| RulesError::FileParse { error })?;
        let rules: RouterRules =
            serde_json::from_str(text).map_err(|error| RulesError::JsonParse { error })?;

        println!("{}", serde_json::to_string_pretty(&rules).unwrap());
        Ok(rules)
    }

    #[tracing::instrument]
    pub fn read_from_stdin() -> Result<Self, RulesError> {
        let mut text = String::new();
        let stdin = io::stdin();
        let rules: RouterRules = if !stdin.is_terminal() {
            for line in stdin.lock().lines() {
                let line = line.map_err(|error| RulesError::StdinRead { error })?;
                text.push_str(&line);
            }

            serde_json::from_str(&text).map_err(|error| RulesError::JsonParse { error })?
        } else {
            RouterRules::default()
        };

        Ok(rules)
    }

    pub fn read(file: Option<PathBuf>) -> Result<Self, RulesError> {
        match file {
            None => Self::read_from_stdin(),
            Some(file) => Self::read_from_file(file),
        }
    }
}

#[cfg(feature = "rule-schema-writer")]
pub fn generate_json_schema(dest: &PathBuf) -> anyhow::Result<()> {
    let schema = schema_for!(RouterRules);
    let file = std::fs::File::create(dest)?;
    serde_json::to_writer_pretty(file, &schema)?;
    Ok(())
}
