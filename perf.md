# Route generation performance review

_Date:_ 2026-04-08

## Command I ran

I interpreted your requested command as the documented perf route invocation used by this repo:

```bash
RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia
```

Why: `dev.sh` expects the `route` subcommand, and `RIDI_FEATURES=perf` expands to `hotpath,hotpath-alloc,hotpath-mcp`.

## Method

I used two profiled runs:

- **Run A**: main evidence run, with a live Hotpath MCP snapshot while routing was active.
- **Run B**: quick repeat to check whether completion time and hotspot family were stable.

I also compared the new results against the `./perf.md` content that existed before this overwrite.

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

### Run A

- Cargo reuse/build overhead before run: **0.17 s**
- Route generation started: **2026-04-08 21:41:43.629Z**
- Router logged `Routes from itineraries` at **42 s** with:
  - `itinerary_count=109`
  - `routes_count=79`
- Router logged `Route generation finished` at **2026-04-08 21:42:30.400Z**
- Measured wall time from router start to router finish: **46.77 s**
- Final Hotpath summary was printed at **50.57 s** uptime
- Output written successfully to `map-data/routes`
- Final route files produced: **23 GPX files**
- Output size: **4.29 MB total**
- Output route filename lengths ranged from **81 km** to **236 km**

### Run B

- Cargo reuse/build overhead before run: **0.46 s**
- Router logged `Routes from itineraries` at **41 s**
- Router logged `Route generation finished` at **47 s**
- Final Hotpath summary was printed at **50.29 s** uptime

The second run landed in essentially the same runtime band, so the current result looks reproducible.

## Live Hotpath MCP snapshot while the run was active

I queried Hotpath over MCP during **Run A**.

### Snapshot A: ~8.9 s uptime

Profiler/thread status:

- profiler uptime: **8.87 s**
- thread count: **20**
- RSS reported by Hotpath: **7.2 GB**
- cumulative alloc/dealloc at that point: **3.4 GB alloc / 3.3 GB dealloc**
- live alloc/dealloc diff: **95.9 MB**
- eight worker threads were already busy, mostly around **64% to 92% CPU** each
- Hotpath helper thread `hp-functions` was also using about **63.8% CPU**

Top timing hotspots at that early point:

| Function | Total | Why it stood out early |
|---|---:|---|
| `walker::move_forward_to_next_fork_with_context` | **7.35 s** | forward traversal dominated immediately |
| `navigator::generate_routes_with_context` | **5.75 s** | top-level route search stayed busy from the start |
| `walker::get_fork_segments_for_segment_with_context` | **3.43 s** | branch discovery was already expensive |
| `graph::get_tag_value` | **2.81 s** | tag reads accumulated quickly inside the hot traversal loop |
| `tile_manager::loaded_tile` | **2.63 s** | tile lookup plumbing was active constantly |
| `walker::get_roundabout_exits_with_context` | **1.87 s** | roundabout branching became visible early |

Top allocation hotspots at that early point:

| Function | Total alloc | Why it stood out early |
|---|---:|---|
| `walker::move_forward_to_next_fork_with_context` | **495.7 MB** | largest early traversal allocator |
| `walker::get_fork_segments_for_segment_with_context` | **312.6 MB** | branch-expansion churn |
| `walker::get_roundabout_exits_with_context` | **309.8 MB** | roundabout exploration allocates heavily |
| `navigator::generate_routes_with_context` | **302.6 MB** | search-state growth shows up very early |
| `walker::move_backwards_to_prev_fork_with_context` | **148.4 MB** | backward exploration is also a real memory cost |
| `weights::weight_heading` | **93.2 MB** | biggest visible route-logic allocator |

## Final Hotpath timing snapshot at completion

This table is from **Run A**.

Note: Hotpath totals are cumulative across calls, so use them mainly for ranking hotspots.

| Function | Calls | Total | % Total | Why it matters |
|---|---:|---:|---:|---|
| `navigator::generate_routes_with_context` | 15 | **14.34 s** | **28.35%** | top-level route search remains the single largest timing bucket |
| `walker::move_forward_to_next_fork_with_context` | 357,872 | **13.70 s** | **27.09%** | biggest direct traversal hotspot |
| `walker::get_fork_segments_for_segment_with_context` | 3,835,443 | **6.23 s** | **12.33%** | branch discovery is still core cost |
| `graph::get_tag_value` | 4,285,459 | **5.28 s** | **10.44%** | tag fetching still adds up heavily inside search |
| `tile_manager::loaded_tile` | 3,497,502 | **4.77 s** | **9.44%** | low-level tile access is still busy, but no longer the headline story |
| `walker::get_roundabout_exits_with_context` | 108,406 | **4.09 s** | **8.08%** | roundabout branch handling remains expensive |
| `tile_manager::get_tag_value_if_loaded` | 1,797,435 | **3.74 s** | **7.40%** | loaded-path tag reads are still significant |
| `weights::weight_heading` | 141,086 | **2.86 s** | **5.66%** | biggest route-logic timing cost |
| `weights::weight_no_short_detours` | 141,088 | **2.59 s** | **5.11%** | detour logic is meaningful but secondary to traversal |
| `route::is_back_on_road_within_distance` | 49,956 | **2.39 s** | **4.72%** | backward safety scanning is still noticeable |
| `graph::get_point_from_tiles` | 329,586 | **2.33 s** | **4.61%** | visible, but no longer dominant |
| `walker::move_backwards_to_prev_fork_with_context` | 131,319 | **2.29 s** | **4.52%** | backward traversal is still part of the core search cost |
| `graph::get_adjacent` | 329,096 | **1.48 s** | **2.93%** | adjacency lookup is now secondary rather than dominant |
| `weights::weight_rules_highway` | 141,086 | **1.48 s** | **2.93%** | biggest rules-based weight after heading/detour |

Secondary but real costs:

- `graph::get_point_record_from_tiles`: **1.98 s**
- `graph::get_line_from_tiles`: **1.19 s**
- `tile_manager::get_adjacent_by_id_if_loaded`: **1.15 s**
- `weights::weight_rules_surface`: **1.04 s**
- `weights::weight_prefer_same_road`: **1.03 s**
- `walker::get_segments_for_point_with_context`: **798.87 ms**

## Final Hotpath allocation snapshot at completion

This table is also from **Run A**.

Total cumulative allocation reported by Hotpath: **4.1 GB**

| Function | Calls | Total alloc | % Total | Why it matters |
|---|---:|---:|---:|---|
| `walker::move_forward_to_next_fork_with_context` | 357,872 | **940.0 MB** | **22.64%** | biggest remaining allocator |
| `navigator::generate_routes_with_context` | 15 | **763.0 MB** | **18.38%** | overall search orchestration still allocates a lot |
| `walker::get_roundabout_exits_with_context` | 108,406 | **667.1 MB** | **16.07%** | roundabout exploration is still costly in memory |
| `walker::get_fork_segments_for_segment_with_context` | 3,835,443 | **598.4 MB** | **14.41%** | branch expansion churn remains large |
| `walker::move_backwards_to_prev_fork_with_context` | 131,319 | **317.6 MB** | **7.65%** | backward traversal is also a substantial allocator |
| `weights::weight_heading` | 141,086 | **179.9 MB** | **4.33%** | biggest route-logic allocator |
| `walker::get_segments_for_point_with_context` | 141,133 | **74.2 MB** | **1.79%** | still non-trivial, but below the main walker functions |
| `graph::get_adjacent` | 329,096 | **46.5 MB** | **1.12%** | now much smaller than the core traversal buckets |
| `weights::weight_no_short_detours` | 141,088 | **45.5 MB** | **1.10%** | visible, but not first priority |
| `route::is_back_on_road_within_distance` | 49,956 | **44.5 MB** | **1.07%** | same backward-scan pattern shows up in allocs |
| `weights::weight_no_sharp_turns` | 141,089 | **44.1 MB** | **1.06%** | secondary route-logic allocator |
| `graph::get_point_from_tiles` | 329,586 | **11.2 MB** | **0.27%** | no longer a major allocation problem |
| `tile_manager::get_rules_for_point_if_loaded` | 3,015 | **526.1 KB** | **0.01%** | effectively out of the main allocation story |

## Comparison with the previous saved report in this repo

I compared the new measurements with the `./perf.md` content that existed before this overwrite.

| Metric | Previous saved report | Current run | Change |
|---|---:|---:|---:|
| Route generation wall time | **114.22 s** | **46.77 s** | **-59%** |
| Final Hotpath summary uptime | **115.79 s** | **50.57 s** | **-56%** |
| Total cumulative allocation | **40.4 GB** | **4.1 GB** | **-90%** |
| `graph::get_adjacent` total time | **329.08 s** | **1.48 s** | **~ -99.6%** |
| `graph::get_adjacent` alloc | **4.6 GB** | **46.5 MB** | **~ -99.0%** |
| `graph::get_point_from_tiles` total time | **40.66 s** | **2.33 s** | **~ -94%** |
| `graph::get_point_from_tiles` alloc | **53.9 MB** | **11.2 MB** | **~ -79%** |
| `tile_manager::get_rules_for_point_if_loaded` alloc | **2.2 MB** | **526.1 KB** | **~ -76%** |

That is a very large improvement. The old graph hydration / adjacency story has mostly been removed as the first-order bottleneck.

## Main findings

1. **This route is dramatically faster than the previously saved baseline.**
   - The profiled routing phase dropped from about **114 s** to about **47 s**.
   - End-to-end profiled completion dropped to about **50 s**.
   - A second run landed in the same band.

2. **The main bottleneck is now route-search branching, not raw graph hydration.**
   - The dominant cluster is now:
     - `navigator::generate_routes_with_context`
     - `walker::move_forward_to_next_fork_with_context`
     - `walker::get_fork_segments_for_segment_with_context`
     - `walker::get_roundabout_exits_with_context`
     - `walker::move_backwards_to_prev_fork_with_context`
   - That points to search breadth and repeated traversal work as the next target.

3. **Adjacency and point/rules loading are no longer the main problem.**
   - `graph::get_adjacent` is down to **1.48 s** and **46.5 MB** cumulative alloc.
   - `graph::get_point_from_tiles` is down to **2.33 s** and **11.2 MB** alloc.
   - `tile_manager::get_rules_for_point_if_loaded` is effectively negligible.

4. **Tag access is still visible inside the hot traversal loop.**
   - `graph::get_tag_value` and `tile_manager::get_tag_value_if_loaded` still add up to several seconds.
   - They are probably not the first thing to optimize in isolation, but they are now part of the top traversal stack rather than a separate hydration bottleneck.

5. **Roundabout and backward-scan logic still deserve attention.**
   - `walker::get_roundabout_exits_with_context`: **4.09 s**, **667.1 MB**
   - `route::is_back_on_road_within_distance`: **2.39 s**, **44.5 MB**
   - `walker::move_backwards_to_prev_fork_with_context`: **2.29 s**, **317.6 MB**
   - These are not the top item, but they are too large to ignore.

6. **Profiling overhead is still real.**
   - Early in the run, the Hotpath helper thread `hp-functions` itself was consuming a noticeable amount of CPU.
   - Treat this as a hotspot map first, not a clean production benchmark.

7. **RSS still looks surprisingly high relative to live alloc/dealloc diff.**
   - At the live MCP sample, Hotpath reported **7.2 GB RSS** but only about **95.9 MB** live alloc/dealloc diff.
   - That mismatch suggests resident memory is being driven by something broader than simple outstanding allocator delta: likely mapped tile data, retained structures, or profiler-related effects.
   - If memory becomes a primary concern, it deserves a separate memory-focused investigation.

## Recommended next perf targets

Ordered by likely payoff:

1. **Reduce branch fan-out in the walker/navigator path.**
   - Focus on `move_forward_to_next_fork_with_context`, `get_fork_segments_for_segment_with_context`, and `get_roundabout_exits_with_context`.
   - The current profile says the router is still spending most of its budget expanding and evaluating choices.

2. **Instrument branch width and rejection reasons.**
   - Add counters around fork expansion and roundabout exit generation.
   - Good questions: how many candidates are created, how many are rejected later, and how often the same structural work repeats.

3. **Review repeated backward safety checks.**
   - `route::is_back_on_road_within_distance` and `move_backwards_to_prev_fork_with_context` are still a meaningful cost center.
   - If these checks can be memoized, bounded earlier, or made cheaper per branch, that should help.

4. **Only after traversal pruning, look at the remaining weight functions.**
   - `weight_heading` and `weight_no_short_detours` are the largest route-logic buckets.
   - They matter, but the profile says they are still downstream of the bigger traversal/search problem.

5. **Do not spend the next optimization cycle on `get_point_from_tiles` / `get_rules_for_point_if_loaded`.**
   - The current profile shows those changes already paid off.
   - They are no longer where the biggest gains are.

## Bottom line

For `riga,latvia -> sigulda,latvia`, the current profiled build now completes reliably in about **50 seconds** end-to-end under Hotpath, with the actual route-generation phase at about **47 seconds**. That is a major improvement over the previously saved **114-second** profile.

The old adjacency / point hydration bottleneck has largely been solved.

The next meaningful speedup is most likely in **branch-heavy search behavior**: forward traversal, fork discovery, roundabout exit handling, backward checks, and the top-level navigator loop.