# Route generation performance report

## Requested command status

Requested command:

```bash
RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia fsst
```

Current result:

- this fails immediately because preset `fsst` does not exist
- expected preset file: `rule-examples/rules-fsst.json`
- available presets: `avoid-unpaved`, `default`, `empty`, `fast`, `prefer-unpaved`

Because of that, the detailed numbers below are from the nearest valid comparable run with preset `fast`.

## Profiling commands actually run

```bash
RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia fast
HOTPATH_ALLOC_SELF=true RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia fast
```

Hotpath runtime from the current runs:

- normal profiling run: **1.55s**
- alloc-self run: **1.60s**
- shell wall-clock: **2.14s** normal, **2.11s** alloc-self

Compared with the previous report in this file:

- normal Hotpath runtime improved from **4.75s** to **1.55s** (**67.4% faster**)
- alloc-self Hotpath runtime improved from **4.36s** to **1.60s** (**63.3% faster**)

## Current output

- Route file: `map-data/routes/001-67km.gpx`
- Length: **67.11 km**
- Junctions: **238**
- Route points written: **1289**
- Preset used for the successful profiled run: `fast`

## Current overall status

The good news is that route generation is now much faster.

The old report said the main CPU problem was `weights::weight_check_distance_to_next`. That is no longer true for this route. It is now only **12.17 ms** total in the normal run.

The performance story has shifted:

- **main inclusive CPU cost:** `graph::get_point_from_tiles`
- **main lookup costs:** `tile_manager::get_adjacent_by_id` and `tile_manager::get_point_by_id`
- **main allocation problem:** `tile_manager::get_rules_for_point`
- **next route-logic hotspots:** `weights::weight_no_short_detours` and `route::is_back_on_road_within_distance`

So this route is in a much better place overall, but the remaining work is now mostly about point hydration, adjacency lookup, and rule decoding/allocation.

## Biggest current times

Top timings from the current normal profiled run:

| Function | Total | Why it matters |
|---|---:|---|
| `graph::get_point_from_tiles` | **1.10 s** | biggest inclusive structural cost; common access path still does too much work |
| `generator.navigate_itineraries` | **858.92 ms** | route search still dominates the end-to-end routing section |
| `navigator::generate_routes_with_context` | **858.45 ms** | same route-search loop at the API boundary |
| `tile_manager::get_adjacent_by_id` | **629.57 ms** | repeated adjacency lookup remains a major cost |
| `tile_manager::get_point_by_id` | **403.24 ms** | low-level point lookup is still expensive because it is called so often |
| `routing_api::open` | **291.14 ms** | fixed startup cost; visible now because routing got much faster |
| `generator.score_routes` | **268.65 ms** | now large enough to notice after the routing core speedup |
| `route::calc_stats` | **267.99 ms** | similar story: more visible now that the main route loop is faster |
| `weights::weight_no_short_detours` | **186.83 ms** | clearest remaining route-logic hotspot |
| `route::is_back_on_road_within_distance` | **176.23 ms** | backward-scan route logic is still expensive |
| `weights::weight_heading` | **148.81 ms** | meaningful route-logic cost, but below the items above |
| `tile_manager::ensure_tile_loaded` | **111.20 ms** | cheap per call, expensive in aggregate due to very high call count |
| `tile_manager::get_rules_for_point` | **84.57 ms** | not a top CPU cost anymore, but still the dominant allocator |
| `tile_manager::mark_tile_recent` | **39.97 ms** | tile-cache bookkeeping still adds up |
| `weights::weight_check_avoid_rules` | **39.78 ms** | now a visible but secondary route-weight cost |
| `weights::weight_check_distance_to_next` | **12.17 ms** | formerly dominant; now no longer a first-order problem |

## Perf-plan tracked functions

These are the functions called out in `perf-plan.md` for re-checking:

| Function | Current timing | Current read |
|---|---:|---|
| `weights::weight_check_distance_to_next` | **12.17 ms** | big win; not a priority now |
| `weights::weight_no_loops` | **251.13 µs** | effectively irrelevant on this route |
| `weights::weight_progress_speed` | **5.80 µs** | effectively irrelevant on this route |
| `weights::weight_check_avoid_rules` | **39.78 ms** | visible, but far below the main lookup stack |
| `graph::get_point_from_tiles` | **1.10 s** | now the main inclusive CPU hotspot |
| `tile_manager::get_adjacent_by_id` | **629.57 ms** | still a major target |

Takeaway: the perf-plan targets tied to route-weight logic improved a lot. The biggest remaining time is now in map-data access and hydration rather than in the old distance-check logic.

## Biggest current allocation hotspots

Exclusive allocation pass (`HOTPATH_ALLOC_SELF=true`):

| Function | Total alloc | Why it matters |
|---|---:|---|
| `tile_manager::get_rules_for_point` | **1004.4 MB** | still the dominant allocator by a huge margin |
| `tile_manager::get_adjacent_by_id` | **8.8 MB** | repeated adjacency `Vec` creation still costs real memory |
| `tile_manager::adjacent_line_tags` | **5.8 MB** | small per call, large in aggregate |
| `graph::get_point_from_tiles` | **1.8 MB** | very small self alloc; most of its cost is nested work |
| `walker::move_forward_to_next_fork_with_context` | **1.5 MB** | route-search helper alloc is visible but not dominant |
| `gpx_writer::build_gpx` | **1.5 MB** | output cost is small overall |
| `routing_api::open` | **1.4 MB** | startup cost is modest |
| `generator.score_routes` | **1.1 MB** | visible but not a first target |
| `navigator::generate_routes_with_context` | **533.5 KB** | small self allocation compared with nested costs |

Cumulative allocation from the normal run:

- total cumulative alloc recorded by Hotpath: **1.0 GB**
- `graph::get_point_from_tiles`: **1014.5 MB** cumulative
- `tile_manager::get_rules_for_point`: **1004.5 MB** cumulative
- `generator.navigate_itineraries`: **731.6 MB** cumulative
- `navigator::generate_routes_with_context`: **731.5 MB** cumulative
- `generator.score_routes`: **230.0 MB** cumulative
- `route::calc_stats`: **228.9 MB** cumulative
- `weights::weight_no_short_detours`: **193.9 MB** cumulative
- `route::is_back_on_road_within_distance`: **184.1 MB** cumulative
- `weights::weight_heading`: **103.8 MB** cumulative
- `weights::weight_check_avoid_rules`: **37.0 MB** cumulative
- `weights::weight_check_distance_to_next`: **7.2 MB** cumulative

The key allocator story is still simple: `tile_manager::get_rules_for_point` remains the main memory problem. But compared with the old report, the total cumulative allocation picture is much smaller and the old distance-check path is no longer driving the same kind of cost.

## What changed relative to the previous report

Biggest changes from the older numbers in this file:

- total profiled runtime dropped from about **4.75s** to **1.55s**
- `weights::weight_check_distance_to_next` dropped from **3.07s** to **12.17 ms**
- cumulative allocation dropped from **3.7 GB** to **1.0 GB**
- exclusive allocation in `tile_manager::get_rules_for_point` dropped from **3.6 GB** to about **1.0 GB**
- the bottleneck moved away from route-weight distance checks and toward point/adjacency/rule lookup infrastructure

This is a real shift, not just small noise.

## Biggest current areas for improvement

### 1. `tile_manager::get_rules_for_point`

**Priority: highest**

This is still the main allocation problem.

Good next steps:

- cache decoded rules per point
- avoid rebuilding owned structures on every lookup
- separate cheap metadata access from full rule decoding
- return borrowed or memoized data where possible

Expected payoff: **very high**, especially for memory pressure.

### 2. `graph::get_point_from_tiles`

**Priority: highest**

This is now the biggest inclusive CPU hotspot.

Good next steps:

- split lightweight point access from full hydration
- keep adjacency and rules lazy
- avoid constructing data the caller does not need
- make common hot-path callers ask for less

Expected payoff: **very high**.

### 3. `tile_manager::get_adjacent_by_id`

**Priority: high**

This is still very expensive in both time and allocation.

Good next steps:

- cache adjacency by point id
- avoid allocating a new `Vec` on every lookup
- consider borrowed slices or memoized adjacency records

Expected payoff: **high**.

### 4. `tile_manager::get_point_by_id`, `ensure_tile_loaded`, and `mark_tile_recent`

**Priority: high**

These are part of the same remaining lookup stack.

Good next steps:

- reduce repeated same-point and same-tile lookups
- avoid unnecessary tile recency churn
- batch related reads where practical
- let callers reuse already-fetched point data

Expected payoff: **high** when combined with the hydration fixes above.

### 5. `weights::weight_no_short_detours` and `route::is_back_on_road_within_distance`

**Priority: medium-high**

These are now the main route-logic costs.

Good next steps:

- reduce backward rescans
- reuse incremental route state
- avoid repeated metadata lookups for already-seen route segments

Expected payoff: **moderate to high**.

### 6. `generator.score_routes` and `route::calc_stats`

**Priority: medium**

These are not terrible, but they now stand out more clearly because the main search loop got much faster.

Good next steps:

- avoid recomputing route statistics that are already known during generation
- reuse cached per-route metadata during scoring
- check for duplicate scans over the same route points

Expected payoff: **moderate**.

## Areas that are not the problem right now

These are visible or tracked, but they are not where the next big wins are:

- `weights::weight_check_distance_to_next`: **12.17 ms**
- `weights::weight_no_loops`: **251.13 µs**
- `weights::weight_progress_speed`: **5.80 µs**
- GPX writing: ~**9 ms**

Do not optimize these first.

## Recommended next optimization order

1. `tile_manager::get_rules_for_point`
2. `graph::get_point_from_tiles`
3. `tile_manager::get_adjacent_by_id`
4. `tile_manager::get_point_by_id` plus tile-cache churn (`ensure_tile_loaded`, `mark_tile_recent`)
5. `weights::weight_no_short_detours`
6. `route::is_back_on_road_within_distance`
7. `generator.score_routes` / `route::calc_stats`

## Bottom line

The exact requested `fsst` command cannot be profiled yet because the preset is missing.

For the closest valid route run with preset `fast`, current route generation is about **1.55s** in the profiled run and **1.60s** in alloc-self mode.

That is a major improvement over the previous report.

The remaining performance story is now:

- **CPU / structure:** `graph::get_point_from_tiles`
- **lookup stack:** `tile_manager::get_adjacent_by_id` and `tile_manager::get_point_by_id`
- **allocations:** `tile_manager::get_rules_for_point`
- **next route-logic targets:** `weights::weight_no_short_detours` and `route::is_back_on_road_within_distance`

If we need true numbers for `fsst`, the next step is to either add `rule-examples/rules-fsst.json` or confirm that `fsst` was meant to be `fast`.
