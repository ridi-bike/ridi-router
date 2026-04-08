# Route generation performance review

## Exact command run

```bash
RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia
```

## What `dev.sh` resolved and ran

- preset: `default`
- rule file: `rule-examples/rules-default.json`
- start query: `riga,latvia`
- finish query: `sigulda,latvia`
- resolved start: `Rīga, Latvija -> 56.9493977,24.1051846`
- resolved finish: `Sigulda, Siguldas novads, LV-2150, Latvija -> 57.1540561,24.8567141`

Underlying CLI command:

```bash
target/release/ridi-router-cli generate-route \
  --tiles /home/toms/dev/ridi-router/map-data/output \
  --output-dir /home/toms/dev/ridi-router/map-data/routes \
  --format gpx \
  --rule-file /home/toms/dev/ridi-router/rule-examples/rules-default.json \
  start-finish \
  --start 56.9493977,24.1051846 \
  --finish 57.1540561,24.8567141
```

## Run status

- Exit status: **0**
- Cargo reuse/build overhead before run: **0.38 s**
- Route generation started: **2026-04-08 19:43:27.827Z**
- Router logged `Routes from itineraries` at **109 s** with:
  - `itinerary_count=109`
  - `routes_count=79`
- Router logged `Route generation finished` at **114 s**
- Measured wall time from router start to router finish: **114.22 s**
- Final Hotpath summary was printed at **115.79 s** uptime
- Output written successfully to `map-data/routes`
- Final route files produced: **23 GPX files**
- Output size: **4.57 MB total**
- Output route filename lengths ranged from **81 km** to **261 km**

Important note: this was a **profiled** run with `hotpath`, `hotpath-alloc`, and `hotpath-mcp` enabled, so absolute wall time includes profiling overhead. Use the hotspot rankings much more than the raw runtime.

## Live Hotpath MCP snapshots while the run was active

I queried Hotpath over MCP during the live run.

### Snapshot A: ~12 s uptime

- profiler uptime: **12.00 s**
- RSS: **1.2 GB**
- thread count: **19**
- live alloc/dealloc diff: **92.8 MB**
- busiest worker threads were each around **36% to 44% CPU**
- profiling helper thread `hp-functions` was using about **95.8% CPU**

Top timing hotspots at this point:

| Function | Total | Why it stood out early |
|---|---:|---|
| `walker::get_fork_segments_for_segment_with_context` | **46.44 s** | branching work dominated immediately |
| `walker::move_forward_to_next_fork_with_context` | **44.48 s** | forward exploration was the main traversal cost |
| `graph::get_adjacent` | **38.30 s** | adjacency lookup stack was already large |
| `navigator::generate_routes_with_context` | **29.07 s** | top-level search wrapper stayed busy |
| `walker::get_roundabout_exits_with_context` | **26.99 s** | roundabout logic was already expensive |

Top allocation hotspots at this point:

| Function | Total alloc | Why it stood out early |
|---|---:|---|
| `walker::move_forward_to_next_fork_with_context` | **815.8 MB** | biggest early traversal allocator |
| `walker::get_fork_segments_for_segment_with_context` | **749.5 MB** | fork expansion churn |
| `walker::get_roundabout_exits_with_context` | **695.7 MB** | expensive branch enumeration |
| `graph::get_adjacent` | **596.8 MB** | adjacency materialization cost |
| `navigator::generate_routes_with_context` | **501.8 MB** | overall search state was already large |

### Snapshot B: ~66.8 s uptime

- profiler uptime: **66.82 s**
- RSS: **4.9 GB**
- thread count: **19**
- live alloc/dealloc diff: **118.1 MB**
- worker threads were still mostly around **36% to 40% CPU each**
- `hp-functions` still consumed about **95.8% CPU**

Top timing hotspots at this point:

| Function | Total | Why it mattered mid-run |
|---|---:|---|
| `navigator::generate_routes_with_context` | **379.64 s** | whole search stayed deep and wide |
| `walker::get_fork_segments_for_segment_with_context` | **264.32 s** | fork discovery remained a core cost |
| `walker::move_forward_to_next_fork_with_context` | **253.10 s** | forward search stayed expensive |
| `graph::get_adjacent` | **220.45 s** | adjacency plumbing remained heavy |
| `walker::get_roundabout_exits_with_context` | **155.18 s** | roundabout handling still large |

Top allocation hotspots at this point:

| Function | Total alloc | Why it mattered mid-run |
|---|---:|---|
| `navigator::generate_routes_with_context` | **6.0 GB** | search-state growth dominated cumulative allocs |
| `walker::move_forward_to_next_fork_with_context` | **4.2 GB** | forward traversal churn |
| `walker::get_fork_segments_for_segment_with_context` | **3.9 GB** | branch expansion churn |
| `walker::get_roundabout_exits_with_context` | **3.6 GB** | roundabout branching churn |
| `graph::get_adjacent` | **3.1 GB** | adjacency lookup/materialization cost |

Tooling note:

- `hotpath_tokio_runtime` was **not available** because this app does not currently register Tokio runtime metrics with `hotpath::tokio_runtime!()`.

## Final Hotpath timing snapshot at completion

Note: Hotpath totals are **inclusive cumulative time**, so totals above wall-clock time and percentages above 100% are expected.

| Function | Calls | Total | Why it matters |
|---|---:|---:|---|
| `navigator::generate_routes_with_context` | 89 | **587.74 s** | top-level route search stayed active almost the whole run |
| `walker::get_fork_segments_for_segment_with_context` | 18,117,321 | **394.85 s** | biggest branching hotspot |
| `walker::move_forward_to_next_fork_with_context` | 1,683,498 | **377.03 s** | forward exploration remains very expensive |
| `graph::get_adjacent` | 18,791,470 | **329.08 s** | adjacency lookup stack is still first-order cost |
| `walker::get_roundabout_exits_with_context` | 491,091 | **232.31 s** | roundabout traversal is still a major hotspot |
| `graph::get_tag_value` | 23,590,840 | **136.79 s** | tag reads add up heavily inside traversal |
| `walker::move_backwards_to_prev_fork_with_context` | 607,298 | **123.63 s** | backward search is still significant |
| `weights::weight_heading` | 673,941 | **62.98 s** | biggest route-logic timing cost |
| `graph::get_point_record_from_tiles` | 3,187,492 | **49.04 s** | point record lookup remains noticeable |
| `weights::weight_no_short_detours` | 673,941 | **46.07 s** | detour logic is meaningful but not dominant |
| `route::is_back_on_road_within_distance` | 280,882 | **43.57 s** | expensive backward-scan safety check |
| `graph::get_point_from_tiles` | 1,596,056 | **40.66 s** | still visible, but no longer the headline bottleneck |
| `graph::get_line_from_tiles` | 1,718,190 | **37.75 s** | repeated line hydration/lookups still cost real time |
| `tile_manager::get_adjacent_by_id` | 18,791,471 | **30.19 s** | hot inner plumbing function |
| `weights::weight_rules_highway` | 673,941 | **29.58 s** | largest rules-based weight cost |

Other measurable but secondary timing costs:

- `walker::get_segments_for_point_with_context`: **25.12 s**
- `weights::weight_rules_surface`: **17.56 s**
- `weights::weight_prefer_same_road`: **15.26 s**
- `tile_manager::touch_loaded_tile`: **12.27 s**
- `tile_manager::ensure_tile_loaded`: **12.16 s**
- `tile_manager::loaded_tile`: **11.82 s**
- `tile_manager::get_tag_value_if_loaded`: **11.49 s**
- `weights::weight_no_sharp_turns`: **9.83 s**

## Final Hotpath allocation snapshot at completion

Note: this is **inclusive cumulative allocation bytes**, not peak resident memory.

Total cumulative allocation reported by Hotpath: **40.4 GB**

| Function | Calls | Total alloc | Why it matters |
|---|---:|---:|---|
| `navigator::generate_routes_with_context` | 89 | **9.3 GB** | biggest cumulative allocator in the completed run |
| `walker::move_forward_to_next_fork_with_context` | 1,683,498 | **6.2 GB** | traversal churn remains huge |
| `walker::get_fork_segments_for_segment_with_context` | 18,117,321 | **5.7 GB** | branch expansion allocates constantly |
| `walker::get_roundabout_exits_with_context` | 491,091 | **5.3 GB** | roundabout exploration is a major allocator |
| `graph::get_adjacent` | 18,791,470 | **4.6 GB** | adjacency lookup/materialization churn |
| `tile_manager::get_adjacent_by_id` | 18,791,471 | **3.3 GB** | hot low-level allocator in the same stack |
| `walker::move_backwards_to_prev_fork_with_context` | 607,298 | **2.7 GB** | backward exploration also allocates heavily |
| `weights::weight_heading` | 673,941 | **1.0 GB** | biggest route-logic allocator |
| `walker::get_segments_for_point_with_context` | 674,147 | **364.5 MB** | segment gathering still churns memory |
| `weights::weight_no_short_detours` | 673,941 | **288.9 MB** | meaningful allocator, but below traversal costs |
| `route::is_back_on_road_within_distance` | 280,882 | **283.7 MB** | same backward-scan pattern shows up in allocs |

Other visible but clearly smaller allocators:

- `weights::weight_no_sharp_turns`: **197.8 MB**
- `graph::get_tag_value`: **109.7 MB**
- `tile_manager::get_tag_value_if_loaded`: **109.5 MB**
- `itinerary::check_set_next`: **105.5 MB**
- `walker::move_to_roundabout_exit_with_context`: **95.4 MB**
- `weights::weight_check_avoid_rules`: **81.6 MB**
- `weights::weight_rules_highway`: **78.0 MB**
- `weights::weight_rules_surface`: **70.7 MB**
- `weights::weight_rules_smoothness`: **64.1 MB**
- `graph::get_point_from_tiles`: **53.9 MB**
- `tile_manager::get_rules_for_point_if_loaded`: **2.2 MB**

## Main findings

1. **The route now completes, but it is still slow.**
   - This run finished successfully instead of stalling indefinitely.
   - The actual routing phase still took about **114 s** under profiling.

2. **The biggest story is now search orchestration and traversal, not raw point hydration.**
   - `navigator::generate_routes_with_context` is the top timing and allocation entry.
   - The main cost cluster is the walker stack: forward moves, fork discovery, roundabout exits, and backward moves.

3. **Fork and roundabout exploration remain the primary bottleneck.**
   - `get_fork_segments_for_segment_with_context`, `move_forward_to_next_fork_with_context`, and `get_roundabout_exits_with_context` dominate both time and allocation.
   - This suggests the router still spends most of its budget expanding and scoring branches rather than finalizing routes.

4. **The route-logic weights matter, but they are not the first problem.**
   - `weight_heading` is the largest rules/weight timing and allocation cost.
   - `weight_no_short_detours` and the highway/surface/smoothness weights are visible, but still secondary to traversal.

5. **Memory pressure is real, and the RSS story is not explained by alloc/dealloc diff alone.**
   - At the 66.8 s live sample, RSS was about **4.9 GB**.
   - Hotpath's live alloc/dealloc diff at that moment was only about **118 MB**.
   - That points to substantial resident memory outside the simple live-allocation delta Hotpath reports here, or to retention/profiler effects that deserve separate memory-focused investigation.

6. **Profiling overhead is non-trivial.**
   - The `hp-functions` helper thread was consuming about one full core while I sampled the run.
   - Treat this report as a hotspot map first, not a clean production-runtime benchmark.

## Comparison with the previous saved report in this repo

Compared with the older `perf.md` content that was in the repo before this run:

- **Previously:** the same command had not finished by about **102.64 s** and was killed.
- **Now:** the route finished successfully in about **114.22 s** wall time under profiling.
- **Previously:** `graph::get_point_from_tiles` and `get_rules_for_point` were dominant allocation hotspots at **57.8 GB** and **57.2 GB** inclusive alloc.
- **Now:** `graph::get_point_from_tiles` is only **53.9 MB** inclusive alloc, and `tile_manager::get_rules_for_point_if_loaded` is only **2.2 MB**.
- **Previously:** `graph::get_point_from_tiles` dominated timing at **463.03 s** inclusive total.
- **Now:** it is down to **40.66 s**, and the main story has shifted to the broader navigator/walker traversal stack.

That is a real improvement in the old graph-hydration hotspot area. The remaining bottleneck is now much more clearly the route-search branching work itself.

## Bottom line

For `riga,latvia -> sigulda,latvia`, the profiled run now **finishes successfully** and writes **23 GPX files**, but it still spends roughly **114 seconds** in route generation under Hotpath. The main remaining bottleneck is **branch-heavy route search**: fork discovery, forward/backward traversal, roundabout exit handling, and the top-level navigator loop. The older `get_point_from_tiles` / `get_rules_for_point` allocation story is no longer the headline issue. If you want the next meaningful speedup, focus on **reducing branch expansion and repeated traversal work inside the navigator/walker stack**.