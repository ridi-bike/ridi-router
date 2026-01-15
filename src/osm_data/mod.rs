use crate::map_data::MapDataError;
use std::{io, path::PathBuf};

pub mod pbf_area_reader;

#[derive(Debug, thiserror::Error)]
pub enum OsmDataReaderError {
    #[error("Map data error: {error}")]
    MapDataError { error: MapDataError },

    #[error("File error: {error}")]
    FileError { error: io::Error },

    #[error("Failed to open PBF file: {error}")]
    PbfFileOpenError { error: io::Error },

    #[error("Failed to read PBF file: {error}")]
    PbfFileReadError { error: osmpbfreader::Error },

    #[error("PBF file error: {error}")]
    PbfFileError { error: String },

    #[error("Unexpected element")]
    UnexpectedElement,
}

#[derive(Debug, PartialEq, Clone)]
pub enum DataSource {
    PbfFile { file: PathBuf },
}
