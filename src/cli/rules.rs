use std::{
    io::{self, BufRead, IsTerminal},
    path::PathBuf,
    str::Utf8Error,
};

use tracing::trace;

use crate::router::rules::RouterRules;

#[derive(Debug, thiserror::Error)]
pub enum RuleFileError {
    #[error("Failed to read rules file: {error}")]
    FileRead { error: io::Error },

    #[error("Failed to parse file as UTF-8: {error}")]
    FileParse { error: Utf8Error },

    #[error("Failed to parse JSON: {error}")]
    JsonParse { error: serde_json::Error },

    #[error("Failed to read from stdin: {error}")]
    StdinRead { error: io::Error },
}

pub fn read_router_rules(file: Option<PathBuf>) -> Result<RouterRules, RuleFileError> {
    match file {
        Some(path) => read_router_rules_from_file(path),
        None => read_router_rules_from_stdin(),
    }
}

pub fn read_router_rules_from_file(file: PathBuf) -> Result<RouterRules, RuleFileError> {
    let file = std::fs::read(file).map_err(|error| RuleFileError::FileRead { error })?;
    let text = std::str::from_utf8(&file[..]).map_err(|error| RuleFileError::FileParse { error })?;
    let rules =
        serde_json::from_str(text).map_err(|error| RuleFileError::JsonParse { error })?;

    trace!(
        rules = serde_json::to_string_pretty(&rules).unwrap(),
        "Rules from file"
    );

    Ok(rules)
}

pub fn read_router_rules_from_stdin() -> Result<RouterRules, RuleFileError> {
    let mut text = String::new();
    let stdin = io::stdin();
    if !stdin.is_terminal() {
        for line in stdin.lock().lines() {
            let line = line.map_err(|error| RuleFileError::StdinRead { error })?;
            text.push_str(&line);
        }

        return serde_json::from_str(&text).map_err(|error| RuleFileError::JsonParse { error });
    }

    Ok(RouterRules::default())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        process,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::router::rules::RulesTagValueAction;

    use super::{read_router_rules_from_file, RuleFileError};

    fn unique_test_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "ridi-router-rules-{}-{}.json",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn router_runner_parses_rule_file_into_rust_rule_values() {
        let path = unique_test_path();
        fs::write(
            &path,
            r#"{
                "highway": {
                    "primary": { "action": "avoid" }
                }
            }"#,
        )
        .unwrap();

        let rules = read_router_rules_from_file(path.clone()).unwrap();
        fs::remove_file(path).unwrap();

        assert!(matches!(
            rules.highway.as_ref().and_then(|values| values.get("primary")),
            Some(RulesTagValueAction::Avoid)
        ));
    }

    #[test]
    fn read_router_rules_reports_typed_json_parse_error() {
        let path = unique_test_path();
        fs::write(&path, b"{not-json").unwrap();

        let error = read_router_rules_from_file(path.clone()).unwrap_err();
        fs::remove_file(path).unwrap();

        assert!(matches!(error, RuleFileError::JsonParse { .. }));
    }
}
