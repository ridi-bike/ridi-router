//! Needed-node set (roaring-style: id >> 16 selects a container that is either a sorted u16
//! array or an 8 KiB bitmap), its rank directory, and the coordinate store.
//!
//! Memory: sparse inputs (country extracts: ~1 needed node per few thousand ids) cost ~2 bytes
//! per needed node; dense ranges (planet: ~13% of the 13.5e9 id space) cost 1 bit per id
//! (~1.7 GB). Unlike a flat id-indexed bitset this does not touch every page of the id space
//! for small inputs (round 0 paid ~1.7 GB under THP even for Latvia).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use anyhow::{bail, Result};

/// Supported node id range: [0, 2^35).
const MAX_ID_BITS: u32 = 35;
const NCONT: usize = 1 << (MAX_ID_BITS - 16);
const ARRAY_MAX: usize = 4096;

enum Cont {
    Empty,
    /// Unsorted while building, may contain duplicates.
    Array(Vec<u16>),
    Bits(Box<[u64; 1024]>),
}

pub struct NeededSet {
    conts: Vec<Mutex<Cont>>,
    max_id: AtomicU64,
}

impl NeededSet {
    pub fn new() -> Self {
        Self {
            conts: (0..NCONT).map(|_| Mutex::new(Cont::Empty)).collect(),
            max_id: AtomicU64::new(0),
        }
    }

    /// Insert node ids (any order, duplicates allowed). `scratch` is reused between calls.
    pub fn insert_batch(&self, ids: &mut Vec<i64>) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        ids.sort_unstable();
        ids.dedup();
        if ids[0] < 0 || (*ids.last().unwrap() as u64) >> MAX_ID_BITS != 0 {
            bail!(
                "node id outside supported range [0, 2^{MAX_ID_BITS}): {} / {}",
                ids[0],
                ids.last().unwrap()
            );
        }
        self.max_id
            .fetch_max(*ids.last().unwrap() as u64, Ordering::Relaxed);
        let mut i = 0;
        while i < ids.len() {
            let high = (ids[i] as u64 >> 16) as usize;
            let mut j = i;
            while j < ids.len() && (ids[j] as u64 >> 16) as usize == high {
                j += 1;
            }
            let mut c = self.conts[high].lock().unwrap();
            let lows = ids[i..j].iter().map(|&id| id as u16);
            match &mut *c {
                Cont::Empty => {
                    *c = Cont::Array(lows.collect());
                }
                Cont::Array(v) => {
                    v.extend(lows);
                }
                Cont::Bits(b) => {
                    for l in lows {
                        b[(l >> 6) as usize] |= 1 << (l & 63);
                    }
                }
            }
            if let Cont::Array(v) = &mut *c {
                if v.len() > 2 * ARRAY_MAX {
                    v.sort_unstable();
                    v.dedup();
                    if v.len() > ARRAY_MAX {
                        let mut b = Box::new([0u64; 1024]);
                        for &l in v.iter() {
                            b[(l >> 6) as usize] |= 1 << (l & 63);
                        }
                        *c = Cont::Bits(b);
                    }
                }
            }
            i = j;
        }
        Ok(())
    }

    /// Freeze and build the rank directory.
    pub fn finish(self) -> RankedSet {
        use rayon::prelude::*;
        let max_high = (self.max_id.load(Ordering::Relaxed) >> 16) as usize;
        let mut conts_in = self.conts;
        conts_in.truncate(max_high + 1);
        // finalize containers in parallel (sorting arrays dominated the serial version)
        let fin: Vec<(FCont, u64, usize)> = conts_in
            .into_par_iter()
            .map(|m| {
                let fc = match m.into_inner().unwrap() {
                    Cont::Empty => FCont::Empty,
                    Cont::Array(mut v) => {
                        v.sort_unstable();
                        v.dedup();
                        if v.len() > ARRAY_MAX {
                            let mut b = Box::new([0u64; 1024]);
                            for &l in v.iter() {
                                b[(l >> 6) as usize] |= 1 << (l & 63);
                            }
                            to_fbits(b)
                        } else {
                            FCont::Array(v.into_boxed_slice())
                        }
                    }
                    Cont::Bits(b) => to_fbits(b),
                };
                let (n, sz) = match &fc {
                    FCont::Empty => (0, 0),
                    FCont::Array(a) => (a.len() as u64, a.len() * 2),
                    FCont::Bits(b, _) => {
                        (b.iter().map(|w| w.count_ones() as u64).sum(), 8192 + 256)
                    }
                };
                (fc, n, sz)
            })
            .collect();
        let mut conts = Vec::with_capacity(fin.len());
        let mut base = Vec::with_capacity(fin.len() + 1);
        let mut total = 0u64;
        let mut bytes = 0usize;
        for (fc, n, sz) in fin {
            base.push(total);
            total += n;
            bytes += sz;
            conts.push(fc);
        }
        base.push(total);
        tracing::info!(
            "Needed-node set: {} ids, {} MB in containers",
            total,
            bytes >> 20
        );
        RankedSet {
            conts,
            base,
            count: total,
        }
    }
}

fn to_fbits(b: Box<[u64; 1024]>) -> FCont {
    let mut pre = Box::new([0u16; 128]);
    let mut acc = 0u32;
    for blk in 0..128 {
        pre[blk] = acc as u16;
        for w in &b[blk * 8..blk * 8 + 8] {
            acc += w.count_ones();
        }
    }
    FCont::Bits(b, pre)
}

enum FCont {
    Empty,
    Array(Box<[u16]>),
    /// bitmap + popcount prefix per 8 words
    Bits(Box<[u64; 1024]>, Box<[u16; 128]>),
}

pub struct RankedSet {
    conts: Vec<FCont>,
    base: Vec<u64>,
    pub count: u64,
}

/// Merge-join cursor for lookups with non-decreasing ids (the node pass).
#[derive(Default)]
pub struct SlotCursor {
    high: usize,
    pos: usize,
    last_low: u16,
}

impl RankedSet {
    /// Like `slot`, but amortized O(1) when ids arrive in increasing order.
    #[inline]
    pub fn slot_seq(&self, id: i64, cur: &mut SlotCursor) -> Option<u64> {
        if id < 0 {
            return None;
        }
        let high = (id as u64 >> 16) as usize;
        let low = id as u16;
        match self.conts.get(high)? {
            FCont::Array(a) => {
                if cur.high != high || low < cur.last_low {
                    cur.high = high;
                    cur.pos = 0;
                }
                cur.last_low = low;
                while cur.pos < a.len() && a[cur.pos] < low {
                    cur.pos += 1;
                }
                if cur.pos < a.len() && a[cur.pos] == low {
                    Some(self.base[high] + cur.pos as u64)
                } else {
                    None
                }
            }
            _ => self.slot(id),
        }
    }

    /// Dense slot of `id` if it is a needed node.
    #[inline]
    pub fn slot(&self, id: i64) -> Option<u64> {
        if id < 0 {
            return None;
        }
        let high = (id as u64 >> 16) as usize;
        let low = id as u16;
        match self.conts.get(high)? {
            FCont::Empty => None,
            FCont::Array(a) => a
                .binary_search(&low)
                .ok()
                .map(|i| self.base[high] + i as u64),
            FCont::Bits(b, pre) => {
                let wi = (low >> 6) as usize;
                let bit = 1u64 << (low & 63);
                let w = b[wi];
                if w & bit == 0 {
                    return None;
                }
                let blk = wi / 8;
                let mut r = pre[blk] as u64;
                for word in &b[blk * 8..wi] {
                    r += word.count_ones() as u64;
                }
                r += (w & (bit - 1)).count_ones() as u64;
                Some(self.base[high] + r)
            }
        }
    }
}

/// Encode decimicro (lat, lon) into a non-zero u64 (0 = node missing from the PBF).
#[inline]
pub fn enc_coord(lat: i32, lon: i32) -> u64 {
    let la = (lat as i64 + 900_000_001) as u32 as u64;
    let lo = (lon as i64 + 1_800_000_001) as u32 as u64;
    (la << 32) | lo
}

#[inline]
pub fn dec_coord(v: u64) -> Option<(i32, i32)> {
    if v == 0 {
        return None;
    }
    let lat = ((v >> 32) as i64 - 900_000_001) as i32;
    let lon = ((v & 0xffff_ffff) as i64 - 1_800_000_001) as i32;
    Some((lat, lon))
}

/// In-memory, slot-indexed coordinate array that node-pass workers fill concurrently.
pub struct MemStore {
    cells: Vec<AtomicU64>,
}

impl MemStore {
    pub fn new(count: u64) -> Self {
        // zeroed allocation: pages are committed lazily
        let v: Vec<u64> = vec![0u64; count as usize];
        let mut v = std::mem::ManuallyDrop::new(v);
        let cells =
            unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut AtomicU64, v.len(), v.capacity()) };
        Self { cells }
    }
    #[inline]
    pub fn set_range(&self, first_slot: u64, vals: &[u64]) {
        for (i, v) in vals.iter().enumerate() {
            self.cells[first_slot as usize + i].store(*v, Ordering::Relaxed);
        }
    }
    #[inline]
    pub fn get(&self, slot: u64) -> u64 {
        self.cells[slot as usize].load(Ordering::Relaxed)
    }
    pub fn len(&self) -> u64 {
        self.cells.len() as u64
    }
}
