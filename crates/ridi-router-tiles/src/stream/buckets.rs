//! On-disk spill files and per-region bucket files.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::Result;

/// Append-only file of framed batches: [u32 len][payload]. Writers append whole batches under a
/// mutex; readers index the frames and read them in parallel with `pread`.
pub struct SpillWriter {
    out: Mutex<BufWriter<File>>,
    pub path: PathBuf,
}

impl SpillWriter {
    pub fn create(path: &Path) -> Result<Self> {
        let f = File::create(path)?;
        Ok(Self {
            out: Mutex::new(BufWriter::with_capacity(4 << 20, f)),
            path: path.to_path_buf(),
        })
    }

    pub fn write_batch(&self, payload: &[u8]) -> Result<()> {
        if payload.is_empty() {
            return Ok(());
        }
        let mut out = self.out.lock().unwrap();
        out.write_all(&(payload.len() as u32).to_le_bytes())?;
        out.write_all(payload)?;
        Ok(())
    }

    pub fn finish(self) -> Result<PathBuf> {
        self.out.into_inner().unwrap().flush()?;
        Ok(self.path)
    }
}

pub struct SpillReader {
    file: File,
    pub frames: Vec<(u64, u32)>,
}

impl SpillReader {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        let mut frames = Vec::new();
        let mut off = 0u64;
        let mut lb = [0u8; 4];
        while off + 4 <= len {
            file.read_exact_at(&mut lb, off)?;
            let l = u32::from_le_bytes(lb);
            frames.push((off + 4, l));
            off += 4 + l as u64;
        }
        Ok(Self { file, frames })
    }

    pub fn read_frame(&self, i: usize) -> Result<Vec<u8>> {
        let (off, l) = self.frames[i];
        let mut buf = vec![0u8; l as usize];
        self.file.read_exact_at(&mut buf, off)?;
        Ok(buf)
    }
}

pub const REC_WAY: u8 = 1;
pub const REC_POLY: u8 = 2;
pub const REC_RESTRICTION: u8 = 3;

/// One append-only file per region. File handles are opened lazily and kept open; each region
/// has its own mutex so appends of whole buffers never interleave.
pub struct RegionBuckets {
    dir: PathBuf,
    files: HashMap<u32, (OnceLock<File>, Mutex<()>)>,
}

impl RegionBuckets {
    pub fn new(dir: &Path, regions: impl Iterator<Item = u32>) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let files = regions
            .map(|r| (r, (OnceLock::new(), Mutex::new(()))))
            .collect();
        Ok(Self {
            dir: dir.to_path_buf(),
            files,
        })
    }

    pub fn path_for(dir: &Path, region: u32) -> PathBuf {
        dir.join(format!("r{region}.bin"))
    }

    fn append(&self, region: u32, data: &[u8]) -> Result<()> {
        let (cell, lock) = self
            .files
            .get(&region)
            .unwrap_or_else(|| panic!("region {region} not registered"));
        let file = match cell.get() {
            Some(f) => f,
            None => {
                let f = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(Self::path_for(&self.dir, region))?;
                let _ = cell.set(f);
                cell.get().unwrap()
            }
        };
        let _g = lock.lock().unwrap();
        (&*file).write_all(data)?;
        Ok(())
    }

    pub fn local(&self) -> LocalBuckets<'_> {
        LocalBuckets {
            buckets: self,
            bufs: HashMap::new(),
            total: 0,
        }
    }
}

/// Per-worker write buffers; flushed when they exceed the threshold and on drop.
pub struct LocalBuckets<'a> {
    buckets: &'a RegionBuckets,
    bufs: HashMap<u32, Vec<u8>>,
    total: usize,
}

const LOCAL_FLUSH_BYTES: usize = 16 << 20;

impl LocalBuckets<'_> {
    /// Append a framed record `[kind][varint len][payload]` to `region`.
    pub fn push(&mut self, region: u32, kind: u8, payload: &[u8]) -> Result<()> {
        let buf = self.bufs.entry(region).or_default();
        let before = buf.len();
        buf.push(kind);
        super::util::put_uvarint(buf, payload.len() as u64);
        buf.extend_from_slice(payload);
        self.total += buf.len() - before;
        if self.total > LOCAL_FLUSH_BYTES {
            self.flush()?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        for (region, buf) in self.bufs.iter_mut() {
            if !buf.is_empty() {
                self.buckets.append(*region, buf)?;
                buf.clear();
            }
        }
        self.bufs.retain(|_, b| b.capacity() < (1 << 20));
        self.total = 0;
        Ok(())
    }
}

impl Drop for LocalBuckets<'_> {
    fn drop(&mut self) {
        if let Err(e) = self.flush() {
            tracing::error!("bucket flush failed: {e:#}");
            panic!("bucket flush failed: {e:#}");
        }
    }
}
