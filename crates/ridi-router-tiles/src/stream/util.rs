//! Small helpers: phase logging, varint codecs, directory sizes.

use std::path::Path;
use std::time::Instant;

use tracing::info;

/// (VmRSS, VmHWM) in MB for the current process.
pub fn mem_status_mb() -> (u64, u64) {
    let mut rss = 0;
    let mut hwm = 0;
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            let parse = |l: &str| -> u64 {
                l.split_whitespace()
                    .nth(1)
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0)
                    / 1024
            };
            if line.starts_with("VmRSS:") {
                rss = parse(line);
            } else if line.starts_with("VmHWM:") {
                hwm = parse(line);
            }
        }
    }
    (rss, hwm)
}

pub fn dir_size_bytes(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() {
                    total += dir_size_bytes(&entry.path());
                } else {
                    // allocated size (sparse files count only written blocks)
                    use std::os::unix::fs::MetadataExt;
                    total += meta.blocks() * 512;
                }
            }
        }
    }
    total
}

/// Process CPU time (user + system) in seconds.
pub fn cpu_seconds() -> f64 {
    std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|s| {
            let rest = &s[s.rfind(')')? + 2..];
            let f: Vec<&str> = rest.split_whitespace().collect();
            let ticks = f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?;
            Some(ticks as f64 / unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64)
        })
        .unwrap_or(0.0)
}

/// Return freed heap memory to the OS. glibc keeps freed memory in its arenas (its mmap threshold
/// adapts upwards after large frees), so without this the buffers of one phase stay resident
/// while the next phase allocates its own (measured on the planet: +5 GB after the route phase).
pub fn release_free_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::malloc_trim(0);
    }
}

pub struct PhaseClock {
    cpu_start: f64,
    start: Instant,
    total_start: Instant,
    scratch: std::path::PathBuf,
    /// Max VmHWM seen across phases (VmHWM may be reset per phase).
    peak_hwm: u64,
}

impl PhaseClock {
    pub fn new(scratch: &Path) -> Self {
        let now = Instant::now();
        Self {
            cpu_start: cpu_seconds(),
            start: now,
            total_start: now,
            scratch: scratch.to_path_buf(),
            peak_hwm: 0,
        }
    }

    pub fn phase(&mut self, name: &str) {
        release_free_memory();
        let (rss, hwm) = mem_status_mb();
        let scratch_mb = dir_size_bytes(&self.scratch) / (1024 * 1024);
        self.peak_hwm = self.peak_hwm.max(hwm);
        let cpu = cpu_seconds();
        info!(
            "PHASE name={} wall_s={:.3} rss_mb={} hwm_mb={} scratch_mb={} cpu_s={:.2}",
            name,
            self.start.elapsed().as_secs_f64(),
            rss,
            hwm,
            scratch_mb,
            cpu - self.cpu_start
        );
        self.start = Instant::now();
        self.cpu_start = cpu;
    }

    /// Reset the kernel's VmHWM so the next phase line shows that phase's own peak
    /// (the process-wide peak is kept and reported in the total line).
    pub fn reset_hwm(&mut self) {
        let (_, hwm) = mem_status_mb();
        self.peak_hwm = self.peak_hwm.max(hwm);
        let _ = std::fs::write("/proc/self/clear_refs", "5");
    }

    pub fn total(&self) {
        let (rss, hwm) = mem_status_mb();
        let hwm = hwm.max(self.peak_hwm);
        info!(
            "PHASE name=total wall_s={:.3} rss_mb={} hwm_mb={}",
            self.total_start.elapsed().as_secs_f64(),
            rss,
            hwm
        );
    }
}

#[inline]
pub fn put_uvarint(buf: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        buf.push((v as u8) | 0x80);
        v >>= 7;
    }
    buf.push(v as u8);
}

#[inline]
pub fn put_svarint(buf: &mut Vec<u8>, v: i64) {
    put_uvarint(buf, ((v << 1) ^ (v >> 63)) as u64);
}

#[inline]
pub fn get_uvarint(buf: &[u8], pos: &mut usize) -> u64 {
    let mut result = 0u64;
    let mut shift = 0;
    loop {
        let b = buf[*pos];
        *pos += 1;
        result |= ((b & 0x7f) as u64) << shift;
        if b < 0x80 {
            return result;
        }
        shift += 7;
    }
}

#[inline]
pub fn get_svarint(buf: &[u8], pos: &mut usize) -> i64 {
    let v = get_uvarint(buf, pos);
    ((v >> 1) as i64) ^ -((v & 1) as i64)
}

#[inline]
pub fn put_str(buf: &mut Vec<u8>, s: &[u8]) {
    put_uvarint(buf, s.len() as u64);
    buf.extend_from_slice(s);
}

#[inline]
pub fn get_str<'a>(buf: &'a [u8], pos: &mut usize) -> &'a [u8] {
    let len = get_uvarint(buf, pos) as usize;
    let s = &buf[*pos..*pos + len];
    *pos += len;
    s
}

#[inline]
pub fn put_f32(buf: &mut Vec<u8>, v: f32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

#[inline]
pub fn get_f32(buf: &[u8], pos: &mut usize) -> f32 {
    let v = f32::from_le_bytes(buf[*pos..*pos + 4].try_into().unwrap());
    *pos += 4;
    v
}

#[inline]
pub fn put_f64(buf: &mut Vec<u8>, v: f64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

#[inline]
pub fn get_f64(buf: &[u8], pos: &mut usize) -> f64 {
    let v = f64::from_le_bytes(buf[*pos..*pos + 8].try_into().unwrap());
    *pos += 8;
    v
}

/// Coordinates are kept as OSM decimicro-degrees; this reproduces
/// `osmpbfreader::Node::lat()` bit-for-bit.
#[inline]
pub fn decimicro_to_deg(v: i32) -> f64 {
    v as f64 * 1e-7
}

/// Raise the soft open-file limit to the hard limit (bucket files are kept open).
pub fn raise_nofile_limit() {
    unsafe {
        let mut lim = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) == 0 && lim.rlim_cur < lim.rlim_max {
            lim.rlim_cur = lim.rlim_max;
            let _ = libc::setrlimit(libc::RLIMIT_NOFILE, &lim);
        }
    }
}

/// Effective memory limit: the smallest cgroup v2 `memory.max` on our cgroup path, else MemTotal.
pub fn memory_limit_bytes() -> u64 {
    let mut limit = u64::MAX;
    if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                if let Some(kb) = rest
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                {
                    limit = kb * 1024;
                }
            }
        }
    }
    if let Ok(cg) = std::fs::read_to_string("/proc/self/cgroup") {
        if let Some(path) = cg.lines().find_map(|l| l.strip_prefix("0::")) {
            let mut p =
                std::path::PathBuf::from("/sys/fs/cgroup").join(path.trim_start_matches('/'));
            loop {
                if let Ok(v) = std::fs::read_to_string(p.join("memory.max")) {
                    if let Ok(v) = v.trim().parse::<u64>() {
                        limit = limit.min(v);
                    }
                }
                if !p.pop() || p == std::path::Path::new("/sys/fs") {
                    break;
                }
            }
        }
    }
    if limit == u64::MAX {
        8 << 30
    } else {
        limit
    }
}

/// Throttled progress logging (at most one line per 30 s) for long-running loops.
pub struct Progress {
    label: &'static str,
    total: u64,
    unit: &'static str,
    unit_div: u64,
    done: std::sync::atomic::AtomicU64,
    start: Instant,
    last_log_ms: std::sync::atomic::AtomicU64,
}

impl Progress {
    pub fn new(label: &'static str, total: u64, unit: &'static str, unit_div: u64) -> Self {
        Self {
            label,
            total,
            unit,
            unit_div: unit_div.max(1),
            done: std::sync::atomic::AtomicU64::new(0),
            start: Instant::now(),
            last_log_ms: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn add(&self, n: u64) {
        use std::sync::atomic::Ordering;
        let done = self.done.fetch_add(n, Ordering::Relaxed) + n;
        let now_ms = self.start.elapsed().as_millis() as u64;
        let last = self.last_log_ms.load(Ordering::Relaxed);
        if now_ms >= last + 30_000
            && self
                .last_log_ms
                .compare_exchange(last, now_ms, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            let secs = now_ms as f64 / 1000.0;
            let pct = if self.total > 0 {
                done as f64 * 100.0 / self.total as f64
            } else {
                0.0
            };
            let eta = if done > 0 && self.total > done {
                secs * (self.total - done) as f64 / done as f64
            } else {
                0.0
            };
            info!(
                "Progress {}: {:.1}% ({}/{} {}) after {:.0}s, ETA {:.0}s",
                self.label,
                pct,
                done / self.unit_div,
                self.total / self.unit_div,
                self.unit,
                secs,
                eta
            );
        }
    }
}

/// Free bytes on the filesystem holding `path`.
pub fn free_space_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    Some(st.f_bavail as u64 * st.f_frsize as u64)
}

/// Write back `path` and drop it from the page cache (best effort).
pub fn flush_and_drop_cache(path: &Path) {
    use std::os::unix::io::AsRawFd;
    if let Ok(f) = std::fs::File::open(path) {
        unsafe {
            libc::sync_file_range(
                f.as_raw_fd(),
                0,
                0,
                libc::SYNC_FILE_RANGE_WAIT_BEFORE
                    | libc::SYNC_FILE_RANGE_WRITE
                    | libc::SYNC_FILE_RANGE_WAIT_AFTER,
            );
            libc::posix_fadvise(f.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED);
        }
    }
}

/// Fast multiplicative hasher for u64 keys (OSM ids); not DoS resistant, which is fine here.
#[derive(Default, Clone, Copy)]
pub struct IdHasher(u64);

impl std::hash::Hasher for IdHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ b as u64).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
        }
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.0 = (self.0.rotate_left(5) ^ i).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

/// Concurrent set of ids below 2^34, as a bitmap allocated in 2^16-id blocks on first use.
pub struct IdBitmap {
    blocks: Vec<std::sync::OnceLock<Box<[std::sync::atomic::AtomicU64]>>>,
}

impl IdBitmap {
    const BLOCK_BITS: u32 = 16;
    const MAX_ID: u64 = 1 << 34;

    pub fn new() -> Self {
        Self {
            blocks: (0..Self::MAX_ID >> Self::BLOCK_BITS)
                .map(|_| std::sync::OnceLock::new())
                .collect(),
        }
    }

    /// Insert `id`; true if it was not present yet.
    pub fn insert(&self, id: u64) -> anyhow::Result<bool> {
        use std::sync::atomic::{AtomicU64, Ordering};
        if id >= Self::MAX_ID {
            anyhow::bail!("id {id} out of range for IdBitmap");
        }
        let block = self.blocks[(id >> Self::BLOCK_BITS) as usize].get_or_init(|| {
            (0..1 << (Self::BLOCK_BITS - 6))
                .map(|_| AtomicU64::new(0))
                .collect()
        });
        let bit = id & ((1 << Self::BLOCK_BITS) - 1);
        let mask = 1u64 << (bit & 63);
        Ok(block[(bit >> 6) as usize].fetch_or(mask, Ordering::Relaxed) & mask == 0)
    }
}

pub type IdBuild = std::hash::BuildHasherDefault<IdHasher>;
pub type IdSet = std::collections::HashSet<u64, IdBuild>;
pub type IdMap<V> = std::collections::HashMap<u64, V, IdBuild>;
