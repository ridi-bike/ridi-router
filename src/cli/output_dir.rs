use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum OutputDirError {
    #[error("Failed to create output directory '{directory:?}': {error}")]
    Create { directory: PathBuf, error: io::Error },

    #[error("Output directory invalid '{directory:?}': {reason}")]
    Invalid { directory: PathBuf, reason: String },

    #[error("Output directory must be empty '{directory:?}'")]
    NotEmpty { directory: PathBuf },
}

pub fn prepare_empty_output_dir(output_dir: &Path) -> Result<(), OutputDirError> {
    if output_dir.exists() {
        let metadata = fs::metadata(output_dir).map_err(|error| OutputDirError::Invalid {
            directory: output_dir.to_path_buf(),
            reason: format!("failed to read metadata: {error}"),
        })?;

        if !metadata.is_dir() {
            return Err(OutputDirError::Invalid {
                directory: output_dir.to_path_buf(),
                reason: "path exists but is not a directory".to_string(),
            });
        }

        let mut entries = fs::read_dir(output_dir).map_err(|error| OutputDirError::Invalid {
            directory: output_dir.to_path_buf(),
            reason: format!("failed to read directory: {error}"),
        })?;

        if entries
            .next()
            .transpose()
            .map_err(|error| OutputDirError::Invalid {
                directory: output_dir.to_path_buf(),
                reason: format!("failed while reading directory contents: {error}"),
            })?
            .is_some()
        {
            return Err(OutputDirError::NotEmpty {
                directory: output_dir.to_path_buf(),
            });
        }

        return Ok(());
    }

    fs::create_dir_all(output_dir).map_err(|error| OutputDirError::Create {
        directory: output_dir.to_path_buf(),
        error,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        process,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{prepare_empty_output_dir, OutputDirError};

    fn unique_test_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "ridi-router-output-dir-{}-{}",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn prepare_output_dir_accepts_missing_then_createable_dir() {
        let output_dir = unique_test_dir();

        prepare_empty_output_dir(&output_dir).unwrap();

        assert!(output_dir.is_dir());
        assert_eq!(fs::read_dir(&output_dir).unwrap().count(), 0);
        fs::remove_dir_all(output_dir).unwrap();
    }

    #[test]
    fn prepare_output_dir_accepts_existing_empty_dir() {
        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();

        prepare_empty_output_dir(&output_dir).unwrap();

        fs::remove_dir_all(output_dir).unwrap();
    }

    #[test]
    fn prepare_output_dir_rejects_non_empty_dir() {
        let output_dir = unique_test_dir();
        fs::create_dir_all(&output_dir).unwrap();
        fs::write(output_dir.join("already-there.txt"), "x").unwrap();

        let error = prepare_empty_output_dir(&output_dir).unwrap_err();
        fs::remove_dir_all(output_dir).unwrap();

        assert!(matches!(error, OutputDirError::NotEmpty { .. }));
    }
}
