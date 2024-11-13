use std::path::PathBuf;

#[derive(Debug)]
pub enum DataDestination {
    Gpx { file: PathBuf },
    Json { file: PathBuf },
}
