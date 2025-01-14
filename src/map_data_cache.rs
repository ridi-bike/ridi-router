use std::{
    fs::File,
    io::{self},
    path::PathBuf,
    time::Instant,
};

use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::info;

use crate::{map_data::graph::MapDataGraphPacked, osm_data_reader::DataSource};

fn read_cache_file(file_folder: &PathBuf, file_name: &str) -> Result<Vec<u8>, MapDataCacheError> {
    let mut file = file_folder.clone();
    file.push(format!("{file_name}.cache"));
    let file_contents =
        std::fs::read(file).map_err(|error| MapDataCacheError::FileError { error })?;

    Ok(file_contents)
}
fn write_cache_file(
    file_folder: &PathBuf,
    file_name: &str,
    file_contents: &Vec<u8>,
) -> Result<(), MapDataCacheError> {
    let mut file = file_folder.clone();
    file.push(format!("{file_name}.cache"));
    std::fs::write(file, file_contents).map_err(|error| MapDataCacheError::FileError { error })?;

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum MapDataCacheError {
    #[error("File error cause {error}")]
    FileError { error: io::Error },

    #[error("IO writer error {error}")]
    IoWriter { error: io::Error },

    #[error("Required cache value is missing")]
    MissingValue,

    #[error("Unexpected value encountered during cache operation")]
    UnexpectedValue,

    #[error("Metadata serialize/deserialize error {error}")]
    MetadataSerde { error: serde_json::Error },
}

#[derive(Debug, Clone)]
enum WriteToCache {
    No,
    WithData(CacheMetadata),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    pub data_source_hash: String,
    pub router_version: String,
}

pub struct MapDataCache {
    data_source: DataSource,
    cache_dir: Option<PathBuf>,
    write_to_cache: WriteToCache,
}

impl MapDataCache {
    pub fn init(cache_dir: Option<PathBuf>, data_source: &DataSource) -> Self {
        Self {
            data_source: data_source.clone(),
            write_to_cache: WriteToCache::No,
            cache_dir,
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn read_input_metadata(&mut self) -> Result<CacheMetadata, MapDataCacheError> {
        let mut file = match &self.data_source {
            DataSource::JsonFile { file } => File::open(file),
            DataSource::PbfFile { file } => File::open(file),
        }
        .map_err(|error| MapDataCacheError::FileError { error })?;

        let mut sha256 = Sha256::new();
        io::copy(&mut file, &mut sha256).map_err(|error| MapDataCacheError::IoWriter { error })?;
        let hash = sha256.finalize();

        let new_metadata = CacheMetadata {
            data_source_hash: format!("{hash:x}"),
            router_version: env!("CARGO_PKG_VERSION").to_string(),
        };

        self.write_to_cache = WriteToCache::WithData(new_metadata.clone());

        info!(
            hash = new_metadata.data_source_hash,
            version = new_metadata.router_version,
            "Cache metadata"
        );

        Ok(new_metadata)
    }
    #[tracing::instrument(skip(self))]
    pub fn read_cache(&mut self) -> Result<Option<MapDataGraphPacked>, MapDataCacheError> {
        let new_metadata = self.read_input_metadata()?;

        let cache_dir = match &self.cache_dir {
            None => {
                self.write_to_cache = WriteToCache::No;
                return Ok(None);
            }
            Some(cd) => cd,
        };

        let read_start = Instant::now();

        if !std::fs::exists(&cache_dir).map_err(|error| MapDataCacheError::FileError { error })? {
            return Ok(None);
        }

        let Some(metadata_file_path) = self.get_metadata_file_path() else {
            return Ok(None);
        };

        let metadata_file = File::open(metadata_file_path)
            .map_err(|error| MapDataCacheError::FileError { error })?;

        let old_metadata: CacheMetadata = serde_json::from_reader(metadata_file)
            .map_err(|error| MapDataCacheError::MetadataSerde { error })?;

        if new_metadata.router_version != old_metadata.router_version
            || new_metadata.data_source_hash != old_metadata.data_source_hash
        {
            return Ok(None);
        }

        self.write_to_cache = WriteToCache::No;

        let mut points: Option<Result<Vec<u8>, MapDataCacheError>> = None;
        let mut point_grid: Option<Result<Vec<u8>, MapDataCacheError>> = None;
        let mut lines: Option<Result<Vec<u8>, MapDataCacheError>> = None;
        let mut tags: Option<Result<Vec<u8>, MapDataCacheError>> = None;
        rayon::scope(|scope| {
            scope.spawn(|_| {
                points = Some(read_cache_file(&cache_dir, "points"));
            });
            scope.spawn(|_| {
                point_grid = Some(read_cache_file(&cache_dir, "point_grid"));
            });
            scope.spawn(|_| {
                lines = Some(read_cache_file(&cache_dir, "lines"));
            });
            scope.spawn(|_| {
                tags = Some(read_cache_file(&cache_dir, "tags"));
            });
        });

        let packed_data = MapDataGraphPacked {
            points: points.ok_or(MapDataCacheError::MissingValue)??,
            point_grid: point_grid.ok_or(MapDataCacheError::MissingValue)??,
            lines: lines.ok_or(MapDataCacheError::MissingValue)??,
            tags: tags.ok_or(MapDataCacheError::MissingValue)??,
        };

        let read_duration = read_start.elapsed();
        info!("cache read took {} seconds", read_duration.as_secs());

        Ok(Some(packed_data))
    }

    #[tracing::instrument(skip(self, packed_data))]
    pub fn write_cache(&self, packed_data: MapDataGraphPacked) -> Result<(), MapDataCacheError> {
        let WriteToCache::WithData(ref new_metadata) = self.write_to_cache else {
            return Ok(());
        };

        let write_start = Instant::now();

        if let Some(cache_dir) = &self.cache_dir {
            if std::fs::exists(&cache_dir)
                .map_err(|error| MapDataCacheError::FileError { error })?
            {
                std::fs::remove_dir_all(&cache_dir)
                    .map_err(|error| MapDataCacheError::FileError { error })?;
            }
            std::fs::create_dir_all(&cache_dir)
                .map_err(|error| MapDataCacheError::FileError { error })?;

            let Some(metadata_file_path) = self.get_metadata_file_path() else {
                return Err(MapDataCacheError::MissingValue);
            };

            let metadata_file = File::create(metadata_file_path)
                .map_err(|error| MapDataCacheError::FileError { error })?;

            serde_json::to_writer(metadata_file, &new_metadata)
                .map_err(|error| MapDataCacheError::MetadataSerde { error })?;

            let tasks = [0u8; 4];
            tasks
                .par_iter()
                .enumerate()
                .map(|(i, _)| match i {
                    0 => write_cache_file(&cache_dir, "points", &packed_data.points),
                    1 => write_cache_file(&cache_dir, "point_grid", &packed_data.point_grid),
                    2 => write_cache_file(&cache_dir, "lines", &packed_data.lines),
                    3 => write_cache_file(&cache_dir, "tags", &packed_data.tags),
                    _ => Err(MapDataCacheError::UnexpectedValue),
                })
                .collect::<Result<Vec<_>, MapDataCacheError>>()?;
        }
        let write_end = write_start.elapsed();
        info!("cache write {}s", write_end.as_secs());
        Ok(())
    }

    fn get_metadata_file_path(&self) -> Option<PathBuf> {
        let Some(mut metadata_file_path) = self.cache_dir.clone() else {
            return None;
        };
        metadata_file_path.push("metadata.json");
        Some(metadata_file_path)
    }
}
