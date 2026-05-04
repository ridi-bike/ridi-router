# Route perf review

_Date:_ 2026-04-11

## Command

```bash
RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia
```

## Artifacts

- run A log: `.pi/perf/run-20260411-013410.log`
- run A meta: `.pi/perf/run-20260411-013410.meta`
- run B log: `.pi/perf/run-20260411-013530.log`
- run B meta: `.pi/perf/run-20260411-013530.meta`
- output dir: `map-data/routes`

## Scope / environment notes

- Profile reflects current local worktree, not clean `HEAD`.
- Dirty routing files during run:
  - `crates/ridi-router-routing/src/router/navigator.rs`
  - `crates/ridi-router-routing/src/router/route/mod.rs`
  - `crates/ridi-router-routing/src/router/walker.rs`
  - `crates/ridi-router-routing/src/router/weights.rs`
  - `crates/ridi-router-routing/src/routing_context.rs`
- Tiles already existed in `map-data/output`, so this is route-search profile, not tile-generation profile.
- Both runs completed successfully. Earlier `BorrowMutError` blocker is gone.

## Executive summary

Current perf problem is no longer startup or snap/open. Main cost is route exploration inside walker.

Stable hottest stack across 2 warm runs:

1. `walker::move_forward_to_next_fork_with_context`
2. `walker::with_fork_segments_for_segment_with_context`
3. `weights::weight_heading`
4. `weights::weight_no_short_detours` + `route::is_back_on_road_within_distance`

Meaning:

- forward traversal still dominates wall time
- fork-segment discovery/classification still dominates allocation churn
- heading + backward road-name scan still cost real time and memory
- startup/snap work is now small enough to ignore for first-pass optimization

## Run summary

| Run | Cargo rebuild | Route gen from log | Hotpath elapsed | Wrapper wall | Itineraries | Candidate routes | GPX files |
|---|---:|---:|---:|---:|---:|---:|---:|
| A (`013410`) | 0.56 s | 37 s | 39.75 s | 42 s | 109 | 79 | 23 |
| B (`013530`) | 0.37 s | 40 s | 44.01 s | 46 s | 109 | 79 | 23 |

Notes:

- Generator logged `routes_count: 79` before clustering/scoring.
- Final output dir contains 23 GPX files.
- That drop is expected: `generator.rs` clusters routes, keeps best route per cluster, then appends small noise sample before returning best routes.

## Live Hotpath MCP snapshot

One live MCP attach succeeded during run A.

Snapshot at `23.76 s` profiler uptime:

- total threads: **19**
- RSS: **9.3 GB**
- total alloc: **4.9 GB**
- total dealloc: **4.7 GB**
- live alloc-dealloc diff: **203.9 MB**
- app worker threads: 8 `ridi-router-cli` workers, each peaking around **84%–92% CPU**, averaging roughly **49%–55% CPU** over sample window

Takeaway:

- app is parallel and CPU-heavy
- but snapshot did not show all workers saturated at once
- memory traffic is big, but retained memory is modest compared with cumulative allocation churn
- biggest problem looks like repeated exploration/allocation work, not leak-like growth

## Timing hotspots

### Run A

| Function | Calls | Total | % total |
|---|---:|---:|---:|
| `walker::move_forward_to_next_fork_with_context` | 316,665 | 19.74 s | 49.67% |
| `navigator::generate_routes_with_context` | 18 | 15.12 s | 38.04% |
| `walker::with_fork_segments_for_segment_with_context` | 995,398 | 12.28 s | 30.89% |
| `weights::weight_heading` | 124,614 | 4.74 s | 11.92% |
| `weights::weight_no_short_detours` | 126,204 | 2.03 s | 5.11% |
| `route::is_back_on_road_within_distance` | 63,730 | 1.95 s | 4.90% |
| `walker::move_backwards_to_prev_fork_with_context` | 116,270 | 1.32 s | 3.33% |
| `walker::classify_segments_for_point_with_context` | 124,667 | 1.56 s | 3.93% |

### Run B

| Function | Calls | Total | % total |
|---|---:|---:|---:|
| `walker::move_forward_to_next_fork_with_context` | 322,203 | 20.26 s | 46.04% |
| `navigator::generate_routes_with_context` | 16 | 14.87 s | 33.80% |
| `walker::with_fork_segments_for_segment_with_context` | 1,000,334 | 12.65 s | 28.74% |
| `weights::weight_heading` | 126,958 | 5.06 s | 11.49% |
| `weights::weight_no_short_detours` | 127,952 | 1.54 s | 3.51% |
| `route::is_back_on_road_within_distance` | 47,692 | 1.48 s | 3.37% |
| `walker::move_backwards_to_prev_fork_with_context` | 117,470 | 1.34 s | 3.03% |
| `walker::classify_segments_for_point_with_context` | 126,987 | 1.59 s | 3.61% |

### Timing read

What stands out:

- `move_forward_to_next_fork_with_context` alone burns about **20 s** each run.
- `with_fork_segments_for_segment_with_context` adds another **12.3–12.7 s**.
- `weight_heading` is still large at **4.7–5.1 s**.
- `weight_no_short_detours` and `is_back_on_road_within_distance` remain meaningful second-tier hotspots.
- `classify_segments_for_point_with_context` is no longer top-of-chart. It matters, but it is not first target now.

Important non-finding:

- `routing_api::open`, `snap_start`, `snap_finish`, closest-point search, tile open path are now around **~1% each** or less.
- so snap/open path is no longer first optimization target for this command.

## Allocation hotspots

### Run A

| Function | Calls | Total alloc | % total alloc |
|---|---:|---:|---:|
| `walker::move_forward_to_next_fork_with_context` | 316,665 | 775.3 MB | 29.27% |
| `walker::with_fork_segments_for_segment_with_context` | 995,398 | 535.9 MB | 20.23% |
| `navigator::generate_routes_with_context` | 18 | 513.2 MB | 19.38% |
| `weights::weight_heading` | 124,614 | 180.2 MB | 6.80% |
| `route::is_back_on_road_within_distance` | 63,730 | 57.6 MB | 2.18% |
| `weights::weight_no_short_detours` | 126,204 | 57.6 MB | 2.18% |
| `graph::get_closest_to_coords` | 44 | 32.4 MB | 1.22% |
| `generator::create_waypoints_around` | 2 | 24.0 MB | 0.90% |

### Run B

| Function | Calls | Total alloc | % total alloc |
|---|---:|---:|---:|
| `walker::move_forward_to_next_fork_with_context` | 322,203 | 781.6 MB | 29.73% |
| `walker::with_fork_segments_for_segment_with_context` | 1,000,334 | 537.3 MB | 20.44% |
| `navigator::generate_routes_with_context` | 16 | 494.7 MB | 18.82% |
| `weights::weight_heading` | 126,958 | 192.5 MB | 7.32% |
| `route::is_back_on_road_within_distance` | 47,692 | 43.4 MB | 1.65% |
| `weights::weight_no_short_detours` | 127,952 | 43.4 MB | 1.65% |
| `graph::get_closest_to_coords` | 44 | 32.4 MB | 1.23% |
| `generator::create_waypoints_around` | 2 | 24.0 MB | 0.91% |

### Allocation read

Main story:

- route exploration churns **way more** memory than startup/snap
- `move_forward_to_next_fork_with_context` + `with_fork_segments_for_segment_with_context` together account for about **1.31 GB** cumulative alloc each run
- `weight_heading` is not only slow; it is also memory-heavy
- `is_back_on_road_within_distance` / `weight_no_short_detours` still allocate more than they should for helper logic
- closest-point snapping allocates some memory, but nowhere near enough to justify first attention now

## Stability across runs

Hotspot order stayed same across both warm runs.

Variation:

- overall runtime moved by about **10%** (`39.75 s` → `44.01 s` hotpath elapsed)
- top 3 hotspots stayed within same band
- route counts stayed identical: **109 itineraries**, **79 candidate routes**, **23 GPX outputs**

So profile is stable enough to guide optimization work.

## What changed vs previous blocked perf note

This route now completes cleanly.

That means:

- old `BorrowMutError` panic is fixed in current worktree
- profile now reaches real route-search hot path
- current tables are finally valid for optimization planning

Also, current data says first target shifted.

Earlier focus on snap/open or crash path is obsolete for this exact command. Current focus should move to walker traversal and weight evaluation.

## Recommended next steps

### Priority 1 — shrink work inside `move_forward_to_next_fork_with_context`

Reason:

- biggest time bucket by far
- biggest allocation bucket by far

What to inspect first:

- repeated branch expansion from same oriented segments
- repeated route cloning / `SegmentList -> Vec` conversions inside forward stepping
- repeated cache lookups that could be hoisted per fork / per segment
- whether same segment/fork exploration is revisited across itineraries without memoization

Expected payoff: biggest wall-time win, biggest allocation win.

### Priority 2 — reduce churn in `with_fork_segments_for_segment_with_context`

Reason:

- second-biggest time bucket
- second-biggest allocation bucket
- called about **1.0 million** times per run

What to inspect first:

- result cache keying for oriented segments
- avoid rebuilding fork-segment collections when caller only needs filtered subset
- borrow cached line/adjacency/tag data instead of cloning into fresh vectors

Expected payoff: big alloc win, medium-to-big wall-time win.

### Priority 3 — cut `weight_heading` cost

Reason:

- stable **~5 s** hotspot
- stable **180–193 MB** alloc hotspot

What to inspect first:

- avoid recomputing geometry / heading inputs already known in walker
- avoid temporary allocations in angle/segment look-ahead helpers
- consider early exit when other cheap fork rules already reject path

Expected payoff: solid wall-time win on every explored fork.

### Priority 4 — cheapen backward road-name scan helpers

Targets:

- `weights::weight_no_short_detours`
- `route::is_back_on_road_within_distance`

Reason:

- together still cost **~3.0–4.0 s**
- together still allocate **~87–115 MB**

What to inspect first:

- tag value caching
- road-name / road-ref caching per segment
- avoid repeated backward scan over same route suffix

Expected payoff: good second-order win after walker-forward path.

### Priority 5 — leave snap/open alone for now

Reason:

- no longer primary bottleneck
- even perfect snap optimization would move total runtime only modestly for this command

## Bottom line

`RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia` now works and produces route output.

Main finding:

> route-search cost is dominated by walker forward exploration and fork-segment work, not startup, snapping, or tile open.

Best optimization target order:

> `move_forward_to_next_fork_with_context` → `with_fork_segments_for_segment_with_context` → `weight_heading` → `weight_no_short_detours` / `is_back_on_road_within_distance`
