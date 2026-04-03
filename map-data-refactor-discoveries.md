# Map Data Refactor Discoveries

This document captures discoveries for the **follow-up refactor** that removes the remaining old global map-data architecture.

The main IPC simplification can proceed first, but the bigger structural issue is still the routing stack's dependence on global map-data access.

---

## Executive Summary

The current RMDF routing path is not truly request-scoped yet.

The code already has:
- request-local `TileManager` creation in `src/router_runner.rs`
- RMDF manifest loading in `src/rmdf/tile_manager.rs`
- tile-backed `MapDataGraph` methods in `src/map_data/graph.rs`

But the routing stack still fundamentally assumes:
- one global `MapDataGraph`
- initialized once
- accessed everywhere through `MapDataGraph::get()`

That assumption is the main leftover from the old long-lived process model.

---

## Main Discoveries

## 1. `MapDataGraph` is still a global singleton

File:
- `src/map_data/graph.rs`

Key lines discovered:
- `pub static MAP_DATA_GRAPH: OnceLock<MapDataGraph> = OnceLock::new();`
- `MapDataGraph::init(tiles_path)` initializes it once
- `MapDataGraph::get()` returns the global instance

Implication:
- routing still depends on hidden process-global initialization order
- one-shot CLI execution and future library usage are both awkward with this model

Why this matters for the follow-up:
- a library should not depend on hidden global state for correctness
- multiple independent route computations in one process become harder to reason about
- tests also need singleton workarounds

---

## 2. The current route path still does not cleanly wire request-local tiles into routing

File:
- `src/router_runner.rs`

Current behavior:
- `run_generate_route()` creates `TileManager::new(tiles_dir)`
- it then does an unsafe transmute to `&'static mut TileManager`
- that value is passed into `generate_route_with_tiles()`
- but the parameter is unused
- actual routing still accesses `MapDataGraph::get()`

This shows a mismatch:
- the runner thinks in terms of request-local state
- the routing core still thinks in terms of global state

Implication:
- the current code is transitional glue, not a stable design
- the follow-up refactor should remove this mismatch instead of extending it

---

## 3. `MapDataGraph` already wraps `TileManager`, but behind a global access pattern

File:
- `src/map_data/graph.rs`

Current structure:
- `MapDataGraph` stores `tile_manager: RwLock<crate::rmdf::TileManager>`
- methods like `get_closest_to_coords()`, `get_adjacent()`, `get_point_from_tiles()`, and `get_line_from_tiles()` already route through tile-backed lookup

This is good news:
- the tile-based backend is already in place
- the larger problem is mostly access pattern and lifecycle, not missing functionality

Implication:
- the follow-up does not need to reinvent the backend
- it mainly needs to replace global singleton access with request-scoped shared access

---

## 4. The routing stack reaches into global map data from many places

Files:
- `src/router/generator.rs`
- `src/router/navigator.rs`
- `src/router/walker.rs`
- `src/router/weights.rs`
- `src/map_data/graph.rs`

Observed pattern:
- route generation frequently calls `MapDataGraph::get()` directly
- this happens for closest-point lookup, adjacency traversal, point/line dereferencing, and tag lookups

Implication:
- removing the singleton is a multi-file refactor
- this is the real reason it should be its own dedicated follow-up

Expected shape of the future work:
- pass shared routing context explicitly through generator/navigator/walker/weights
- or make references carry enough context to avoid repeated global calls

---

## 5. The current map-data API is already close to a reusable routing context

Files:
- `src/map_data/graph.rs`
- `src/rmdf/tile_manager.rs`

The existing `MapDataGraph` already acts like a request context:
- owns access to `TileManager`
- resolves points and lines on demand
- centralizes tile-aware lookups

That suggests a straightforward follow-up path:

### Conservative path
- keep the `MapDataGraph` type name
- remove the global `OnceLock`
- instantiate `MapDataGraph` per request
- share it via `Arc<MapDataGraph>`

### Cleaner path
- rename `MapDataGraph` to something like `RoutingContext`
- remove the old global naming and assumptions entirely

My discovery-based recommendation:
- start with the conservative path first
- rename later if it still feels useful

---

## 6. There are still incomplete tile-backed behaviors in the map-data layer

Files:
- `src/map_data/graph.rs`
- `src/rmdf/tile_manager.rs`

Notable TODOs found:
- `src/map_data/graph.rs`: `// TODO: Implement proper tag loading from tiles`
- `src/map_data/graph.rs`: `rules: Vec::new(), // TODO: Fetch rules from tiles`
- `src/rmdf/tile_manager.rs`: `// TODO: Implement rules filtering`
- `src/rmdf/tile_manager.rs`: `// TODO: Implement highway tag filtering`

Implication:
- the follow-up map-data refactor should not assume the tile access layer is fully complete
- there is architectural cleanup work and feature-completeness work mixed together

Important distinction:
- removing the singleton is one refactor
- finishing all tile-backed filtering/rule-loading semantics may be a separate concern

---

## 7. Test support currently depends on the singleton too

File:
- `src/test_utils.rs`

Found:
- `set_graph_static(map_data: MapDataGraph) -> &'static MapDataGraph`
- many tests use global graph setup assumptions

Implication:
- the follow-up refactor will need a testing story that does not depend on a process-global singleton
- likely by constructing a request-scoped context directly in tests

This should improve tests, but it will require touching test helpers.

---

## 8. The crate is still binary-only today

Discovery:
- there is no `src/lib.rs`
- the repository currently exposes a CLI, not a reusable library surface

Implication for the map-data follow-up:
- singleton removal should be done in a way that makes future library extraction easy
- explicit context passing is much more library-friendly than hidden globals

This matters because future library consumers will likely want:
- to create a routing engine/context explicitly
- to submit route requests programmatically
- to receive route results as in-memory data structures

---

## 9. `MapDataGraph` currently provides a natural seam for library extraction

If the project later becomes a library, a sensible split would be:

- library domain types and route computation in `src/lib.rs`
- CLI parsing/orchestration in `src/main.rs` / runner modules

A future library-facing API could be shaped around:
- `RoutingContext::from_tiles_dir(...)`
- `RouteRequest`
- `RouteResult` / `ComputedRoute`
- optional writer/helpers for GPX/JSON/NDJSON outside the core engine

The current global singleton model is the biggest blocker to that direction.

---

## Recommended Follow-up Refactor Direction

## Phase A - Remove global initialization assumptions

Goal:
- stop using `MAP_DATA_GRAPH` / `MapDataGraph::get()` in runtime code

Plan direction:
- construct `MapDataGraph` per request
- share via `Arc<MapDataGraph>`
- pass it through generator/navigator/walker/weights

## Phase B - Fix runner/context ownership boundary

Goal:
- remove unsafe transmute and the unused tile-manager parameter path in `router_runner.rs`

Plan direction:
- runner creates the request-scoped context
- routing stack receives that context explicitly

## Phase C - Repair test helpers

Goal:
- make tests construct local contexts directly
- stop depending on singleton initialization helpers

## Phase D - Prepare for library extraction

Goal:
- make CLI a thin wrapper over reusable Rust API

Plan direction:
- introduce `src/lib.rs` later
- keep core routing request/response types independent from CLI/file concerns

---

## What This Follow-up Refactor Should Not Automatically Try to Do

To keep it manageable, the follow-up refactor should not automatically absorb every remaining concern.
These may need to stay separate unless you explicitly want to combine them:

- full JSON output redesign
- NDJSON event streaming implementation
- completion of all tag/rule loading TODOs from tiles
- broader routing algorithm changes
- all library extraction work at once

The main purpose of the follow-up should be:
- remove singleton/global map-data access
- make runtime state explicit and request-scoped
- improve future library readiness

---

## Bottom Line

The IPC cleanup is the easy visible cleanup.

The deeper architectural follow-up is:
- remove `MapDataGraph` as a global singleton
- make routing context explicit
- make the core usable both from CLI and, later, from a Rust library

That follow-up is where the old server-era architecture is really still holding on.
