# Route generation performance report

## Current profiling command

```bash
RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia fast
HOTPATH_ALLOC_SELF=true RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia fast
```

Hotpath runtime from the current runs:

- normal profiling run: **4.75s**
- alloc-self run: **4.36s**

Note: shell wall-clock for the first run can be much higher if Cargo rebuilds first. The numbers above are the in-process Hotpath timings.

## Current output

- Route file: `map-data/routes/001-67km.gpx`
- Length: **67.11 km**
- Junctions: **237**
- Route points written: **1287**
- Preset: `fast`

## Current overall status

Route generation is now in a good place overall.

The route search still dominates runtime, but the profile is now much clearer and more focused:

- the main CPU hotspot is now `weights::weight_check_distance_to_next`
- the main structural cost is repeated point + adjacency + rule hydration
- the main allocation hotspot is `tile_manager::get_rules_for_point`

## Biggest current times

Top timings from the current normal profiled run:

| Function | Total | Why it matters |
|---|---:|---|
| `graph::get_point_from_tiles` | **4.27s** | dominant inclusive point hydration cost |
| `generator.navigate_itineraries` | **4.01s** | route search still dominates end-to-end runtime |
| `navigator::generate_routes_with_context` | **4.01s** | same search loop at the API boundary |
| `weights::weight_check_distance_to_next` | **3.07s** | clearest algorithmic CPU hotspot |
| `tile_manager::get_adjacent_by_id` | **2.29s** | repeated adjacency lookup remains expensive |
| `tile_manager::get_point_by_id` | **1.58s** | low-level point lookup still costs a lot due to call volume |
| `tile_manager::get_rules_for_point` | **374.10 ms** | smaller CPU cost than the items above, but still very allocation-heavy |
| `generator.score_routes` | **307.17 ms** | now large enough to notice, but not the first target |
| `route::calc_stats` | **306.44 ms** | visible, but not urgent |
| `weights::weight_no_short_detours` | **190.48 ms** | secondary algorithmic hotspot |
| `route::is_back_on_road_within_distance` | **179.22 ms** | related backward-scan work |
| `weights::weight_heading` | **162.64 ms** | not dominant, but part of the remaining routing cost |
| `tile_manager::ensure_tile_loaded` | **320.14 ms** | individually cheap, expensive in aggregate |
| `tile_manager::mark_tile_recent` | **126.08 ms** | LRU bookkeeping overhead from very high call count |

## Biggest current allocation hotspots

Exclusive allocation pass (`HOTPATH_ALLOC_SELF=true`):

| Function | Total alloc | Why it matters |
|---|---:|---|
| `tile_manager::get_rules_for_point` | **3.6 GB** | by far the biggest remaining allocator |
| `tile_manager::get_adjacent_by_id` | **28.5 MB** | repeated adjacency Vec creation |
| `weights::weight_check_distance_to_next` | **8.8 MB** | meaningful remaining algorithmic allocation cost |
| `graph::get_point_from_tiles` | **6.0 MB** | most of its remaining alloc is nested under rule hydration |
| `tile_manager::adjacent_line_tags` | **5.8 MB** | small per-call, large aggregate |
| `gpx_writer::build_gpx` | **1.5 MB** | not important overall |
| `routing_api::open` | **1.4 MB** | startup cost is modest |
| `generator.score_routes` | **1.1 MB** | visible but not a top problem |

Cumulative allocation from the normal run:

- total cumulative alloc recorded by Hotpath: **3.7 GB**
- biggest cumulative allocator: `tile_manager::get_rules_for_point` at **3.6 GB**
- `weights::weight_check_distance_to_next` also drives substantial cumulative work because it sits on top of repeated point/rule access

## Biggest current areas for improvement

### 1. `weights::weight_check_distance_to_next`

**Priority: highest**

This is the clearest current CPU hotspot.

What to look for:

- route slicing or route cloning
- repeated backward scans over route history
- repeated point/line/tag lookups inside the same check
- opportunities to switch to index-based or cached route state

Expected payoff: **very high**.

### 2. `tile_manager::get_rules_for_point`

**Priority: highest**

This is the dominant allocation problem.

The current profile says rule hydration is still too expensive and too repetitive.

Good next steps:

- cache decoded rules per point
- avoid rebuilding owned structures on every lookup
- separate cheap metadata access from full rule decoding
- return borrowed or memoized data where possible

Expected payoff: **very high**, especially for memory pressure.

### 3. `graph::get_point_from_tiles`

**Priority: highest**

This remains the biggest inclusive structural cost.

It likely does too much for common hot-path callers.

Good next steps:

- split lightweight point access from full hydration
- keep adjacency and rules lazy
- avoid constructing data the caller does not need

Expected payoff: **very high**.

### 4. `tile_manager::get_adjacent_by_id`

**Priority: high**

This is still expensive in both time and allocation.

Good next steps:

- cache adjacency by point id
- avoid allocating a new Vec on every lookup
- consider borrowed slices or memoized adjacency records

Expected payoff: **high**.

### 5. `route::is_back_on_road_within_distance` and `weights::weight_no_short_detours`

**Priority: medium-high**

These are now the next clear route-logic hotspots after `weight_check_distance_to_next`.

Good next steps:

- reduce backward rescans
- reuse incremental route state
- avoid repeated metadata lookups for already-seen route segments

Expected payoff: **moderate to high**.

### 6. Tile-cache churn: `ensure_tile_loaded` and `mark_tile_recent`

**Priority: medium**

These are cheap per call, but very frequent.

Good next steps:

- reduce repeated same-tile lookups
- avoid recency updates for every tiny read if possible
- batch related reads where practical

Expected payoff: **moderate** after the bigger structural fixes.

## Areas that are not the problem right now

These are visible, but they are not where the next big wins are:

- `routing_api::open`: ~**292 ms**
- `generator.score_routes`: ~**307 ms**
- `route::calc_stats`: ~**306 ms**
- clustering: ~**37 ms**
- GPX writing: ~**9 ms**

Do not optimize these first.

## Recommended next optimization order

1. `weights::weight_check_distance_to_next`
2. `tile_manager::get_rules_for_point`
3. `graph::get_point_from_tiles`
4. `tile_manager::get_adjacent_by_id`
5. `route::is_back_on_road_within_distance`
6. tile-cache churn (`ensure_tile_loaded`, `mark_tile_recent`)

## Bottom line

Current route generation is about **4.75s** in the profiled run.

The remaining performance story is now simple:

- **CPU:** `weights::weight_check_distance_to_next`
- **allocations:** `tile_manager::get_rules_for_point`
- **structural lookup cost:** `graph::get_point_from_tiles` and `tile_manager::get_adjacent_by_id`
