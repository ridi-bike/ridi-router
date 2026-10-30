//! Run configuration, read once from environment variables. Every setting has a discovered or
//! fixed default; an unparsable value fails the run with the variable's name.
//!
//! | Variable                       | Default                                  |
//! |--------------------------------|------------------------------------------|
//! | `RIDI_TILES_MEMORY_MB`         | cgroup `memory.max`, else `MemTotal`     |
//! | `RIDI_TILES_THREADS`           | all available cores                      |
//! | `RIDI_TILES_SCRATCH_DIR`       | the output directory                     |
//! | `RIDI_TILES_STORE_MB`          | 45% of the memory budget                 |
//! | `RIDI_TILES_JOB_MEMORY_MB`     | (memory - 2 GB) / 24, 64 MB..2 GB        |
//! | `RIDI_TILES_JOB_TARGET_MB`     | total bucket size / (4 x threads), 4..24 MB |
//! | `RIDI_TILES_JOB_THREADS`       | `RIDI_TILES_THREADS`                     |
//! | `RIDI_TILES_WRITER_THREADS`    | 4 per job                                |
//! | `RIDI_TILES_KEEP_SCRATCH`      | 0 (scratch is deleted at the end)        |
//! | `RIDI_TILES_SKIP_SPACE_CHECK`  | 0                                        |
//! | `RIDI_TILES_DROP_OUTPUT_CACHE` | 1 (written tiles are flushed from the page cache) |
//!
//! Test and benchmark only: `RIDI_TILES_GRID_ANCHOR=reference` (legacy per-input proximity
//! grid), `RIDI_TILES_STOP_AFTER=index|areas`, `RIDI_TILES_VERIFY_PROXIMITY=1` (checks every
//! proximity shortcut against the reference maths; slow).

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::util;

#[derive(Debug, Clone)]
pub struct Config {
    pub memory_bytes: u64,
    pub threads: usize,
    /// Directory the run's own scratch directory is created in.
    pub scratch_base: PathBuf,
    pub store_bytes: u64,
    pub job_memory_bytes: u64,
    /// None: derived from the total bucket size once it is known.
    pub job_target_bytes: Option<u64>,
    pub job_threads: usize,
    pub writer_threads: usize,
    pub keep_scratch: bool,
    pub skip_space_check: bool,
    pub drop_output_cache: bool,
    pub reference_anchor: bool,
    pub stop_after: Option<StopAfter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopAfter {
    Index,
    Areas,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn parse<T: std::str::FromStr>(name: &str) -> Result<Option<T>>
where
    T::Err: std::fmt::Display,
{
    var(name)
        .map(|v| {
            v.parse::<T>()
                .map_err(|e| anyhow::anyhow!("{name}={v:?} is not valid: {e}"))
        })
        .transpose()
}

fn flag(name: &str) -> Result<Option<bool>> {
    match var(name).as_deref() {
        None => Ok(None),
        Some("1" | "true" | "yes") => Ok(Some(true)),
        Some("0" | "false" | "no") => Ok(Some(false)),
        Some(v) => bail!("{name}={v:?} is not valid: expected 0 or 1"),
    }
}

fn megabytes(name: &str) -> Result<Option<u64>> {
    match parse::<u64>(name)? {
        Some(0) => bail!("{name} must be greater than 0"),
        v => Ok(v.map(|mb| mb << 20)),
    }
}

impl Config {
    pub fn from_env(output_dir: &Path) -> Result<Self> {
        let memory_bytes =
            megabytes("RIDI_TILES_MEMORY_MB")?.unwrap_or_else(util::memory_limit_bytes);
        let threads = match parse::<usize>("RIDI_TILES_THREADS")? {
            Some(0) => bail!("RIDI_TILES_THREADS must be greater than 0"),
            Some(n) => n,
            None => std::thread::available_parallelism().map_or(4, |n| n.get()),
        };
        let reference_anchor = match var("RIDI_TILES_GRID_ANCHOR").as_deref() {
            None | Some("global") => false,
            Some("reference") => true,
            Some(other) => {
                bail!("RIDI_TILES_GRID_ANCHOR must be 'global' or 'reference', got {other:?}")
            }
        };
        let stop_after = match var("RIDI_TILES_STOP_AFTER").as_deref() {
            None => None,
            Some("index") => Some(StopAfter::Index),
            Some("areas") => Some(StopAfter::Areas),
            Some(other) => bail!("RIDI_TILES_STOP_AFTER must be 'index' or 'areas', got {other:?}"),
        };
        let positive = |name: &str| -> Result<Option<usize>> {
            match parse::<usize>(name)? {
                Some(0) => bail!("{name} must be greater than 0"),
                v => Ok(v),
            }
        };
        Ok(Self {
            memory_bytes,
            threads,
            scratch_base: var("RIDI_TILES_SCRATCH_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| output_dir.to_path_buf()),
            store_bytes: megabytes("RIDI_TILES_STORE_MB")?.unwrap_or(memory_bytes / 100 * 45),
            job_memory_bytes: megabytes("RIDI_TILES_JOB_MEMORY_MB")?.unwrap_or_else(|| {
                // Poland measured ~23x RSS per in-flight bucket byte (incl. allocator retention)
                (memory_bytes.saturating_sub(2 << 30) / 24).clamp(64 << 20, 2 << 30)
            }),
            job_target_bytes: megabytes("RIDI_TILES_JOB_TARGET_MB")?,
            job_threads: positive("RIDI_TILES_JOB_THREADS")?.unwrap_or(threads),
            writer_threads: positive("RIDI_TILES_WRITER_THREADS")?.unwrap_or(4),
            keep_scratch: flag("RIDI_TILES_KEEP_SCRATCH")?.unwrap_or(false),
            skip_space_check: flag("RIDI_TILES_SKIP_SPACE_CHECK")?.unwrap_or(false),
            drop_output_cache: flag("RIDI_TILES_DROP_OUTPUT_CACHE")?.unwrap_or(true),
            reference_anchor,
            stop_after,
        })
    }

    /// Create this run's scratch directory (unique also for concurrent runs in one process).
    pub fn create_scratch_dir(&self) -> Result<PathBuf> {
        static RUN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = self.scratch_base.join(format!(
            ".ridi-scratch-{}-{}",
            std::process::id(),
            RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).with_context(|| format!("create scratch dir {path:?}"))?;
        Ok(path)
    }
}
