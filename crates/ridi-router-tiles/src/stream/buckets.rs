//! On-disk spill files and per-region bucket files.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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

/// One append-only file per region, written through per-region buffers.
///
/// Records arrive in way-id order, which is spread over the whole planet, so writing each worker's
/// batch straight to the files would make millions of appends of a few hundred bytes (measured on
/// the planet: ~7 M write IOs for ~20 GB of buckets, with partial pages written back repeatedly).
/// Instead, records collect in a shared buffer per region; a region is written once its buffer
/// reaches `REGION_WRITE_BYTES`, and when all buffers together exceed the budget the largest ones
/// are written. Record order within a region file is irrelevant (the local stage sorts by id).
pub struct RegionBuckets {
    dir: PathBuf,
    regions: HashMap<u32, Region>,
    buffered: AtomicU64,
    budget: u64,
    spilling: Mutex<()>,
}

#[derive(Default)]
struct Region {
    file: OnceLock<File>,
    buf: Mutex<Vec<u8>>,
    /// serializes file writes of this region (a buffer taken by one thread and a later one taken
    /// by another may be written concurrently otherwise)
    write: Mutex<()>,
}

const REGION_WRITE_BYTES: usize = 1 << 20;

impl RegionBuckets {
    pub fn new(dir: &Path, regions: impl Iterator<Item = u32>, budget: u64) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            regions: regions.map(|r| (r, Region::default())).collect(),
            buffered: AtomicU64::new(0),
            budget,
            spilling: Mutex::new(()),
        })
    }

    pub fn path_for(dir: &Path, region: u32) -> PathBuf {
        dir.join(format!("r{region}.bin"))
    }

    fn region(&self, region: u32) -> &Region {
        self.regions
            .get(&region)
            .unwrap_or_else(|| panic!("region {region} not registered"))
    }

    fn append(&self, region: u32, data: &[u8]) -> Result<()> {
        let r = self.region(region);
        let full = {
            let mut buf = r.buf.lock().unwrap();
            buf.extend_from_slice(data);
            (buf.len() >= REGION_WRITE_BYTES).then(|| std::mem::take(&mut *buf))
        };
        let total = self
            .buffered
            .fetch_add(data.len() as u64, Ordering::Relaxed)
            + data.len() as u64;
        if let Some(full) = full {
            self.write(region, r, &full)?;
        }
        if total > self.budget {
            self.write_largest()?;
        }
        Ok(())
    }

    fn write(&self, region: u32, r: &Region, data: &[u8]) -> Result<()> {
        let _w = r.write.lock().unwrap();
        let file = match r.file.get() {
            Some(f) => f,
            None => {
                let f = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(Self::path_for(&self.dir, region))?;
                let _ = r.file.set(f);
                r.file.get().unwrap()
            }
        };
        (&*file).write_all(data)?;
        self.buffered
            .fetch_sub(data.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    /// Write the largest buffers until the total is back under half the budget (one thread at a
    /// time; others carry on buffering).
    fn write_largest(&self) -> Result<()> {
        let Ok(_g) = self.spilling.try_lock() else {
            return Ok(());
        };
        let mut sizes: Vec<(usize, u32)> = self
            .regions
            .iter()
            .map(|(id, r)| (r.buf.lock().unwrap().len(), *id))
            .filter(|(n, _)| *n > 0)
            .collect();
        sizes.sort_unstable_by(|a, b| b.cmp(a));
        for (_, id) in sizes {
            if self.buffered.load(Ordering::Relaxed) <= self.budget / 2 {
                break;
            }
            let r = self.region(id);
            let data = std::mem::take(&mut *r.buf.lock().unwrap());
            if !data.is_empty() {
                self.write(id, r, &data)?;
            }
        }
        Ok(())
    }

    /// Write all buffered records; call before the bucket files are read.
    pub fn finish(&self) -> Result<()> {
        for (id, r) in &self.regions {
            let data = std::mem::take(&mut *r.buf.lock().unwrap());
            if !data.is_empty() {
                self.write(*id, r, &data)?;
            }
        }
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
