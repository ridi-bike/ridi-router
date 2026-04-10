# Route generation performance review

_Date:_ 2026-04-09

## Command I ran

Your prompt used `RIDI_FEATURES ./dev.sh riga,latvia sigulda,latvia`, but this repo's `dev.sh` expects the `route` subcommand and the perf feature value.

So I ran the repo-valid perf invocation:

```bash
RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia
```

Why this is the right equivalent:
- `dev.sh` requires `route`
- `RIDI_FEATURES=perf` expands to `hotpath,hotpath-alloc,hotpath-mcp`
- that enables live Hotpath MCP access during the run

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

## Run result

- Cargo reuse/build overhead before routing: **0.42 s**
- Route generation started: **2026-04-09 19:06:04.887Z**
- Router logged `Routes from itineraries` at **45 s** with:
  - `itinerary_count=109`
  - `routes_count=79`
- Router logged `Route generation finished` at **2026-04-09 19:06:55.868Z**
- Reported route-generation duration: **50 s**
- Final Hotpath summary printed at **53.90 s** uptime
- Process exit code: **0**
- Output written successfully to `map-data/routes`
- Final route files produced: **23 GPX files**
- Output size: **4.42 MB total**
- Output route filename lengths ranged from **81 km** to **261 km**

## Live Hotpath MCP snapshot while the run was active

I connected to Hotpath during the active route-generation phase.

### Snapshot A: early thread/runtime state (~10.9 s elapsed)

Profiler/runtime status:
- profiler uptime when queried: **7.57 s**
- thread snapshot elapsed: **10.92 s**
- thread count: **19**
- RSS reported by Hotpath: **8.3 GB**
- cumulative alloc/dealloc at that point: **3.3 GB alloc / 3.2 GB dealloc**
- live alloc/dealloc diff: **100.9 MB**
- eight worker threads were already busy, mostly around **79.6% to 91.6% CPU** each
- Hotpath helper thread `hp-functions` was also using about **59.7% CPU**

### Snapshot B: early timing hotspots (~13.6 s elapsed)

| Function | Total | Why it stood out early |
|---|---:|---|
| `walker::move_forward_to_next_fork_with_context` | **11.99 s** | still the dominant traversal hotspot immediately |
| `walker::classify_fork_segments_for_segment_with_context` | **6.88 s** | fork classification is already first-order work |
| `navigator::generate_routes_with_context` | **6.41 s** | top-level route search remains expensive |
| `walker::get_roundabout_exits_with_context` | **5.72 s** | roundabout expansion shows up very early |
| `walker::get_fork_segments_for_segment_with_context` | **4.25 s** | branch discovery remains core cost |
| `graph::get_tag_value` | **3.98 s** | tag reads still accumulate inside the hot traversal loop |

### Snapshot C: early allocation hotspots (~16.1 s elapsed)

| Function | Total alloc | Why it stood out early |
|---|---:|---|
| `walker::move_forward_to_next_fork_with_context` | **650.0 MB** | biggest early allocator |
| `walker::get_roundabout_exits_with_context` | **482.6 MB** | roundabout exploration allocates heavily |
| `walker::classify_fork_segments_for_segment_with_context` | **327.6 MB** | classification now has clear allocation cost too |
| `walker::get_fork_segments_for_segment_with_context` | **251.8 MB** | branch-expansion churn remains large |
| `navigator::generate_routes_with_context` | **243.6 MB** | search orchestration still allocates a lot |
| `walker::move_backwards_to_prev_fork_with_context` | **234.4 MB** | backward exploration is also costly |

## Final Hotpath timing snapshot at completion

This table is from the final Hotpath summary printed by the profiled run.

Note: Hotpath totals are cumulative across calls, so use them mainly for hotspot ranking.

| Function | Calls | Total | % Total | Why it matters |
|---|---:|---:|---:|---|
| `walker::move_forward_to_next_fork_with_context` | 321,473 | **16.56 s** | **30.72%** | biggest direct traversal hotspot |
| `walker::classify_fork_segments_for_segment_with_context` | 3,486,721 | **9.30 s** | **17.25%** | fork classification is now a first-order cost center |
| `navigator::generate_routes_with_context` | 13 | **8.23 s** | **15.26%** | top-level search is still large, but no longer the single dominant bucket |
| `walker::get_roundabout_exits_with_context` | 96,531 | **7.65 s** | **14.19%** | roundabout branch handling is very expensive |
| `graph::get_tag_value` | 4,695,641 | **6.49 s** | **12.05%** | repeated tag reads still add up materially |
| `walker::get_fork_segments_for_segment_with_context` | 2,785,483 | **5.69 s** | **10.56%** | branch discovery remains core work |
| `tile_manager::loaded_tile` | 3,626,024 | **5.26 s** | **9.76%** | low-level tile access is still busy |
| `tile_manager::get_tag_value_if_loaded` | 2,046,829 | **4.60 s** | **8.53%** | loaded-path tag reads are still significant |
| `walker::move_backwards_to_prev_fork_with_context` | 117,561 | **4.23 s** | **7.86%** | backward traversal remains too expensive |
| `weights::weight_no_short_detours` | 127,106 | **3.67 s** | **6.81%** | biggest route-logic timing cost in this run |
| `route::is_back_on_road_within_distance` | 54,138 | **3.43 s** | **6.36%** | backward safety scanning is still noticeable |
| `weights::weight_heading` | 127,105 | **3.05 s** | **5.65%** | heading logic remains a real cost |
| `graph::get_adjacent` | 306,492 | **1.56 s** | **2.90%** | still visible, but clearly no longer the main problem |

## Final Hotpath allocation snapshot at completion

Total cumulative allocation reported by Hotpath: **3.6 GB**

| Function | Calls | Total alloc | % Total | Why it matters |
|---|---:|---:|---:|---|
| `walker::move_forward_to_next_fork_with_context` | 321,473 | **873.9 MB** | **23.49%** | biggest remaining allocator |
| `walker::get_roundabout_exits_with_context` | 96,531 | **625.9 MB** | **16.82%** | roundabout exploration is still a major memory cost |
| `walker::classify_fork_segments_for_segment_with_context` | 3,486,721 | **440.3 MB** | **11.83%** | classification now has substantial allocation churn |
| `walker::get_fork_segments_for_segment_with_context` | 2,785,483 | **324.9 MB** | **8.73%** | branch expansion still allocates heavily |
| `walker::move_backwards_to_prev_fork_with_context` | 117,561 | **307.8 MB** | **8.27%** | backward traversal is also a substantial allocator |
| `navigator::generate_routes_with_context` | 13 | **304.9 MB** | **8.19%** | overall search orchestration still allocates a lot |
| `weights::weight_heading` | 127,105 | **161.2 MB** | **4.33%** | biggest route-logic allocator |
| `weights::weight_no_short_detours` | 127,106 | **61.1 MB** | **1.64%** | visible route-logic allocation cost |
| `route::is_back_on_road_within_distance` | 54,138 | **60.2 MB** | **1.62%** | backward-scan pattern shows up in allocs too |
| `graph::get_adjacent` | 306,492 | **44.2 MB** | **1.19%** | adjacency is now far below the walker cluster |

## Comparison with the previous saved report in this repo

I compared this run against the `./perf.md` content that existed before this overwrite.

| Metric | Previous saved report | Current run | Change |
|---|---:|---:|---:|
| Route generation wall time | **46.77 s** | **50.00 s** | **+6.9%** |
| Final Hotpath summary uptime | **50.57 s** | **53.90 s** | **+6.6%** |
| Total cumulative allocation | **4.1 GB** | **3.6 GB** | **-12.2%** |
| `navigator::generate_routes_with_context` total time | **14.34 s** | **8.23 s** | **-42.6%** |
| `walker::move_forward_to_next_fork_with_context` total time | **13.70 s** | **16.56 s** | **+20.9%** |
| `walker::get_roundabout_exits_with_context` total time | **4.09 s** | **7.65 s** | **+87.0%** |
| `walker::get_fork_segments_for_segment_with_context` total time | **6.23 s** | **5.69 s** | **-8.7%** |
| `graph::get_tag_value` total time | **5.28 s** | **6.49 s** | **+22.9%** |
| `weights::weight_heading` total time | **2.86 s** | **3.05 s** | **+6.6%** |
| `graph::get_adjacent` total time | **1.48 s** | **1.56 s** | **+5.4%** |
| `navigator::generate_routes_with_context` alloc | **763.0 MB** | **304.9 MB** | **-60.0%** |
| `walker::get_fork_segments_for_segment_with_context` alloc | **598.4 MB** | **324.9 MB** | **-45.7%** |
| `walker::move_forward_to_next_fork_with_context` alloc | **940.0 MB** | **873.9 MB** | **-7.0%** |
| `weights::weight_heading` alloc | **179.9 MB** | **161.2 MB** | **-10.4%** |
| `graph::get_adjacent` alloc | **46.5 MB** | **44.2 MB** | **-4.9%** |

Two especially important takeaways from that comparison:
- **memory/allocation got better again**
- **wall time got a bit worse because cost shifted into the walker/roundabout/classification path**

## Main findings

1. **This run is slightly slower than the previous saved report, even though cumulative allocation improved.**
   - Route generation moved from about **46.8 s** to **50.0 s**.
   - Hotpath total uptime moved from about **50.6 s** to **53.9 s**.
   - But total cumulative allocation improved from **4.1 GB** to **3.6 GB**.

2. **The main bottleneck is still branch-heavy route search, and the classification layer is now clearly part of that bottleneck.**
   - The dominant cluster is now:
     - `move_forward_to_next_fork_with_context`
     - `classify_fork_segments_for_segment_with_context`
     - `get_roundabout_exits_with_context`
     - `get_fork_segments_for_segment_with_context`
     - `move_backwards_to_prev_fork_with_context`
   - This is no longer a graph-hydration story.

3. **Top-level search orchestration got cheaper, but the lower-level walker work got worse.**
   - `navigator::generate_routes_with_context` time dropped by about **43%**.
   - That win was more than offset by slower forward traversal, slower roundabout handling, and higher tag-read cost inside the traversal loop.

4. **Roundabout and backward-scan behavior look like the sharpest runtime pain points.**
   - `get_roundabout_exits_with_context`: **7.65 s**, **625.9 MB**
   - `move_backwards_to_prev_fork_with_context`: **4.23 s**, **307.8 MB**
   - `is_back_on_road_within_distance`: **3.43 s**, **60.2 MB**
   - These are too large to treat as secondary details.

5. **Tag access is still meaningfully expensive, but mainly because it sits inside the hot walker loop.**
   - `graph::get_tag_value`: **6.49 s**
   - `tile_manager::get_tag_value_if_loaded`: **4.60 s**
   - The right question is probably not “make tag lookup faster in isolation”, but “how often can the walker avoid repeating the same lookups?”

6. **Adjacency is still under control.**
   - `graph::get_adjacent` is only **1.56 s** and **44.2 MB** cumulative alloc.
   - That confirms the earlier adjacency work is still paying off.

7. **The live Hotpath thread snapshot still showed very high RSS relative to allocator delta.**
   - At the live sample, Hotpath reported **8.3 GB RSS** but only about **100.9 MB** live alloc/dealloc diff.
   - That again suggests resident memory is driven by something broader than simple outstanding heap allocations: likely mapped tile data, retained structures, and profiler overhead.

8. **Comparison quality is reasonably good because the route search breadth signals matched the previous saved report.**
   - This run also produced `itinerary_count=109`, `routes_count=79`, and **23 GPX files**.
   - That makes the regression look more like **per-branch work getting slower** than a large change in search breadth.

## Recommended next perf targets

Ordered by likely payoff:

1. **Cut repeated fork classification work in the walker path.**
   - `classify_fork_segments_for_segment_with_context` is now too large to ignore.
   - First question: can classification results be reused, cached, or computed from cheaper pre-fetched data?

2. **Revisit roundabout exit generation.**
   - `get_roundabout_exits_with_context` is large in both time and allocation.
   - This looks like one of the cleanest places for a meaningful win.

3. **Reduce repeated backward safety scans.**
   - `move_backwards_to_prev_fork_with_context` and `is_back_on_road_within_distance` are still expensive enough to justify focused work.
   - If they can be memoized, bounded earlier, or skipped more often, that should help.

4. **After branch pruning, reduce repeated tag reads inside traversal.**
   - The tag costs are now real enough to matter.
   - But they still look downstream of the larger branch/classification problem.

5. **Do not spend the next optimization cycle on adjacency plumbing.**
   - The current profile still says adjacency is not the first-order bottleneck anymore.

## Bottom line

For `riga,latvia -> sigulda,latvia`, the current profiled build completes successfully in about **53.9 seconds** end-to-end under Hotpath, with the actual route-generation phase reported at **50 seconds**.

Compared with the previously saved report in this repo, this build is **slightly slower in wall time** but **better on cumulative allocation**.

The old adjacency bottleneck is still not the main story.

The next meaningful speedup is most likely in the **walker branch-expansion stack**: forward traversal, fork classification, roundabout exit handling, backward checks, and the repeated tag lookups embedded in that path.
