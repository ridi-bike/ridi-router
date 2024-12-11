use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, BufRead};
use std::{path::PathBuf, str::Utf8Error};

#[derive(Debug)]
pub enum HintsError {
    FileRead { error: io::Error },
    FileParse { error: Utf8Error },
    JsonParse { error: serde_json::Error },
    StdinRead { error: io::Error },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum HintTagValueAction {
    Avoid,
    Priority { value: u8 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterHints {
    pub highway: Option<HashMap<String, HintTagValueAction>>,
    pub surface: Option<HashMap<String, HintTagValueAction>>,
    pub smoothness: Option<HashMap<String, HintTagValueAction>>,
}

impl RouterHints {
    pub fn read_from_file(file: PathBuf) -> Result<Self, HintsError> {
        let file = std::fs::read(file).map_err(|error| HintsError::FileRead { error })?;
        let text =
            std::str::from_utf8(&file[..]).map_err(|error| HintsError::FileParse { error })?;
        let hints: RouterHints =
            serde_json::from_str(text).map_err(|error| HintsError::JsonParse { error })?;

        Ok(hints)
    }

    pub fn read_from_stdin() -> Result<Self, HintsError> {
        let mut text = String::new();
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = line.map_err(|error| HintsError::StdinRead { error })?;
            text.push_str(&line);
        }

        let hints: RouterHints =
            serde_json::from_str(&text).map_err(|error| HintsError::JsonParse { error })?;

        Ok(hints)
    }

    pub fn read(file: Option<PathBuf>) -> Result<Self, HintsError> {
        match file {
            None => Self::read_from_stdin(),
            Some(file) => Self::read_from_file(file),
        }
    }
}
