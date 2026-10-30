//! Random-access PBF blob reader with a zero-copy protobuf decoder.
//!
//! The file is indexed once (blob headers only, `posix_fadvise(RANDOM)` so the kernel does not
//! read ahead into blob bodies). Because PBFs are sorted Type_then_ID, the node / way / relation
//! blob ranges are found by bisection, and each pass then decodes only its own blobs in parallel
//! (rayon over blob indices, `pread` per blob). Every blob is therefore decompressed ~once in
//! total across the three passes.
//!
//! Decoding is zero-copy: after inflating, the block is walked with a minimal protobuf reader
//! that only borrows slices (string table entries, packed arrays) — nothing is allocated per
//! entity, and fields the generator does not need (info, denseinfo, ...) are skipped.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::os::unix::fs::FileExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use tracing::info;

// ---------------------------------------------------------------------------------------------
// Minimal protobuf reader
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct Pb<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Pb<'a> {
    #[inline]
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    #[inline]
    fn varint(&mut self) -> Result<u64> {
        let mut result = 0u64;
        let mut shift = 0;
        loop {
            let Some(&b) = self.buf.get(self.pos) else {
                bail!("truncated varint");
            };
            self.pos += 1;
            result |= ((b & 0x7f) as u64) << shift;
            if b < 0x80 {
                return Ok(result);
            }
            shift += 7;
            if shift > 63 {
                bail!("varint too long");
            }
        }
    }

    /// Next (field number, wire type, value). Length-delimited values come back as `Bytes`.
    #[inline]
    pub fn next(&mut self) -> Result<Option<(u32, Val<'a>)>> {
        if self.pos >= self.buf.len() {
            return Ok(None);
        }
        let key = self.varint()?;
        let field = (key >> 3) as u32;
        let val = match key & 7 {
            0 => Val::Int(self.varint()?),
            1 => {
                self.pos += 8;
                Val::Skip
            }
            2 => {
                let len = self.varint()? as usize;
                let end = self.pos + len;
                if end > self.buf.len() {
                    bail!("truncated length-delimited field");
                }
                let s = &self.buf[self.pos..end];
                self.pos = end;
                Val::Bytes(s)
            }
            5 => {
                self.pos += 4;
                Val::Skip
            }
            wt => bail!("unsupported wire type {wt}"),
        };
        Ok(Some((field, val)))
    }
}

#[derive(Clone, Copy)]
pub enum Val<'a> {
    Int(u64),
    Bytes(&'a [u8]),
    Skip,
}

impl<'a> Val<'a> {
    #[inline]
    fn int(self) -> u64 {
        match self {
            Val::Int(v) => v,
            _ => 0,
        }
    }
    #[inline]
    fn bytes(self) -> &'a [u8] {
        match self {
            Val::Bytes(b) => b,
            _ => &[],
        }
    }
}

#[inline]
pub fn zigzag(v: u64) -> i64 {
    ((v >> 1) as i64) ^ -((v & 1) as i64)
}

/// Iterator over a packed varint array.
#[derive(Clone, Copy)]
pub struct Packed<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Packed<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
}

impl Iterator for Packed<'_> {
    type Item = u64;
    #[inline]
    fn next(&mut self) -> Option<u64> {
        let b0 = *self.buf.get(self.pos)?;
        if b0 < 0x80 {
            self.pos += 1;
            return Some(b0 as u64);
        }
        self.pos += 1;
        let mut result = (b0 & 0x7f) as u64;
        let mut shift = 7;
        while self.pos < self.buf.len() {
            let b = self.buf[self.pos];
            self.pos += 1;
            result |= ((b & 0x7f) as u64) << shift;
            if b < 0x80 {
                break;
            }
            shift += 7;
        }
        Some(result)
    }
}

/// Packed sint64 array.
#[inline]
pub fn packed_sint(buf: &[u8]) -> impl Iterator<Item = i64> + '_ {
    Packed::new(buf).map(zigzag)
}

// ---------------------------------------------------------------------------------------------
// OSM block structures (borrowed)
// ---------------------------------------------------------------------------------------------

pub struct Block<'a> {
    pub strings: Vec<&'a [u8]>,
    pub groups: Vec<&'a [u8]>,
    pub granularity: i64,
    pub lat_offset: i64,
    pub lon_offset: i64,
}

impl<'a> Block<'a> {
    pub fn parse(buf: &'a [u8]) -> Result<Self> {
        let mut b = Block {
            strings: Vec::new(),
            groups: Vec::new(),
            granularity: 100,
            lat_offset: 0,
            lon_offset: 0,
        };
        let mut pb = Pb::new(buf);
        while let Some((f, v)) = pb.next()? {
            match f {
                1 => {
                    let mut st = Pb::new(v.bytes());
                    while let Some((sf, sv)) = st.next()? {
                        if sf == 1 {
                            b.strings.push(sv.bytes());
                        }
                    }
                }
                2 => b.groups.push(v.bytes()),
                17 => b.granularity = v.int() as i64,
                19 => b.lat_offset = v.int() as i64,
                20 => b.lon_offset = v.int() as i64,
                _ => {}
            }
        }
        Ok(b)
    }

    #[inline]
    pub fn coord(&self, c: i64, lat: bool) -> i32 {
        let off = if lat {
            self.lat_offset
        } else {
            self.lon_offset
        };
        ((off + self.granularity * c) / 100) as i32
    }

    /// Visit each group's members: `f(field, bytes)` with field 1=Node, 2=DenseNodes, 3=Way,
    /// 4=Relation.
    pub fn for_each_member(&self, mut f: impl FnMut(u32, &'a [u8]) -> Result<()>) -> Result<()> {
        for g in &self.groups {
            let mut pb = Pb::new(g);
            while let Some((field, v)) = pb.next()? {
                if (1..=4).contains(&field) {
                    f(field, v.bytes())?;
                }
            }
        }
        Ok(())
    }

    /// (min kind, max kind) of the entities in this block. Empty blocks count as relations.
    pub fn kinds(&self) -> Result<(u8, u8)> {
        let mut min = u8::MAX;
        let mut max = 0u8;
        self.for_each_member(|field, bytes| {
            let k = match field {
                1 => KIND_NODE,
                2 => {
                    if DenseNodes::parse(bytes)?.ids.is_empty() {
                        return Ok(());
                    }
                    KIND_NODE
                }
                3 => KIND_WAY,
                _ => KIND_RELATION,
            };
            min = min.min(k);
            max = max.max(k);
            Ok(())
        })?;
        Ok(if min == u8::MAX {
            (KIND_RELATION, KIND_RELATION)
        } else {
            (min, max)
        })
    }
}

pub struct DenseNodes<'a> {
    pub ids: &'a [u8],
    pub lats: &'a [u8],
    pub lons: &'a [u8],
}

impl<'a> DenseNodes<'a> {
    pub fn parse(buf: &'a [u8]) -> Result<Self> {
        let mut d = DenseNodes {
            ids: &[],
            lats: &[],
            lons: &[],
        };
        let mut pb = Pb::new(buf);
        while let Some((f, v)) = pb.next()? {
            match (f, v) {
                (1, Val::Bytes(b)) => d.ids = b,
                (8, Val::Bytes(b)) => d.lats = b,
                (9, Val::Bytes(b)) => d.lons = b,
                (1 | 8 | 9, _) => bail!("unpacked DenseNodes arrays are not supported"),
                _ => {}
            }
        }
        Ok(d)
    }
}

/// Plain (non-dense) node: (id, raw lat, raw lon).
pub fn parse_node(buf: &[u8]) -> Result<(i64, i64, i64)> {
    let (mut id, mut lat, mut lon) = (0, 0, 0);
    let mut pb = Pb::new(buf);
    while let Some((f, v)) = pb.next()? {
        match f {
            1 => id = zigzag(v.int()),
            8 => lat = zigzag(v.int()),
            9 => lon = zigzag(v.int()),
            _ => {}
        }
    }
    Ok((id, lat, lon))
}

pub struct WayMsg<'a> {
    pub id: i64,
    pub keys: &'a [u8],
    pub vals: &'a [u8],
    pub refs: &'a [u8],
}

impl<'a> WayMsg<'a> {
    pub fn parse(buf: &'a [u8]) -> Result<Self> {
        let mut w = WayMsg {
            id: 0,
            keys: &[],
            vals: &[],
            refs: &[],
        };
        let mut pb = Pb::new(buf);
        while let Some((f, v)) = pb.next()? {
            match (f, v) {
                (1, Val::Int(i)) => w.id = i as i64,
                (2, Val::Bytes(b)) => w.keys = b,
                (3, Val::Bytes(b)) => w.vals = b,
                (8, Val::Bytes(b)) => w.refs = b,
                (2 | 3 | 8, Val::Int(_)) => bail!("unpacked Way arrays are not supported"),
                _ => {}
            }
        }
        Ok(w)
    }
}

pub struct RelationMsg<'a> {
    pub id: i64,
    pub keys: &'a [u8],
    pub vals: &'a [u8],
    pub roles: &'a [u8],
    pub memids: &'a [u8],
    pub types: &'a [u8],
}

impl<'a> RelationMsg<'a> {
    pub fn parse(buf: &'a [u8]) -> Result<Self> {
        let mut r = RelationMsg {
            id: 0,
            keys: &[],
            vals: &[],
            roles: &[],
            memids: &[],
            types: &[],
        };
        let mut pb = Pb::new(buf);
        while let Some((f, v)) = pb.next()? {
            match (f, v) {
                (1, Val::Int(i)) => r.id = i as i64,
                (2, Val::Bytes(b)) => r.keys = b,
                (3, Val::Bytes(b)) => r.vals = b,
                (8, Val::Bytes(b)) => r.roles = b,
                (9, Val::Bytes(b)) => r.memids = b,
                (10, Val::Bytes(b)) => r.types = b,
                (2 | 3 | 8 | 9 | 10, Val::Int(_)) => {
                    bail!("unpacked Relation arrays are not supported")
                }
                _ => {}
            }
        }
        Ok(r)
    }
}

// ---------------------------------------------------------------------------------------------
// File / blob access
// ---------------------------------------------------------------------------------------------

/// A sorted PBF opened for byte-range parallel decoding.
///
/// There is no up-front blob index (a serial chain of header reads costs ~1 random IO per blob,
/// i.e. minutes on a planet file). Instead any byte offset can be synchronized to the next blob
/// boundary by searching for the `OSMData` BlobHeader signature and validating the chain of two
/// headers. Type boundaries are found by bisection over byte offsets, and each pass splits its
/// byte range into segments that rayon workers read sequentially.
pub struct PbfFile {
    file: File,
    len: u64,
    kinds: Mutex<HashMap<u64, (u8, u8)>>,
    pub data_start: u64,
    /// `osmosis_replication_timestamp` from the header, if present
    pub replication_timestamp: Option<i64>,
}

pub const KIND_NODE: u8 = 0;
pub const KIND_WAY: u8 = 1;
pub const KIND_RELATION: u8 = 2;

fn parse_blob_header(buf: &[u8]) -> Result<(String, u64)> {
    let mut t = String::new();
    let mut size = 0;
    let mut pb = Pb::new(buf);
    while let Some((f, v)) = pb.next()? {
        match f {
            1 => t = String::from_utf8_lossy(v.bytes()).into_owned(),
            3 => size = v.int(),
            _ => {}
        }
    }
    Ok((t, size))
}

const SIG: &[u8] = b"\x0a\x07OSMData";

impl PbfFile {
    #[hotpath::measure]
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("Failed to open PBF {path:?}"))?;
        let len = file.metadata()?.len();
        unsafe {
            libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_SEQUENTIAL);
        }
        let mut pbf = Self {
            file,
            len,
            kinds: Mutex::new(HashMap::new()),
            data_start: 0,
            replication_timestamp: None,
        };
        // header blob(s) at the start
        let mut off = 0u64;
        let mut sorted_flag = None;
        while let Some((btype, data_off, dsize)) = pbf.header_at(off)? {
            if btype == "OSMHeader" {
                let raw = read_blob_raw(&pbf.file, data_off, dsize as u32)?;
                let mut sorted = false;
                let mut pb = Pb::new(&raw);
                while let Some((f, v)) = pb.next()? {
                    if f == 5 && v.bytes() == b"Sort.Type_then_ID" {
                        sorted = true;
                    }
                    if f == 32 {
                        pbf.replication_timestamp = Some(v.int() as i64);
                    }
                }
                sorted_flag = Some(sorted);
                off = data_off + dsize;
            } else {
                break;
            }
        }
        pbf.data_start = off;
        info!(
            "Opened PBF ({} MB), header Sort.Type_then_ID={:?}",
            len >> 20,
            sorted_flag
        );
        Ok(pbf)
    }

    /// Parse the blob header starting at `off`: (type, data offset, data size).
    fn header_at(&self, off: u64) -> Result<Option<(String, u64, u64)>> {
        if off + 4 > self.len {
            return Ok(None);
        }
        let mut lenbuf = [0u8; 4];
        self.file.read_exact_at(&mut lenbuf, off)?;
        let hlen = u32::from_be_bytes(lenbuf) as u64;
        if hlen > 64 * 1024 || off + 4 + hlen > self.len {
            bail!("Invalid blob header length {hlen} at offset {off}");
        }
        let mut hbuf = vec![0u8; hlen as usize];
        self.file.read_exact_at(&mut hbuf, off + 4)?;
        let (t, dsize) = parse_blob_header(&hbuf)?;
        Ok(Some((t, off + 4 + hlen, dsize)))
    }

    fn valid_header_at(&self, off: u64) -> bool {
        match self.header_at(off) {
            Ok(Some((t, data_off, dsize))) => {
                (t == "OSMData" || t == "OSMHeader") && dsize > 0 && data_off + dsize <= self.len
            }
            _ => false,
        }
    }

    /// First blob (header offset) starting at or after `off`, or None at EOF.
    pub fn sync(&self, off: u64) -> Result<Option<u64>> {
        let off = off.max(self.data_start);
        const WIN: u64 = 256 * 1024;
        let mut pos = off;
        let mut buf = vec![0u8; WIN as usize];
        while pos < self.len {
            let n = WIN.min(self.len - pos) as usize;
            self.file.read_exact_at(&mut buf[..n], pos)?;
            let mut i = 0;
            while let Some(j) = find(&buf[i..n], SIG) {
                let sig_at = pos + (i + j) as u64;
                i += j + 1;
                if sig_at < off + 4 {
                    continue;
                }
                let h = sig_at - 4;
                if !self.valid_header_at(h) {
                    continue;
                }
                let (_, data_off, dsize) = self.header_at(h)?.unwrap();
                let next = data_off + dsize;
                if next == self.len || self.valid_header_at(next) {
                    return Ok(Some(h));
                }
            }
            if n < WIN as usize {
                break;
            }
            pos += WIN - SIG.len() as u64 - 4;
        }
        Ok(None)
    }

    /// Read + inflate the blob whose header starts at `h`; returns (raw block, next offset).
    pub fn read_at(&self, h: u64) -> Result<(Vec<u8>, u64)> {
        let (t, data_off, dsize) = self
            .header_at(h)?
            .ok_or_else(|| anyhow::anyhow!("no blob at {h}"))?;
        if t != "OSMData" {
            bail!("unexpected blob type {t} at {h}");
        }
        Ok((
            read_blob_raw(&self.file, data_off, dsize as u32)?,
            data_off + dsize,
        ))
    }

    fn kinds(&self, h: u64) -> Result<(u8, u8)> {
        if let Some(k) = self.kinds.lock().unwrap().get(&h) {
            return Ok(*k);
        }
        let (raw, _) = self.read_at(h)?;
        let k = Block::parse(&raw)?.kinds()?;
        self.kinds.lock().unwrap().insert(h, k);
        Ok(k)
    }

    /// Header offset of the first blob for which `pred` holds (monotonic false..true), or EOF.
    fn bisect(&self, pred: impl Fn((u8, u8)) -> bool) -> Result<u64> {
        let (mut lo, mut hi) = (self.data_start, self.len);
        let f = |x: u64| -> Result<bool> {
            Ok(match self.sync(x)? {
                None => true,
                Some(h) => pred(self.kinds(h)?),
            })
        };
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if f(mid)? {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        Ok(self.sync(lo)?.unwrap_or(self.len))
    }

    /// Byte ranges (of blob header offsets) holding nodes, ways and relations.
    pub fn type_ranges(&self) -> Result<[std::ops::Range<u64>; 3]> {
        let node_end = self.bisect(|(min, _)| min > KIND_NODE)?;
        let way_start = self.bisect(|(_, max)| max >= KIND_WAY)?;
        let way_end = self.bisect(|(min, _)| min > KIND_WAY)?;
        let rel_start = self.bisect(|(_, max)| max >= KIND_RELATION)?;
        let n = self.len;
        info!(
            "Byte ranges: nodes {}..{node_end}, ways {way_start}..{way_end}, relations {rel_start}..{n} ({} bisection blobs decoded)",
            self.data_start,
            self.kinds.lock().unwrap().len()
        );
        Ok([self.data_start..node_end, way_start..way_end, rel_start..n])
    }

    /// Parallel iterator over the inflated blocks whose header offset lies in `range`:
    /// items are (header offset, raw block bytes). Segments are read sequentially by one worker.
    pub fn par_blocks(
        &self,
        range: std::ops::Range<u64>,
        progress: std::sync::Arc<super::util::Progress>,
    ) -> impl rayon::iter::ParallelIterator<Item = Result<(u64, Vec<u8>)>> + '_ {
        use rayon::prelude::*;
        let threads = rayon::current_num_threads() as u64;
        let span = range.end.saturating_sub(range.start);
        let seg = (span / (threads * 8)).clamp(4 << 20, 256 << 20);
        let nseg = span.div_ceil(seg);
        (0..nseg).into_par_iter().flat_map_iter(move |si| {
            let progress = progress.clone();
            let seg_start = range.start + si * seg;
            let seg_end = (seg_start + seg).min(range.end);
            let first = if si == 0 {
                Ok(Some(range.start))
            } else {
                self.sync(seg_start)
            };
            let mut cur = match first {
                Ok(c) => c.filter(|&h| h < seg_end).map(Ok),
                Err(e) => Some(Err(e)),
            };
            std::iter::from_fn(move || {
                let h = match cur.take()? {
                    Ok(h) => h,
                    Err(e) => return Some(Err(e)),
                };
                match self.read_at(h) {
                    Ok((raw, next)) => {
                        progress.add(next - h);
                        // each blob is read exactly once: drop it from the page cache so the
                        // input file does not compete with our own memory (cgroup limits)
                        unsafe {
                            libc::posix_fadvise(
                                self.file.as_raw_fd(),
                                h as i64,
                                (next - h) as i64,
                                libc::POSIX_FADV_DONTNEED,
                            );
                        }
                        if next < seg_end && next < self.len {
                            cur = Some(Ok(next));
                        }
                        Some(Ok((h, raw)))
                    }
                    Err(e) => Some(Err(e)),
                }
            })
        })
    }
}

/// One or more PBF inputs read as one dataset. Every pass reads the matching type range of all
/// files in parallel; objects present in several (overlapping) files are deduplicated by the
/// passes.
pub struct PbfSet {
    files: Vec<(PbfFile, [std::ops::Range<u64>; 3])>,
}

impl PbfSet {
    pub fn open(paths: &[std::path::PathBuf]) -> Result<Self> {
        let files = paths
            .iter()
            .map(|path| -> Result<_> {
                let pbf = PbfFile::open(path)?;
                let ranges = pbf
                    .type_ranges()
                    .with_context(|| format!("Failed to index PBF {path:?}"))?;
                Ok((pbf, ranges))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut stamps: Vec<i64> = files
            .iter()
            .filter_map(|(f, _)| f.replication_timestamp)
            .collect();
        stamps.sort_unstable();
        stamps.dedup();
        if stamps.len() > 1 {
            tracing::warn!(
                "Input PBFs have {} different replication timestamps ({}..{}); objects present in several files are taken from any one of them",
                stamps.len(),
                stamps[0],
                stamps[stamps.len() - 1]
            );
        }
        Ok(Self { files })
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn ranges(&self, file: usize) -> &[std::ops::Range<u64>; 3] {
        &self.files[file].1
    }

    /// Parallel iterator over the blocks of entity `kind` in all files:
    /// items are (file index, header offset, raw block bytes).
    pub fn par_blocks(
        &self,
        kind: u8,
        label: &'static str,
    ) -> impl rayon::iter::ParallelIterator<Item = Result<(usize, u64, Vec<u8>)>> + '_ {
        use rayon::prelude::*;
        let total = self
            .files
            .iter()
            .map(|(_, r)| r[kind as usize].end.saturating_sub(r[kind as usize].start))
            .sum();
        let progress = std::sync::Arc::new(super::util::Progress::new(label, total, "MB", 1 << 20));
        self.files
            .par_iter()
            .enumerate()
            .flat_map(move |(fi, (pbf, ranges))| {
                pbf.par_blocks(ranges[kind as usize].clone(), progress.clone())
                    .map(move |item| item.map(|(bi, raw)| (fi, bi, raw)))
            })
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

pub static INFLATE_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static PREAD_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Log and reset the cumulative pread / inflate CPU time.
pub fn log_io_counters(phase: &str) {
    use std::sync::atomic::Ordering;
    let i = INFLATE_NS.swap(0, Ordering::Relaxed);
    let p = PREAD_NS.swap(0, Ordering::Relaxed);
    info!(
        "{phase}: pread {:.2} s, inflate {:.2} s (summed over threads)",
        p as f64 / 1e9,
        i as f64 / 1e9
    );
}

fn read_blob_raw(file: &File, offset: u64, size: u32) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; size as usize];
    let t = std::time::Instant::now();
    file.read_exact_at(&mut buf, offset)?;
    PREAD_NS.fetch_add(
        t.elapsed().as_nanos() as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
    let mut raw: Option<&[u8]> = None;
    let mut zlib: Option<&[u8]> = None;
    let mut raw_size = 0usize;
    let mut pb = Pb::new(&buf);
    while let Some((f, v)) = pb.next()? {
        match f {
            1 => raw = Some(v.bytes()),
            2 => raw_size = v.int() as usize,
            3 => zlib = Some(v.bytes()),
            4 | 5 | 6 | 7 => {
                bail!("Unsupported blob compression (field {f}); only raw and zlib are supported")
            }
            _ => {}
        }
    }
    if let Some(r) = raw {
        Ok(r.to_vec())
    } else if let Some(z) = zlib {
        let t = std::time::Instant::now();
        let mut out = Vec::new();
        if raw_size > 0 {
            out.resize(raw_size, 0);
            thread_local! {
                static DEFLATE: std::cell::RefCell<libdeflater::Decompressor> =
                    std::cell::RefCell::new(libdeflater::Decompressor::new());
            }
            let n = DEFLATE
                .with(|d| d.borrow_mut().zlib_decompress(z, &mut out))
                .map_err(|e| anyhow::anyhow!("zlib decompression failed: {e:?}"))?;
            out.truncate(n);
        } else {
            flate2::read::ZlibDecoder::new(z).read_to_end(&mut out)?;
        }
        INFLATE_NS.fetch_add(
            t.elapsed().as_nanos() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        Ok(out)
    } else {
        bail!("Blob has no data")
    }
}

/// Map each string-table entry to a small key code using `classify`.
pub fn classify_strings(block: &Block, classify: impl Fn(&[u8]) -> u8) -> Vec<u8> {
    block.strings.iter().map(|s| classify(s)).collect()
}

pub fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
