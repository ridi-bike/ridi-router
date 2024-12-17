use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, BufRead, IsTerminal};
use std::{path::PathBuf, str::Utf8Error};

#[derive(Debug)]
pub enum RulesError {
    FileRead { error: io::Error },
    FileParse { error: Utf8Error },
    JsonParse { error: serde_json::Error },
    StdinRead { error: io::Error },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum RulesTagValueAction {
    Avoid,
    Priority { value: u8 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicRulePreferSameRoad {
    pub enabled: bool,
    pub priority: u8,
}

impl Default for BasicRulePreferSameRoad {
    fn default() -> Self {
        Self {
            enabled: true,
            priority: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicRuleProgressDirection {
    pub enabled: bool,
    pub check_steps_back: usize,
}

impl Default for BasicRuleProgressDirection {
    fn default() -> Self {
        Self {
            enabled: true,
            check_steps_back: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicRuleProgressSpeed {
    pub enabled: bool,
    pub check_steps_back: usize,
    pub last_step_distance_below_avg_with_ratio: f32,
}

impl Default for BasicRuleProgressSpeed {
    fn default() -> Self {
        Self {
            enabled: false,
            check_steps_back: 100,
            last_step_distance_below_avg_with_ratio: 1.3,
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct BasicRules {
    #[serde(default)]
    pub prefer_same_road: BasicRulePreferSameRoad,

    #[serde(default)]
    pub progression_direction: BasicRuleProgressDirection,

    #[serde(default)]
    pub progression_speed: BasicRuleProgressSpeed,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
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
