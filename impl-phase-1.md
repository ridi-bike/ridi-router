# Implementation Phase 1: validate and land the `RoutingContext` adjacency cache

## Goal
Cut repeated same-point adjacency lookups inside one route search before touching lock strategy or adjacency plumbing.

## Why this is phase 1
This is the highest-confidence, lowest-risk change in `perf-plan.md`:
- `RoutingContext` already owns task-local caches
- the hot path already goes through `RoutingContext::adjacent(...)`
- the change stays local to one file first, so perf results are easy to interpret

It also gives a clean decision gate for the rest of the plan.

## Scope

### In scope
1. Before changing behavior, add characterization tests for current adjacency semantics:
   - graph-level missing-neighbor adjacency still succeeds and filters the missing edge
   - adjacency is cached within one `RoutingContext`
   - route-generation child contexts do not reuse a parent adjacency cache
2. Add adjacency caching to `RoutingCaches` in `crates/ridi-router-routing/src/routing_context.rs`.
3. Change `RoutingContext::adjacent(...)` to:
   - check cache
   - return cloned cached result on hit
   - call `graph.get_adjacent(...)` on miss
   - store and return the result
4. Extend tests in `routing_context.rs` to prove results stay identical to the uncached path.
5. Run the existing profiled route command from `perf.md` before and after.

### Explicitly out of scope
- `MapDataGraph` API redesign
- `TileManager` changes
- lock behavior changes
- `SmallVec`
- shared cross-task caches

## Expected code touch points
- `crates/ridi-router-routing/src/routing_context.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- existing `RoutingContext` tests
- graph adjacency tests for missing-neighbor behavior
- optional ignored microbench for repeated adjacency lookups if it helps local validation

## Implementation steps
1. Add a graph-level characterization test for missing-neighbor adjacency so later lock/refactor work cannot silently change current behavior.
2. Add a new cache field:
   ```rust
   adjacent: HashMap<MapDataPointRef, Vec<(MapDataLineRef, MapDataPointRef)>>,
   ```
3. Update `RoutingContext::adjacent(...)` to use that cache.
4. Expand `cache_sizes()` or add a focused helper so tests can see adjacency cache growth.
5. Add a unit test using the existing synthetic fixtures to call `ctx.adjacent(...)` twice and verify:
   - same returned neighbors
   - cache size grows once
6. Add a fresh-child-context test that proves a route-generation child context does not observe the parent's adjacency cache.
7. Re-run the existing fresh-context test coverage for other caches to confirm task-local behavior stays intact.

## Validation
1. `cargo test -p ridi-router-routing routing_context`
2. `cargo test -p ridi-router-routing map_data::graph`
3. `cargo check --workspace`
4. Profile run from repo root:
   ```bash
   RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia
   ```
5. Compare at minimum:
   - `graph::get_adjacent` calls / total / alloc
   - `tile_manager::get_adjacent_by_id` calls / total / alloc
   - route-generation wall time
   - walker hotspots that repeatedly call adjacency

## Exit criteria
- repeated same-point adjacency lookups within one `RoutingContext` are measurably reduced
- behavior stays unchanged
- the perf run shows whether option 1 is strong enough to keep first priority

## Decision gate
- If phase 1 clearly lowers adjacency call count and time, keep the remaining order as planned.
- If it barely moves the numbers, phase 2 becomes the more important next lever and phase 3 stays cleanup.

## Main risk
Low. The main watch-out is accidental cache sharing across route-generation tasks, which the current `RoutingContext` design already avoids.
