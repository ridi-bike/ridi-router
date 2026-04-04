# Map Data Refactor Review

## Verdict

The refactor looks **functionally complete for the routing crate and CLI path**.

The branch now matches the target architecture from the plan in the areas that mattered most:

- `RoutingExecutor` owns `Arc<MapDataGraph>`
- `RoutingExecutor::generate(&self, ...)` is in place
- `RoutingContext<'_>` exists and is threaded through runtime code
- singleton access in the routing crate has been removed
- route output construction is explicitly context-aware
- singleton-shaped test setup has been replaced with local graph/context helpers
- routing, CLI, and workspace tests pass

I do **not** see evidence that the routing crate still depends on:

- `MAP_DATA_GRAPH`
- `MapDataGraph::init(...)`
- `MapDataGraph::get()`
- `OPEN_TILES_DIR`
- `set_graph_static(...)`

## What I checked

Docs reviewed:

- `map-data-refactor-plan.md`
- `map-data-refactor-phase-1.md`
- `map-data-refactor-phase-2.md`
- `map-data-refactor-phase-3.md`
- `map-data-refactor-phase-4.md`
- `map-data-refactor-phase-5.md`
- `map-data-refactor-discoveries.md`

Code reviewed:

- `crates/ridi-router-routing/src/routing_api.rs`
- `crates/ridi-router-routing/src/routing_context.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/route_output.rs`
- `crates/ridi-router-routing/src/test_utils.rs`
- major routing modules under `crates/ridi-router-routing/src/router/**`
- `crates/ridi-router-cli/src/router_runner.rs`

Validation run locally on the branch:

- `cargo test -p ridi-router-routing -q`
- `cargo test -p ridi-router-cli -q`
- `cargo test --workspace -q`

All passed.

## Phase-by-phase review

### Phase 1 - foundation / executor ownership / context

**Implemented.**

Evidence in code:

- `crates/ridi-router-routing/src/routing_api.rs`
  - `RoutingExecutor` stores `graph: Arc<MapDataGraph>`
  - `RoutingExecutor::open(...)` constructs a fresh graph directly
  - `RoutingExecutor::new(graph: Arc<MapDataGraph>)` exists as crate-private/internal
- `crates/ridi-router-routing/src/routing_context.rs`
  - thin internal `RoutingContext<'_>` exists
  - explicit lookup methods exist for point, line, tag set, tag value, adjacency, and closest-point lookup
- tests in `routing_api.rs` verify multiple datasets can be opened in one process

### Phase 2 - top-level routing flow

**Implemented in code, but the phase doc status is stale.**

Evidence in code:

- `RoutingExecutor::generate(&self, ...)` is implemented
- start/finish lookup goes through `RoutingContext`
- `Generator`, `Walker`, and `Navigator` all accept/pass `&RoutingContext` on runtime paths
- route output construction uses `RouteComputation::from_routes(&ctx, ...)`
- CLI path in `crates/ridi-router-cli/src/router_runner.rs` uses the updated executor API cleanly

Doc issue:

- `map-data-refactor-phase-2.md` still says `_Status: planned_`, but its checklist is effectively reflected in the codebase now

### Phase 3 - lower-level helpers and domain methods

**Implemented.**

Evidence in code:

- helper logic in `weights.rs`, `route/*`, `clustering.rs`, `itinerary.rs`, `point.rs`, `line.rs`, and `rule.rs` now uses explicit context or explicit resolved values
- pure geometry helpers now operate on `MapDataPoint` values instead of ref `.get()` lookups
- route/segment bearing and stats logic now resolve through `RoutingContext`
- formatting/debug paths are shallow rather than doing hidden deep dereferencing

### Phase 4 - remove singleton and implicit dereference machinery

**Implemented.**

Evidence in code:

- `crates/ridi-router-routing/src/map_data/graph.rs` no longer defines singleton state or singleton accessors
- `MapDataElementRef<T>` is now just an ID wrapper with `tile_id` and `element_id`
- tag refs no longer expose ambient `.get()` lookups in the routing crate
- routing code resolves through `MapDataGraph` / `RoutingContext` explicitly

### Phase 5 - tests and final consolidation

**Implemented in code, but the phase doc status is stale.**

Evidence in code:

- `crates/ridi-router-routing/src/test_utils.rs` now provides `RoutingTestContext`
- tests create local graph-backed contexts instead of global singleton setup
- tests explicitly demonstrate multiple local graphs in one process
- workspace validation passes

Doc issue:

- `map-data-refactor-phase-5.md` still says `_Status: planned_`, even though its acceptance checklist reads like completed work and the code matches that state

## Problems / gaps I still see

These are the main things I would still call out.

### 1. The planning docs are behind the code

This is the clearest review issue.

Current mismatch:

- `map-data-refactor-phase-2.md` says planned, but the work is present
- `map-data-refactor-phase-5.md` says planned, but the work is present
- `map-data-refactor-plan.md` still reads like an active execution plan rather than a completed review of the landed branch

This is not a runtime bug, but it will confuse the next person reading the branch.

### 2. Tile-backed rule/filtering concerns are still unresolved

These were explicitly called out as separate from the refactor, and they are still separate now.

Still present:

- `crates/ridi-router-routing/src/map_data/graph.rs`
  - point rules are still returned as `Vec::new()` with a TODO to fetch from tiles
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
  - TODOs remain for rules filtering
  - TODOs remain for highway tag filtering
  - TODO remains for expanding-ring nearest search

So the **architecture cleanup is done**, but the older tile-behavior concerns from the discoveries doc are **not** resolved by this branch.

### 3. Wrong-context ref resolution is still an unchecked internal invariant

This matches the locked plan, but it is still worth calling out in a review.

Current shape:

- refs are lightweight IDs only
- `RoutingContext` resolves them against whatever `MapDataGraph` it was built from
- there is no runtime guard against resolving a ref from graph A through graph B

That was explicitly accepted in the plan, so I would not block the branch on it, but it remains a correctness hazard if internal callers ever mix graphs.

### 4. Internal lookup failures still fail hard

Also consistent with the plan, but still worth documenting.

`crates/ridi-router-routing/src/map_data/graph.rs` still uses `expect(...)` / `unwrap(...)` for internal tile-backed lookups. That means corrupted data, invalid refs, or cross-context mistakes will still panic rather than degrade gracefully.

Again: this matches the chosen scope, but if the next step is making the routing library more defensive, this is where the work would start.

### 5. Repo-wide cleanup is not complete outside the routing crate

The routing refactor itself looks complete, but the repo still contains old-style helper APIs in `crates/ridi-router-tiles`, for example:

- `crates/ridi-router-tiles/src/map_data/graph.rs`
  - `ElementTagValueRef::get()`
  - `ElementTagSetRef::get()`
- `crates/ridi-router-tiles/src/map_data/point.rs`
  - ref-based geometry helpers
- `crates/ridi-router-tiles/src/map_data/line.rs`
  - old-style length helper naming/shape

I would treat this as **out of scope for the routing refactor**, not as a failure of the branch. But if the intent is whole-repo consistency, this remains follow-up work.

### 6. There is still cleanup noise in warnings/dead code

The workspace test run is green, but there are a number of warnings and clearly transitional/dead items, especially around:

- `crates/ridi-router-routing/src/map_data/generation_graph.rs`
- unused helper methods in the routing map-data layer
- some unused imports / helpers in other crates

This is not a correctness blocker, but it suggests there is still consolidation work available after the architectural refactor.

## Have the original discoveries been resolved?

### 1. `MapDataGraph` is still a global singleton

**Resolved in the routing crate.**

No singleton root remains in `crates/ridi-router-routing/src/map_data/graph.rs`.

### 2. Routing was initialized from runner-owned global state

**Resolved.**

`crates/ridi-router-cli/src/router_runner.rs` now opens a `RoutingExecutor`, and the executor owns its graph state. The old global-open pattern is gone from the routing path.

### 3. `MapDataGraph` was already close to an instance-scoped context

**Resolved as an architectural direction.**

The branch now actually uses that shape:

- graph instance owned by executor
- thin borrowed `RoutingContext`
- explicit map-data access through context/graph

### 4. The routing stack reached into global map data from many places

**Resolved for runtime routing code in the routing crate.**

Generator, walker, navigator, route output, scoring, clustering, and itinerary code now use explicit context threading.

### 5. Ref types were globally coupled

**Resolved in the routing crate.**

Refs are now plain identifiers. Ambient dereference methods have been removed from the routing crate path.

### 6. The interim executor plan was only containment

**Superseded / resolved.**

The branch goes beyond the interim containment step. It reaches the explicit instance-scoped architecture instead of stopping at singleton hiding.

### 7. Incomplete tile-backed behaviors existed separately

**Not resolved by this branch.**

This remains true and still shows up in code TODOs.

### 8. Tests depended on the singleton

**Resolved.**

The new test setup uses local graph-backed harnesses via `RoutingTestContext`.

### 9. The crate was still binary-first and needed a cleaner library shape

**Mostly resolved.**

The routing crate now has a much cleaner executor/request/result API and does not publicly expose CLI-specific internals. This is materially more library-ready than the previous design.

## Bottom line

### My review outcome

**Yes: the main map-data refactor appears completed for the routing crate and CLI integration.**

I would describe the branch as:

- **architecturally complete** for the scoped refactor
- **validated by tests**
- **not fully documented as complete yet**
- still carrying a few **explicitly out-of-scope follow-ups**

### What I would address next

In priority order:

1. Update the plan/phase docs so they reflect the branch reality
2. Decide whether the unresolved tile TODOs should become their own tracked follow-up
3. Decide whether wrong-context / invalid-ref failure modes should remain fail-hard or be hardened for library use
4. Clean up dead code and warning noise
5. Optionally align `crates/ridi-router-tiles` with the same explicit-access model if repo-wide consistency is desired
