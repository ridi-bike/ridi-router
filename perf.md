# Route generation performance report

## What I changed

- Added `hotpath` to the workspace and wired these crate features:
  - `hotpath`
  - `hotpath-alloc`
  - `hotpath-mcp`
- Instrumented the CLI and routing code with `#[hotpath::main]`, `#[hotpath::measure_all]`, `#[hotpath::measure]`, and a few phase-level `measure_block!` spans.
- Added project-local MCP config at `.pi/mcp.json` for Hotpath:
  - `http://localhost:6771/mcp`
- Updated `dev.sh` so profiling features can be injected via:
  - `RIDI_ROUTER_CLI_FEATURES=...`

## How I profiled it

The repo script currently requires the `route` subcommand, so I ran the supported equivalent of the requested route command:

```bash
RIDI_ROUTER_CLI_FEATURES='hotpath,hotpath-alloc,hotpath-mcp' \
./dev.sh route riga,latvia sigulda,latvia
```

I also ran an allocation-focused pass with exclusive allocation accounting:

```bash
HOTPATH_ALLOC_SELF=true \
RIDI_ROUTER_CLI_FEATURES='hotpath,hotpath-alloc,hotpath-mcp' \
./dev.sh route riga,latvia sigulda,latvia
```

Output route:

- `map-data/routes/001-67km.gpx`
- Length: `67.15 km`
- Junctions: `245`
- Route points written: `1382`

## High-level result

End-to-end profiled runtime for route generation was about **49.1s**.

The runtime is dominated by the actual navigation/search loop, not by startup, clustering, or GPX writing.

## Main timing findings

### Top phases

- `generator.navigate_itineraries`: **48.43s**
- `navigator::generate_routes_with_context`: **48.43s**
- `routing_api::open`: **0.37s**
- `generator.score_routes`: **0.31s**
- `clustering::generate`: **0.04s**
- `gpx_writer::write_gpx`: **0.01s**

So almost all time is spent inside the route search itself.

### Biggest timing hotspots

Inclusive timing from the second run:

| Function | Total | Notes |
|---|---:|---|
| `graph::get_point_from_tiles` | 44.08s | Dominant repeated point hydration |
| `weights::weight_no_loops` | 41.76s | Calls `route::has_looped` |
| `route::has_looped` | 41.76s | Biggest algorithmic hotspot |
| `tile_manager::get_adjacent_by_id` | 23.43s | Rebuilds adjacency repeatedly |
| `tile_manager::get_point_by_id` | 16.60s | Repeated point lookup |
| `tile_manager::ensure_tile_loaded` | 5.43s | Fast per-call, huge call count |
| `weights::weight_check_distance_to_next` | 4.62s | Secondary but still significant |
| `tile_manager::get_rules_for_point` | 3.48s | Also the main allocation hotspot |
| `tile_manager::mark_tile_recent` | 2.20s | LRU bookkeeping overhead |
| `graph::get_tag_value` / `graph::get_line_from_tiles` / `graph::get_tag_set` | ~1.1-1.4s each | Repeated metadata hydration |

Important: these are **inclusive** Hotpath timings, so they overlap. The real story is:

1. `route::has_looped` is too expensive.
2. It drives massive repeated `ctx.point()` / point hydration work.
3. That cascades into repeated adjacency, rules, tag, and tile-cache lookups.

## Main allocation findings

Exclusive allocation pass (`HOTPATH_ALLOC_SELF=true`):

| Function | Total alloc | Notes |
|---|---:|---|
| `tile_manager::get_rules_for_point` | **32.1 GB** | By far the biggest allocator |
| `tile_manager::get_adjacent_by_id` | 298.6 MB | Repeated adjacency Vec creation |
| `graph::get_point_from_tiles` | 61.8 MB | Small exclusive cost; most alloc is nested |
| `weights::weight_check_distance_to_next` | 36.9 MB | Route-slice cloning / helper work |
| `tile_manager::get_tag_value` | 24.5 MB | String materialization |
| `tile_manager::adjacent_line_tags` | 5.8 MB | Small but frequent |
| `gpx_writer::build_gpx` | 1.5 MB | Not important overall |

This is the clearest memory conclusion:

> **The biggest allocation problem is repeated rule hydration in `tile_manager::get_rules_for_point`, triggered indirectly by repeated point hydration.**

## Interpretation

### 1) `route::has_looped` is the main CPU bottleneck

`weight_no_loops` and `route::has_looped` account for ~**41.8s** of a ~49s run.

That strongly suggests the loop-detection logic is effectively rescanning too much route state on every decision. It also repeatedly calls into:

- `ctx.point(...)`
- `ctx.line(...)`
- `ctx.tag_set(...)`
- `ctx.tag_value(...)`

That compounds the cost badly.

### 2) `ctx.point()` is too expensive for how often it is used

`graph::get_point_from_tiles` is called about **1.67 million** times.

Today, a point lookup hydrates a lot:

- point record
- adjacent lines
- rules
- multiple owned vectors

That is far too heavy for a hot-path primitive.

### 3) Rules are being decoded far too often

`tile_manager::get_rules_for_point` allocates **32.1 GB exclusive** during a single route generation.

That is the biggest actionable memory issue in the profile.

### 4) `weight_check_distance_to_next` is the second meaningful algorithmic hotspot

At **4.62s**, it is not the top issue, but it is large enough to matter after fixing loop detection.

The allocation data also suggests it is doing avoidable route-structure copying/work.

### 5) Startup, clustering, and output are not the problem

- open: ~0.37s
- clustering: ~0.04s
- GPX output: ~0.01s

Do not optimize these first.

## Ranked improvement suggestions

### 1. Rewrite `Route::has_looped` to use incremental state

**Priority: highest**

Current behavior looks like repeated route rescanning.

Better options:

- Track `last_seen_segment_idx` per point id
- Track `last_seen_segment_idx` per road identity (`hw_ref` / name / maybe line id)
- Track cumulative route distance so loop checks can use index + distance arithmetic
- Keep this state in `Walker` or `Route` as the route grows

Goal: turn loop detection from "scan most of the route again" into near O(1) or amortized O(1) per step.

Expected impact: **very large**.

### 2. Split `ctx.point()` / `graph::get_point_from_tiles()` into lightweight vs full hydration APIs

**Priority: highest**

A lot of call sites only need:

- lat/lon
- point id
- junction flag
- residential/nogo flags

They do **not** need adjacency and fully decoded rules every time.

Suggested direction:

- Add a lightweight point accessor for basic point metadata
- Add separate lazy accessors for adjacency and rules
- Avoid building owned `Vec`s unless the caller actually needs them

Expected impact: **very large** on both CPU and allocs.

### 3. Cache decoded point rules and adjacency by `(tile_id, osm_id)`

**Priority: highest**

The same points are clearly being revisited over and over.

Cache candidates:

- decoded rules
- adjacency lists
- maybe fully decoded point metadata

Even a simple in-process memoization layer in `MapDataGraph` or `TileManager` would likely help a lot.

Expected impact: **very large**, especially because `get_rules_for_point` dominates exclusive allocs.

### 4. Stop doing release-time work in `debug_assert_registry_invariants`

**Priority: high, very low risk**

This function still costs about **0.51s** total.

It should probably be compiled out almost entirely in release.

Suggested fix:

- guard the whole body with `#[cfg(debug_assertions)]`
- or make it an empty inline function in release builds

This is an easy win.

### 5. Avoid allocating `String` in hot tag lookups

**Priority: high**

`tile_manager::get_tag_value` still allocates ~**24.5 MB** exclusive.

Prefer one of:

- compare tag indices directly
- return borrowed values instead of owned `String`
- intern tag values once per tile and compare interned ids

Expected impact: moderate, and it also reduces pressure on the allocator.

### 6. Remove route cloning/slicing in `weight_check_distance_to_next`

**Priority: medium-high**

This function allocates **36.9 MB exclusive**.

The likely culprit is route slicing/cloning helpers like `split_at_point(...)`.

Prefer index-based or slice-based helpers that borrow route data instead of cloning vectors.

Expected impact: moderate.

### 7. Rework `is_back_on_road_within_distance` similarly

**Priority: medium**

This is another backward scan over route history with repeated metadata lookups.

It should likely use maintained route state instead of rescanning.

Expected impact: moderate.

### 8. Reduce `ensure_tile_loaded` / `mark_tile_recent` churn

**Priority: medium**

They are individually cheap but called millions of times.

Potential ideas:

- fast path for same-tile repeated lookups
- avoid LRU recency updates for every tiny read in a tight loop
- batch related tile reads where possible

Expected impact: moderate after the bigger structural fixes.

## Concrete code areas worth changing first

Start here:

1. `crates/ridi-router-routing/src/router/route/mod.rs`
   - `Route::has_looped`
   - helpers used by `weight_check_distance_to_next`

2. `crates/ridi-router-routing/src/map_data/graph.rs`
   - `get_point_from_tiles`
   - split lightweight vs full point hydration

3. `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
   - `get_rules_for_point`
   - `get_adjacent_by_id`
   - `debug_assert_registry_invariants`
   - `get_tag_value`

4. `crates/ridi-router-routing/src/router/weights.rs`
   - `weight_no_loops`
   - `weight_check_distance_to_next`
   - `weight_no_short_detours`

## Suggested next profiling loop

After the first round of fixes, rerun the same command and check whether:

- `route::has_looped` drops dramatically
- `graph::get_point_from_tiles` call cost drops
- `tile_manager::get_rules_for_point` allocs collapse
- `weight_check_distance_to_next` becomes the next dominant hotspot

That would confirm the biggest structural problem is solved.

## MCP note

Project-local Hotpath MCP config is in `.pi/mcp.json`.

The current Pi session did not hot-reload the new MCP server list, so using the MCP tools from this same session still requires a Pi restart/reload. The profiled binary was run with `hotpath-mcp` enabled, so the server endpoint is ready when the app runs under a reloaded Pi session.
