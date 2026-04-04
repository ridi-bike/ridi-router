# Map Data Refactor - Phase 1: Foundation (`RoutingExecutor` ownership + `RoutingContext`)

_Status: planned_

## Purpose

Establish the explicit ownership model that the rest of the refactor depends on:

- `RoutingExecutor` owns `Arc<MapDataGraph>`
- process-global open state is removed from the executor path
- internal `RoutingContext<'_>` exists as the explicit lookup surface
- multiple datasets can be opened in one process

This phase combines the plan's original foundation work:

- original phase 1: instance-owned executor state
- original phase 2: explicit resolver/context layer

## Scope

### In scope

- stop using `OPEN_TILES_DIR` in `routing_api.rs`
- stop using `MapDataGraph::init(...)` in the executor open path
- make `RoutingExecutor` store `Arc<MapDataGraph>`
- add an internal/test-oriented `RoutingExecutor::new(graph: Arc<MapDataGraph>)`
- add internal `RoutingContext<'a>` that borrows `&'a MapDataGraph`
- add explicit lookup methods that mostly delegate to `MapDataGraph` primitives:
  - `point(...)`
  - `line(...)`
  - `tag_set(...)`
  - `tag_value(...)`
  - `adjacent(...)`
  - `closest_to_coords(...)`
- keep `RoutingContext` internal to the crate

### Out of scope

- broad call-site conversion across the routing stack
- removing ref/tag `.get()` methods yet
- full test-harness rewrite
- tile TODOs for rule loading/filtering
- wrong-context runtime guards

## Primary files

Illustrative, not exhaustive.

- `crates/ridi-router-routing/src/routing_api.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/lib.rs`
- possibly a new internal file such as `crates/ridi-router-routing/src/routing_context.rs`

## Implementation details

### 1. Make graph ownership explicit in the executor

Update `RoutingExecutor` so it owns `Arc<MapDataGraph>` instead of only remembering the tiles dir.

Preferred shape:

```rust
pub struct RoutingExecutor {
    graph: Arc<MapDataGraph>,
}
```

Implementation notes:

- keep `open(...)` as the main constructor
- add `RoutingExecutor::new(graph: Arc<MapDataGraph>)` for internal/test use
- keep the constructor surface narrow; do not expose `RoutingContext` publicly
- `open(...)` should construct a fresh `MapDataGraph` directly instead of initializing a singleton

### 2. Remove executor-level global-open bookkeeping

`routing_api.rs` currently uses `OPEN_TILES_DIR` and `MapDataGraph::init(...)` as containment around the singleton design. Remove that containment layer entirely in this phase.

Expected direction:

- delete `OPEN_TILES_DIR`
- delete the conflicting-tiles-dir restriction in the executor open path
- continue validating the manifest before or during graph construction
- create `MapDataGraph` directly from `tiles_dir`

If the cleanest path is to add a graph constructor such as `MapDataGraph::open(PathBuf) -> Result<Self, ...>`, that is acceptable.

### 3. Introduce `RoutingContext<'_>` as a thin delegating wrapper

Add an internal context type:

```rust
pub struct RoutingContext<'a> {
    graph: &'a MapDataGraph,
}
```

Design rules:

- keep it thin and boring
- mostly delegate to low-level `MapDataGraph` methods
- no request-local caching in this refactor
- no stored `Arc<MapDataGraph>` inside refs or domain objects
- treat wrong-context ref resolution as an internal invariant, not a guarded runtime case

### 4. Provide explicit lookup equivalents for current global dereference paths

The context layer needs to be a complete landing zone before the mechanical conversion starts.

Required lookup equivalents:

- point lookup from `MapDataPointRef`
- line lookup from `MapDataLineRef`
- tag-set lookup from `ElementTagSetRef`
- tag-value lookup from `ElementTagValueRef`
- adjacency lookup from `MapDataPointRef`
- closest-point lookup from coordinates + routing rules

The important outcome is not elegance yet. The important outcome is that later phases can stop reaching for `MapDataGraph::get()` and stop inventing ad hoc lookup helpers.

### 5. Keep low-level graph primitives in place

Do not move all lookup logic into `RoutingContext`. The plan explicitly keeps primitive lookup methods on `MapDataGraph`, with `RoutingContext` delegating to them.

That means:

- keep `MapDataGraph` as the real tile-backed data owner
- use `RoutingContext` as the routing-friendly lookup surface
- avoid turning `RoutingContext` into another hidden global service layer

## Dependencies and handoff to the next phase

This phase is complete when top-level route generation can start depending on the new context API, even if most old `.get()` paths still exist.

Phase 2 should be able to assume:

- `RoutingExecutor` has graph ownership
- `RoutingExecutor::generate(&self, ...)` is now possible
- `RoutingContext` exposes the lookup operations needed by generator/walker/navigator

## Acceptance criteria

- `RoutingExecutor` owns `Arc<MapDataGraph>`
- `RoutingExecutor::open(...)` no longer uses `OPEN_TILES_DIR`
- `RoutingExecutor::open(...)` no longer uses `MapDataGraph::init(...)`
- two executors can be opened with different `tiles_dir` values in one process
- internal `RoutingContext<'_>` exists and borrows `&MapDataGraph`
- every current global dereference category has an explicit `RoutingContext` equivalent:
  - point
  - line
  - tag set
  - tag value
  - adjacency
  - closest-to-coordinates
- new code can perform those lookups without calling `MapDataGraph::get()`
- no long-lived compatibility shim is introduced for executor open behavior

## Validation checklist

- [ ] `RoutingExecutor` stores `Arc<MapDataGraph>`
- [ ] `RoutingExecutor::new(graph: Arc<MapDataGraph>)` exists for internal/test use
- [ ] `routing_api.rs` no longer defines or uses `OPEN_TILES_DIR`
- [ ] `routing_api.rs` no longer calls `MapDataGraph::init(...)`
- [ ] `RoutingContext<'_>` exists and is crate-internal
- [ ] `RoutingContext` methods exist for point, line, tag set, tag value, adjacency, and closest-point lookup
- [ ] two executors can be constructed for different datasets in one process
- [ ] no caller in this phase needs singleton open ordering for correctness
- [ ] any new graph constructor/open helper has typed error flow compatible with `RoutingOpenError`

### Suggested checks

- [ ] `rg "OPEN_TILES_DIR|ConflictingTilesDir|MapDataGraph::init" crates/ridi-router-routing/src`
- [ ] targeted tests for opening different datasets in one process
- [ ] `cargo test -p ridi-router-routing routing_api -- --nocapture`

## Progress checklist

- [ ] update `RoutingExecutor` storage to `Arc<MapDataGraph>`
- [ ] remove `OPEN_TILES_DIR`
- [ ] switch `open(...)` from singleton init to direct graph construction
- [ ] add `RoutingExecutor::new(graph: Arc<MapDataGraph>)`
- [ ] add internal `RoutingContext<'_>` type
- [ ] add explicit context lookup methods
- [ ] ensure `RoutingContext` delegates to `MapDataGraph` primitives
- [ ] add or update tests for multi-dataset open behavior
- [ ] confirm phase acceptance criteria are met

## Notes and watch-outs

- Keep `RoutingContext` thin. Do not start smuggling policy or caching into it.
- Avoid exposing `RoutingContext` publicly in this phase.
- Do not delay this phase by trying to delete old `.get()` paths early; that is a later phase.
- Temporary breakage outside the intentionally converted area is acceptable if the foundation is in place and the acceptance criteria above are met.
