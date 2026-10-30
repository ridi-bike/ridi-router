//! Coordinate store + the spill/coordinate join.
//!
//! The node pass writes coordinates of needed nodes into slot `rank(id)` (slots grow with id, so
//! each PBF blob writes a few contiguous runs).
//!
//! * If the store fits the memory budget it lives in anonymous memory (`Mem`) and spill records
//!   are resolved by direct lookup.
//! * Otherwise it is written sequentially to a file and joined in `K = ceil(size / budget)`
//!   slot-range chunks: passes 0..K-1 each load one chunk into memory (sequential read) and
//!   write, per spill frame, the coordinates of the refs falling in that chunk in ref order
//!   ("fill" files, sequential). The final pass loads the last chunk and merges the fill streams.
//!   All IO is sequential; extra cost is (K-1) re-reads of the spill + refs*8 bytes of fills.
//!
//! This replaces round 0's mmap store, whose random page faults degrade badly once the store
//! no longer fits in page cache.

use std::fs::{File, OpenOptions};
use std::os::unix::fs::FileExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use anyhow::Result;
use rayon::prelude::*;
use tracing::info;

use super::buckets::SpillReader;
use super::nodeidx::{MemStore, RankedSet};

pub enum Store {
    Mem(MemStore),
    File { file: File, path: PathBuf, count: u64 },
}

impl Store {
    pub fn create(scratch: &Path, count: u64, budget_bytes: u64) -> Result<Self> {
        if count * 8 <= budget_bytes {
            info!(
                "Coordinate store: in memory ({} MB, budget {} MB)",
                count * 8 >> 20,
                budget_bytes >> 20
            );
            Ok(Store::Mem(MemStore::new(count)))
        } else {
            let path = scratch.join("node_coords.bin");
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&path)?;
            file.set_len(count * 8)?;
            info!(
                "Coordinate store: file {:?} ({} MB > budget {} MB)",
                path,
                count * 8 >> 20,
                budget_bytes >> 20
            );
            Ok(Store::File { file, path, count })
        }
    }

    /// Write a run of consecutive slots.
    pub fn write_run(&self, first_slot: u64, vals: &[u64]) -> Result<()> {
        match self {
            Store::Mem(m) => m.set_range(first_slot, vals),
            Store::File { file, .. } => {
                let bytes: &[u8] = bytemuck::cast_slice(vals);
                file.write_all_at(bytes, first_slot * 8)?;
            }
        }
        Ok(())
    }

    fn count(&self) -> u64 {
        match self {
            Store::Mem(m) => m.len(),
            Store::File { count, .. } => *count,
        }
    }

    fn load_chunk(&self, start: u64, end: u64) -> Result<Vec<u64>> {
        let Store::File { file, .. } = self else {
            unreachable!()
        };
        let mut v = vec![0u64; (end - start) as usize];
        let bytes: &mut [u8] = bytemuck::cast_slice_mut(&mut v);
        // read in parallel 64 MB pieces
        bytes
            .par_chunks_mut(64 << 20)
            .enumerate()
            .try_for_each(|(i, piece)| file.read_exact_at(piece, start * 8 + (i as u64) * (64 << 20)))?;
        unsafe {
            libc::posix_fadvise(
                file.as_raw_fd(),
                (start * 8) as i64,
                ((end - start) * 8) as i64,
                libc::POSIX_FADV_DONTNEED,
            );
        }
        Ok(v)
    }

    pub fn remove_file(&self) {
        if let Store::File { path, .. } = self {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Joins spill frames with coordinates.
pub struct Resolver<'a> {
    pub set: &'a RankedSet,
    pub store: &'a Store,
    pub budget_bytes: u64,
    pub scratch: &'a Path,
}

impl Resolver<'_> {
    /// Call `f(state, frame_index, frame_bytes, coords)` for every spill frame, where `coords`
    /// holds the encoded coordinate (0 = missing) of every node ref of the frame, in the order
    /// `refs_of` lists them.
    pub fn for_each_frame<S, I, F>(
        &self,
        spill: &SpillReader,
        label: &'static str,
        refs_of: fn(&[u8], &mut Vec<i64>),
        init: I,
        f: F,
    ) -> Result<()>
    where
        I: Fn() -> S + Sync + Send,
        F: Fn(&mut S, usize, &[u8], &[u64]) -> Result<()> + Sync + Send,
    {
        let nframes = spill.frames.len();
        let progress = super::util::Progress::new(label, nframes as u64, "frames", 1);
        match self.store {
            Store::Mem(m) => (0..nframes).into_par_iter().try_for_each_init(
                || (init(), Vec::new(), Vec::new()),
                |(state, refs, coords), fi| -> Result<()> {
                    let frame = spill.read_frame(fi)?;
                    refs.clear();
                    refs_of(&frame, refs);
                    coords.clear();
                    coords.extend(
                        refs.iter()
                            .map(|&id| self.set.slot(id).map(|s| m.get(s)).unwrap_or(0)),
                    );
                    f(state, fi, &frame, coords)?;
                    progress.add(1);
                    Ok(())
                },
            ),
            Store::File { .. } => {
                let count = self.store.count();
                let chunk_slots = (self.budget_bytes / 8).max(1 << 20);
                let k = count.div_ceil(chunk_slots).max(1);
                info!("Chunked coordinate join: {k} chunk(s) of {} MB", chunk_slots * 8 >> 20);
                // fills[c][fi] = (offset, len in u64s)
                let mut fills: Vec<(File, PathBuf, Vec<(AtomicU64, AtomicU64)>)> = Vec::new();
                for c in 0..k - 1 {
                    let start = c * chunk_slots;
                    let end = (start + chunk_slots).min(count);
                    let chunk = self.store.load_chunk(start, end)?;
                    let path = self.scratch.join(format!("fill_{c}.bin"));
                    let file = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&path)?;
                    let index: Vec<(AtomicU64, AtomicU64)> = (0..nframes)
                        .map(|_| (AtomicU64::new(0), AtomicU64::new(0)))
                        .collect();
                    let tail = Mutex::new(0u64);
                    (0..nframes).into_par_iter().try_for_each_init(
                        || (Vec::new(), Vec::new()),
                        |(refs, out): &mut (Vec<i64>, Vec<u64>), fi| -> Result<()> {
                            let frame = spill.read_frame(fi)?;
                            refs.clear();
                            refs_of(&frame, refs);
                            out.clear();
                            for &id in refs.iter() {
                                if let Some(s) = self.set.slot(id) {
                                    if s >= start && s < end {
                                        out.push(chunk[(s - start) as usize]);
                                    }
                                }
                            }
                            if out.is_empty() {
                                return Ok(());
                            }
                            let bytes: &[u8] = bytemuck::cast_slice(out);
                            let off = {
                                let mut t = tail.lock().unwrap();
                                let o = *t;
                                *t += bytes.len() as u64;
                                o
                            };
                            file.write_all_at(bytes, off)?;
                            index[fi].0.store(off, Ordering::Relaxed);
                            index[fi].1.store(out.len() as u64, Ordering::Relaxed);
                            Ok(())
                        },
                    )?;
                    drop(chunk);
                    fills.push((file, path, index));
                }
                let start = (k - 1) * chunk_slots;
                let chunk = self.store.load_chunk(start, count)?;
                (0..nframes).into_par_iter().try_for_each_init(
                    || (init(), Vec::new(), Vec::new(), Vec::new()),
                    |(state, refs, coords, fillbufs): &mut (S, Vec<i64>, Vec<u64>, Vec<Vec<u64>>),
                     fi|
                     -> Result<()> {
                        let frame = spill.read_frame(fi)?;
                        refs.clear();
                        refs_of(&frame, refs);
                        fillbufs.resize(fills.len(), Vec::new());
                        for (c, (file, _, index)) in fills.iter().enumerate() {
                            let off = index[fi].0.load(Ordering::Relaxed);
                            let len = index[fi].1.load(Ordering::Relaxed) as usize;
                            let buf = &mut fillbufs[c];
                            buf.clear();
                            buf.resize(len, 0);
                            file.read_exact_at(bytemuck::cast_slice_mut(buf), off)?;
                        }
                        let mut cursors = vec![0usize; fills.len()];
                        coords.clear();
                        for &id in refs.iter() {
                            let v = match self.set.slot(id) {
                                None => 0,
                                Some(s) if s >= start => chunk[(s - start) as usize],
                                Some(s) => {
                                    let c = (s / chunk_slots) as usize;
                                    let v = fillbufs[c][cursors[c]];
                                    cursors[c] += 1;
                                    v
                                }
                            };
                            coords.push(v);
                        }
                        f(state, fi, &frame, coords)?;
                        progress.add(1);
                        Ok(())
                    },
                )?;
                for (_, path, _) in fills {
                    let _ = std::fs::remove_file(path);
                }
                Ok(())
            }
        }
    }
}
