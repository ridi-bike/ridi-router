//! Prototype B: streaming, external-memory RMDF tile generator (single PBF input).
//!
//! Data flow (memory bounded independent of input size, except the explicitly modelled indexes):
//!  1. index     – no blob index: type boundaries found by bisection over byte offsets
//!                 (blob boundaries re-synchronised from any offset via the BlobHeader signature).
//!  2. relations – keep restriction + residential/military area relations (in memory, small).
//!  3. ways      – routable ways and area(-member) ways are spilled to disk; their node ids are
//!                 inserted into a roaring-style needed-node set.
//!  4. rank      – rank directory over the set; coordinate store (in memory if it fits the
//!                 budget, else a file joined in slot-range chunks with sequential IO only).
//!  5. nodes     – needed node coordinates written to their rank slot; all-node bounds.
//!  6. route     – spilled ways joined with coordinates and appended to per-region (0.5 deg)
//!                 bucket files; restriction relations follow their member ways.
//!  7. areas     – residential/military polygons assembled and appended to every region near
//!                 their bbox (polygons touching > 16 regions go to one shared file instead).
//!  8. tiles     – base regions grouped into jobs by size; each job runs the shared
//!                 `local_stage` (vendored from prototype A) under a memory budget.
//!  9. manifest  – from the in-memory tile results.
//!
//! Grid anchor: global (-180/-90) by default; `RIDI_GRID_ANCHOR=reference` reproduces the old
//! single-PBF grid for parity tests.

mod buckets;
mod distribute;
mod geom;
mod local;
mod nodeidx;
mod passes;
mod pbf;
mod records;
mod store;
mod util;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use tracing::info;

use crate::local_stage::{manifest::write_manifest, GridAnchor};
use buckets::{RegionBuckets, SpillReader, SpillWriter};
use geom::TileGrid;
use nodeidx::NeededSet;
use store::{Resolver, Store};
use util::{decimicro_to_deg, PhaseClock};

struct ScratchDir(PathBuf, bool);

impl Drop for ScratchDir {
    fn drop(&mut self) {
        if !self.1 {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[hotpath::measure]
pub fn generate(input: &Path, output_dir: &Path, tile_size: f32) -> Result<()> {
    util::raise_nofile_limit();
    let keep = std::env::var("RIDI_KEEP_SCRATCH").is_ok_and(|v| v == "1");
    let scratch_path = std::env::temp_dir().join(format!("ridi-stream-{}", std::process::id()));
    std::fs::create_dir_all(&scratch_path)
        .with_context(|| format!("create scratch dir {scratch_path:?}"))?;
    let scratch = ScratchDir(scratch_path.clone(), keep);
    let scratch_dir = &scratch.0;
    info!("Prototype B streaming generator; scratch dir {scratch_dir:?}");
    std::fs::create_dir_all(output_dir)?;

    let region_tiles: u32 = std::env::var("RIDI_REGION_TILES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| ((0.5 / tile_size).round() as u32).max(1));
    let grid = TileGrid::new(tile_size, region_tiles);
    // Default: global anchor (origin -180/-90, identical for every input; what a planet or
    // split run needs). `reference` reproduces the old single-PBF grid for parity tests.
    let global_anchor = match std::env::var("RIDI_GRID_ANCHOR").as_deref() {
        Ok("global") | Err(_) => true,
        Ok("reference") => false,
        Ok(other) => anyhow::bail!("RIDI_GRID_ANCHOR must be 'reference' or 'global', got {other}"),
    };

    let mem_limit = util::memory_limit_bytes();
    let store_budget: u64 = std::env::var("RIDI_STORE_BUDGET_MB")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|v| v << 20)
        .unwrap_or(mem_limit / 100 * 45);
    info!(
        "Memory limit {} MB, coordinate-store budget {} MB",
        mem_limit >> 20,
        store_budget >> 20
    );

    // Fail early on insufficient scratch space (measured: ~0.19x PBF size at the route/areas
    // peak on Europe, + store/fill files if the coordinate store has to be chunked).
    let pbf_size = std::fs::metadata(input)?.len();
    let scratch_need = pbf_size / 100 * 30;
    if let Some(free) = util::free_space_bytes(scratch_dir) {
        info!(
            "Scratch: {} GB free, ~{} GB expected peak; output dir free: {} GB (expect ~2x PBF size)",
            free >> 30,
            scratch_need >> 30,
            util::free_space_bytes(output_dir).unwrap_or(0) >> 30
        );
        if free < scratch_need && std::env::var("RIDI_SKIP_SPACE_CHECK").is_err() {
            anyhow::bail!(
                "Not enough scratch space in {scratch_dir:?}: {} GB free, ~{} GB needed (set TMPDIR to a bigger disk, or RIDI_SKIP_SPACE_CHECK=1)",
                free >> 30,
                scratch_need >> 30
            );
        }
    }

    let mut clock = PhaseClock::new(scratch_dir);

    // 1. index
    let pbf = pbf::PbfFile::open(input)?;
    let ranges = pbf.type_ranges()?;
    clock.phase("index");
    if std::env::var("RIDI_STOP_AFTER").as_deref() == Ok("index") {
        // probe mode: only locate the type ranges (decodes ~40 blobs)
        write_manifest(output_dir, tile_size, Vec::new(), "none")?;
        clock.total();
        return Ok(());
    }

    // 2. relations
    let rel_data = passes::relation_pass(&pbf, &ranges)?;
    let unresolved = passes::resolvable_relations(&rel_data);
    let mut area_members: util::IdSet = util::IdSet::default();
    for a in &rel_data.areas {
        if !unresolved.contains(&a.id) {
            area_members.extend(a.ways.iter().map(|(w, _)| *w));
        }
    }
    let mut restriction_way_ids: util::IdSet = util::IdSet::default();
    for r in &rel_data.restrictions {
        if !unresolved.contains(&r.id) {
            restriction_way_ids.extend(r.members.iter().filter(|m| m.0 == 1).map(|m| m.2));
        }
    }
    pbf::log_io_counters("relations");
    clock.phase("relations");

    // 3. ways
    let needed = NeededSet::new();
    let routable_spill = SpillWriter::create(&scratch_dir.join("routable_ways.bin"))?;
    let area_spill = SpillWriter::create(&scratch_dir.join("area_ways.bin"))?;
    passes::way_pass(
        &pbf,
        &ranges,
        &area_members,
        &needed,
        &routable_spill,
        &area_spill,
    )?;
    let routable_path = routable_spill.finish()?;
    let area_path = area_spill.finish()?;
    drop(area_members);
    pbf::log_io_counters("ways");
    clock.phase("ways");

    // 4. rank + coordinate store
    let set = needed.finish();
    info!("{} needed nodes", set.count);
    let store = Store::create(scratch_dir, set.count, store_budget)?;
    clock.phase("rank");

    // 5. nodes
    let bounds = passes::node_pass(&pbf, &ranges, &set, &store)?;
    drop(pbf);
    if bounds.count == 0 {
        anyhow::bail!("No nodes found in PBF {input:?}");
    }
    let resolver = Resolver {
        set: &set,
        store: &store,
        budget_bytes: store_budget,
        scratch: scratch_dir,
    };
    let anchor = if global_anchor {
        GridAnchor::global()
    } else {
        GridAnchor::reference(
            decimicro_to_deg(bounds.lat_min),
            decimicro_to_deg(bounds.lat_max),
            decimicro_to_deg(bounds.lon_min),
            decimicro_to_deg(bounds.lon_max),
        )
    };
    info!("Proximity grid anchor ({}): {:?}", if global_anchor { "global" } else { "reference" }, anchor);
    pbf::log_io_counters("nodes");
    clock.phase("nodes");

    // 6. route ways into region buckets
    let bucket_dir = scratch_dir.join("regions");
    let buckets = RegionBuckets::new(&bucket_dir, grid.all_regions())?;
    let nregions = (grid.region_cols * grid.region_rows) as usize;
    let region_used: Vec<AtomicBool> = (0..nregions).map(|_| AtomicBool::new(false)).collect();
    let spill = SpillReader::open(&routable_path)?;
    let (member_regions, _route_stats) = distribute::distribute_ways(
        &spill,
        &resolver,
        &grid,
        &buckets,
        &restriction_way_ids,
        &region_used,
    )?;
    drop(spill);
    std::fs::remove_file(&routable_path)?;
    distribute::distribute_restrictions(
        &rel_data.restrictions,
        &unresolved,
        &member_regions,
        &buckets,
    )?;
    drop(member_regions);
    clock.phase("route");

    // 7. polygons
    let large_path = scratch_dir.join("large_polygons.bin");
    let large = distribute::LargePolys::create(
        &large_path,
        std::env::var("RIDI_LARGE_POLY_REGIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(16),
    )?;
    let spill = SpillReader::open(&area_path)?;
    distribute::distribute_polygons(
        &spill,
        &resolver,
        &grid,
        &buckets,
        &rel_data.areas,
        &unresolved,
        &region_used,
        &large,
    )?;
    info!(
        "{} large polygons (> {} regions) kept in the shared set",
        large.count.load(Ordering::Relaxed),
        large.max_regions
    );
    large.finish()?;
    drop(spill);
    std::fs::remove_file(&area_path)?;
    drop(rel_data);
    store.remove_file();
    drop(store);
    drop(set);
    drop(buckets);
    clock.phase("areas");
    if std::env::var("RIDI_STOP_AFTER").as_deref() == Ok("areas") {
        // measurement mode (front-end only): no tiles are written, empty manifest
        write_manifest(output_dir, tile_size, Vec::new(), "none")?;
        clock.total();
        return Ok(());
    }

    // 8. tiles, per region (VmHWM reset: the tiles PHASE line reports this stage's own peak)
    clock.reset_hwm();
    let sizes: Vec<u64> = (0..nregions as u32)
        .map(|r| {
            if region_used[r as usize].load(Ordering::Relaxed) {
                std::fs::metadata(RegionBuckets::path_for(&bucket_dir, r))
                    .map(|m| m.len())
                    .unwrap_or(0)
            } else {
                0
            }
        })
        .collect();
    // Job size target: big enough to amortize border duplication, small enough that there are
    // several jobs per worker (tail latency) — total / (4 * threads), clamped to 4..24 MB.
    let total_bytes: u64 = sizes.iter().sum();
    let job_target: u64 = std::env::var("RIDI_JOB_TARGET_MB")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|v| v << 20)
        .unwrap_or_else(|| {
            (total_bytes / (4 * rayon::current_num_threads() as u64)).clamp(4 << 20, 24 << 20)
        });
    let mut jobs = plan_jobs(&grid, &sizes, &bucket_dir, job_target);
    jobs.sort_by_key(|j| std::cmp::Reverse(j.bytes));
    let global = local::GlobalPolys::load(&large_path)?;
    let local_budget: u64 = std::env::var("RIDI_LOCAL_BUDGET_MB")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|v| v << 20)
        .unwrap_or_else(|| {
            // poland measured ~23x RSS per in-flight bucket byte (incl. allocator retention)
            (mem_limit.saturating_sub(2 << 30) / 24).clamp(64 << 20, 2 << 30)
        });
    info!(
        "Processing {} jobs from {} base regions ({} tiles per side, {} total bucket MB, largest job {} MB, budget {} MB), {} shared large polygons",
        jobs.len(),
        sizes.iter().filter(|s| **s > 0).count(),
        region_tiles,
        sizes.iter().sum::<u64>() >> 20,
        jobs.first().map(|j| j.bytes >> 20).unwrap_or(0),
        local_budget >> 20,
        global.len()
    );
    let times = std::sync::Arc::new(local::SubPhaseTimes::default());
    let (stats, tiles) = run_jobs(
        jobs,
        tile_size,
        anchor,
        output_dir,
        keep,
        global,
        local_budget,
        times.clone(),
    )?;
    info!("Tile stage: {stats:?}; {}", times.summary());
    clock.phase("tiles");

    // fail loud on tile-grid violations (reference: cols < ceil(360/ts), rows < ceil(180/ts))
    if let Some(t) = tiles
        .iter()
        .find(|t| t.col as i64 >= grid.max_col() || t.row as i64 >= grid.max_row())
    {
        anyhow::bail!("internal error: tile {} outside the valid tile grid", t.filename);
    }
    {
        let mut names: Vec<&str> = tiles.iter().map(|t| t.filename.as_str()).collect();
        names.sort_unstable();
        let n = names.len();
        names.dedup();
        anyhow::ensure!(names.len() == n, "internal error: a tile was written by two jobs");
    }

    // 9. manifest (from in-memory tile results)
    if tiles.is_empty() {
        tracing::warn!("No tiles were generated");
    }
    let source = input
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    write_manifest(output_dir, tile_size, tiles, source)?;
    clock.phase("manifest");
    clock.total();
    drop(scratch);
    Ok(())
}

/// Group base regions into jobs: each 4x4 block of base regions becomes one job if its buckets
/// total <= `target` bytes, else each 2x2 quadrant that fits becomes a job, else single regions.
/// Sparse areas get few, larger jobs (less duplicated parsing of border ways / polygons); dense
/// areas get small jobs so the tail of the tile stage stays short.
fn plan_jobs(grid: &TileGrid, sizes: &[u64], dir: &Path, target: u64) -> Vec<local::Job> {
    let rt = grid.region_tiles;
    let max_col = grid.max_col() as u32;
    let max_row = grid.max_row() as u32;
    let mut jobs = Vec::new();
    let size_of = |c: u32, r: u32| -> u64 {
        if c >= grid.region_cols || r >= grid.region_rows {
            0
        } else {
            sizes[(r * grid.region_cols + c) as usize]
        }
    };
    let rect_size = |c0: u32, r0: u32, n: u32| -> u64 {
        let mut t = 0;
        for r in r0..r0 + n {
            for c in c0..c0 + n {
                t += size_of(c, r);
            }
        }
        t
    };
    let make = |c0: u32, r0: u32, n: u32, jobs: &mut Vec<local::Job>| {
        let mut files = Vec::new();
        let mut bytes = 0;
        for r in r0..r0 + n {
            for c in c0..c0 + n {
                let sz = size_of(c, r);
                if sz > 0 {
                    files.push(RegionBuckets::path_for(dir, r * grid.region_cols + c));
                    bytes += sz;
                }
            }
        }
        if files.is_empty() {
            return;
        }
        let col0 = c0 * rt;
        let row0 = r0 * rt;
        jobs.push(local::Job {
            col0,
            row0,
            ncols: (n * rt).min(max_col - col0),
            nrows: (n * rt).min(max_row - row0),
            files,
            bytes,
        });
    };
    let mut sr = 0;
    while sr < grid.region_rows {
        let mut sc = 0;
        while sc < grid.region_cols {
            let total = rect_size(sc, sr, 4);
            if total == 0 {
            } else if total <= target {
                make(sc, sr, 4, &mut jobs);
            } else {
                for (qc, qr) in [(0, 0), (2, 0), (0, 2), (2, 2)] {
                    let q = rect_size(sc + qc, sr + qr, 2);
                    if q == 0 {
                        continue;
                    }
                    if q <= target {
                        make(sc + qc, sr + qr, 2, &mut jobs);
                    } else {
                        for (bc, br) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            make(sc + qc + bc, sr + qr + br, 1, &mut jobs);
                        }
                    }
                }
            }
            sc += 4;
        }
        sr += 4;
    }
    jobs
}

/// Run the tile stage on the rayon pool, admitting jobs (largest first) only while the sum of
/// in-flight bucket bytes stays within `budget` (measured RAM per job is ~12-23x its bucket
/// bytes). A job larger than the budget runs alone.
fn drop_output_cache() -> bool {
    std::env::var("RIDI_DROP_OUTPUT_CACHE").as_deref() != Ok("0")
}

#[allow(clippy::too_many_arguments)]
fn run_jobs(
    jobs: Vec<local::Job>,
    tile_size: f32,
    anchor: GridAnchor,
    output_dir: &Path,
    keep: bool,
    global: local::GlobalPolys,
    budget: u64,
    times: std::sync::Arc<local::SubPhaseTimes>,
) -> Result<(local::RegionStats, Vec<ridi_router_common::manifest::TileMetadata>)> {
    use std::sync::{mpsc, Arc, Condvar, Mutex};
    type JobResult = Result<(local::RegionStats, Vec<ridi_router_common::manifest::TileMetadata>)>;
    let gate = Arc::new((Mutex::new(0u64), Condvar::new()));
    let (tx, rx) = mpsc::channel::<JobResult>();
    let (flush_tx, flush_rx) = mpsc::channel::<Vec<PathBuf>>();
    let flush_rx = Arc::new(Mutex::new(flush_rx));
    let flushers: Vec<_> = if drop_output_cache() {
        (0..4)
            .map(|_| {
                let rx = flush_rx.clone();
                std::thread::spawn(move || loop {
                    let batch = rx.lock().unwrap().recv();
                    let Ok(paths) = batch else { break };
                    for p in paths {
                        util::flush_and_drop_cache(&p);
                    }
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let flush_tx = (!flushers.is_empty()).then_some(flush_tx);
    let out = Arc::new(output_dir.to_path_buf());
    let global = Arc::new(global);
    let n = jobs.len();
    let collector = std::thread::spawn(move || -> JobResult {
        let mut stats = local::RegionStats::default();
        let mut tiles = Vec::new();
        let mut done = 0;
        let started = std::time::Instant::now();
        let mut last_log = started;
        for s in rx {
            let (st, t) = s?;
            stats = stats.add(st);
            tiles.extend(t);
            done += 1;
            if last_log.elapsed().as_secs() >= 30 || done == n {
                last_log = std::time::Instant::now();
                info!(
                    "Tile jobs: {done}/{n} done, {} tiles written, {:.0}s elapsed",
                    stats.tiles,
                    started.elapsed().as_secs_f64()
                );
            }
        }
        anyhow::ensure!(done == n, "tile jobs lost ({done}/{n})");
        Ok((stats, tiles))
    });
    // Jobs run on dedicated OS threads, not on rayon workers: local_stage::run blocks while its
    // writer threads drain, and a blocked rayon worker would idle a core. The parallel parts of
    // each job still execute on the global rayon pool.
    let job_threads: usize = std::env::var("RIDI_JOB_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(rayon::current_num_threads)
        .max(1);
    type Task = Box<dyn FnOnce() + Send>;
    let (task_tx, task_rx) = mpsc::sync_channel::<Task>(job_threads);
    let task_rx = Arc::new(Mutex::new(task_rx));
    let workers: Vec<_> = (0..job_threads)
        .map(|i| {
            let rx = task_rx.clone();
            std::thread::Builder::new()
                .name(format!("tile-job-{i}"))
                .spawn(move || loop {
                    let task = rx.lock().unwrap().recv();
                    match task {
                        Ok(t) => t(),
                        Err(_) => break,
                    }
                })
                .expect("spawn tile job thread")
        })
        .collect();
    for job in jobs {
        let size = job.bytes;
        {
            let (lock, cv) = &*gate;
            let mut in_flight = lock.lock().unwrap();
            while *in_flight > 0 && *in_flight + size > budget {
                in_flight = cv.wait(in_flight).unwrap();
            }
            *in_flight += size;
        }
        let gate = gate.clone();
        let tx = tx.clone();
        let out = out.clone();
        let global = global.clone();
        let times = times.clone();
        let flush = flush_tx.clone();
        let task: Task = Box::new(move || {
            let s = local::process_job(&job, tile_size, anchor, &out, &global, &times);
            if let (Ok((_, tiles)), Some(flush)) = (&s, &flush) {
                // Written tiles are not read again: a background thread flushes them and drops
                // them from the page cache so dirty output does not push anonymous memory into
                // swap (done off the rayon pool: the writeback wait would idle the workers).
                let _ = flush.send(tiles.iter().map(|t| out.join(&t.filename)).collect());
            }
            if !keep {
                for p in &job.files {
                    let _ = std::fs::remove_file(p);
                }
            }
            let (lock, cv) = &*gate;
            *lock.lock().unwrap() -= size;
            cv.notify_all();
            let _ = tx.send(s);
        });
        task_tx
            .send(task)
            .map_err(|_| anyhow::anyhow!("tile job threads stopped"))?;
    }
    drop(task_tx);
    for w in workers {
        w.join().unwrap_or_else(|p| std::panic::resume_unwind(p));
    }
    drop(tx);
    let collected = collector
        .join()
        .unwrap_or_else(|p| std::panic::resume_unwind(p));
    drop(flush_tx);
    for f in flushers {
        let _ = f.join();
    }
    collected
}
