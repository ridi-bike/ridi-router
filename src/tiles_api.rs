use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::rmdf::generator::{manifest::TileManifest, MultiPbfGenerator, TileGenerator};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TileInputSource {
    File(PathBuf),
    Directory(PathBuf),
}

#[derive(Debug, Clone)]
pub struct TileGenerationRequest {
    pub input: TileInputSource,
    pub output_dir: PathBuf,
    pub tile_size_deg: f32,
    pub db_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileGenerationSummary {
    pub output_dir: PathBuf,
    pub tile_count: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum TileGenerationError {
    #[error("Tile size must be between 0 and 180 degrees, got {tile_size_deg}")]
    InvalidTileSize { tile_size_deg: f32 },

    #[error("Input path does not exist: '{path:?}'")]
    InputMissing { path: PathBuf },

    #[error("Expected a PBF file for --input, but found a directory: '{path:?}'")]
    InputFileExpected { path: PathBuf },

    #[error("Expected a directory for --input-dir, but found a file: '{path:?}'")]
    InputDirectoryExpected { path: PathBuf },

    #[error("No PBF files found in '{directory:?}'")]
    NoPbfFilesFound { directory: PathBuf },

    #[error("Invalid output directory '{path:?}': {reason}")]
    InvalidOutputDir { path: PathBuf, reason: String },

    #[error("Failed to create output directory '{path:?}': {error}")]
    OutputDirCreate { path: PathBuf, error: io::Error },

    #[error("Tile generation failed for '{input:?}': {message}")]
    GenerationFailed {
        input: TileInputSource,
        message: String,
    },

    #[error("Failed to read generated manifest '{manifest_path:?}': {error}")]
    ManifestRead {
        manifest_path: PathBuf,
        error: io::Error,
    },

    #[error("Failed to parse generated manifest '{manifest_path:?}': {error}")]
    ManifestParse {
        manifest_path: PathBuf,
        error: serde_json::Error,
    },
}

pub fn generate_tiles(
    request: TileGenerationRequest,
) -> Result<TileGenerationSummary, TileGenerationError> {
    validate_tile_generation_request(&request)?;

    match &request.input {
        TileInputSource::File(input_file) => TileGenerator::new(
            input_file.clone(),
            request.output_dir.clone(),
            request.tile_size_deg,
        )
        .map_err(|error| TileGenerationError::GenerationFailed {
            input: request.input.clone(),
            message: error.to_string(),
        })?
        .generate()
        .map_err(|error| TileGenerationError::GenerationFailed {
            input: request.input.clone(),
            message: error.to_string(),
        })?,
        TileInputSource::Directory(input_dir) => MultiPbfGenerator::new(
            input_dir.clone(),
            request.output_dir.clone(),
            request.tile_size_deg,
            request
                .db_path
                .clone()
                .unwrap_or_else(|| request.output_dir.join(".intermediate.redb")),
        )
        .map_err(|error| TileGenerationError::GenerationFailed {
            input: request.input.clone(),
            message: error.to_string(),
        })?
        .generate()
        .map_err(|error| TileGenerationError::GenerationFailed {
            input: request.input.clone(),
            message: error.to_string(),
        })?,
    }

    read_generation_summary(&request.output_dir)
}

fn validate_tile_generation_request(
    request: &TileGenerationRequest,
) -> Result<(), TileGenerationError> {
    if request.tile_size_deg <= 0.0 || request.tile_size_deg > 180.0 {
        return Err(TileGenerationError::InvalidTileSize {
            tile_size_deg: request.tile_size_deg,
        });
    }

    prepare_output_dir(&request.output_dir)?;

    match &request.input {
        TileInputSource::File(path) => validate_input_file(path),
        TileInputSource::Directory(path) => validate_input_directory(path),
    }
}

fn validate_input_file(path: &Path) -> Result<(), TileGenerationError> {
    if !path.exists() {
        return Err(TileGenerationError::InputMissing {
            path: path.to_path_buf(),
        });
    }

    if path.is_dir() {
        return Err(TileGenerationError::InputFileExpected {
            path: path.to_path_buf(),
        });
    }

    Ok(())
}

fn validate_input_directory(path: &Path) -> Result<(), TileGenerationError> {
    if !path.exists() {
        return Err(TileGenerationError::InputMissing {
            path: path.to_path_buf(),
        });
    }

    if !path.is_dir() {
        return Err(TileGenerationError::InputDirectoryExpected {
            path: path.to_path_buf(),
        });
    }

    let has_pbf_files = fs::read_dir(path)
        .map_err(|error| TileGenerationError::InvalidOutputDir {
            path: path.to_path_buf(),
            reason: format!("failed to read input directory: {error}"),
        })?
        .filter_map(Result::ok)
        .any(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("pbf"));

    if !has_pbf_files {
        return Err(TileGenerationError::NoPbfFilesFound {
            directory: path.to_path_buf(),
        });
    }

    Ok(())
}

fn prepare_output_dir(path: &Path) -> Result<(), TileGenerationError> {
    if path.exists() {
        let metadata = fs::metadata(path).map_err(|error| TileGenerationError::InvalidOutputDir {
            path: path.to_path_buf(),
            reason: format!("failed to read metadata: {error}"),
        })?;

        if !metadata.is_dir() {
            return Err(TileGenerationError::InvalidOutputDir {
                path: path.to_path_buf(),
                reason: "path exists but is not a directory".to_string(),
            });
        }

        return Ok(());
    }

    fs::create_dir_all(path).map_err(|error| TileGenerationError::OutputDirCreate {
        path: path.to_path_buf(),
        error,
    })
}

fn read_generation_summary(output_dir: &Path) -> Result<TileGenerationSummary, TileGenerationError> {
    let manifest_path = output_dir.join("manifest.json");
    let manifest_file = std::fs::File::open(&manifest_path).map_err(|error| {
        TileGenerationError::ManifestRead {
            manifest_path: manifest_path.clone(),
            error,
        }
    })?;
    let manifest: TileManifest =
        serde_json::from_reader(manifest_file).map_err(|error| TileGenerationError::ManifestParse {
            manifest_path,
            error,
        })?;

    Ok(TileGenerationSummary {
        output_dir: output_dir.to_path_buf(),
        tile_count: manifest.tiles.len(),
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

    use super::{generate_tiles, TileGenerationError, TileGenerationRequest, TileInputSource};

    fn unique_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ridi-router-tiles-api-{prefix}-{}-{}",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn tile_generation_request_accepts_file_input_source() {
        let request = TileGenerationRequest {
            input: TileInputSource::File(PathBuf::from("/tmp/input.osm.pbf")),
            output_dir: PathBuf::from("/tmp/output"),
            tile_size_deg: 1.0,
            db_path: None,
        };

        assert!(matches!(request.input, TileInputSource::File(_)));
    }

    #[test]
    fn tile_generation_request_accepts_directory_input_source() {
        let request = TileGenerationRequest {
            input: TileInputSource::Directory(PathBuf::from("/tmp/input-dir")),
            output_dir: PathBuf::from("/tmp/output"),
            tile_size_deg: 1.0,
            db_path: None,
        };

        assert!(matches!(request.input, TileInputSource::Directory(_)));
    }

    #[test]
    fn tile_generation_error_exposes_invalid_output_dir() {
        let input_file = unique_path("input.pbf");
        let output_file = unique_path("output-file");
        fs::write(&input_file, b"not-a-real-pbf").unwrap();
        fs::write(&output_file, b"occupied").unwrap();

        let error = generate_tiles(TileGenerationRequest {
            input: TileInputSource::File(input_file.clone()),
            output_dir: output_file.clone(),
            tile_size_deg: 1.0,
            db_path: None,
        })
        .unwrap_err();

        fs::remove_file(input_file).unwrap();
        fs::remove_file(output_file).unwrap();

        assert!(matches!(error, TileGenerationError::InvalidOutputDir { .. }));
    }
}
