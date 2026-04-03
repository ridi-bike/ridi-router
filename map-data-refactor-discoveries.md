# Map Data Refactor Discoveries

This document captures discoveries for the **future follow-up refactor** that removes the remaining process-global map-data architecture from routing.

The immediate library split can proceed first with an executor-shaped API, but the deeper routing internals are still built around global map-data access.

---

## Executive Summary

The current RMDF routing path is **tile-backed**, but it is **not instance-scoped** yet.

What the code already has:
- `MapDataGraph` as a concrete struct that wraps tile access
- `TileManager`-backed lookup for point, line, adjacency, and nearest-point operations
- a natural place to hang routing context state

What the code still assumes:
- one process-global `MapDataGraph`
- initialized once through `OnceLock`
- accessed from routing code through `MapDataGraph::get()`
- references and tag lookups that resolve through that same global instance

That means the real architectural problem is not missing tile support. It is the **global access pattern**.

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
- runtime correctness still depends on hidden process-global initialization order
- this is acceptable for the short-term CLI flow, but awkward for a reusable library
- opening different datasets in the same process is not safely supported by the current design

Why this matters for the follow-up:
- a routing library should not depend on ambient global state for correctness
- explicit context ownership is a much cleaner base for testing and reuse

---

## 2. The current route path already initializes the graph from the runner, not from request-local state

File:
- `src/router_runner.rs`

Current behavior:
- `run_generate_route()` calls `MapDataGraph::init(tiles_dir)`
- route generation then resolves start and finish through `MapDataGraph::get()`
- deeper routing code also continues to call `MapDataGraph::get()` directly

Implication:
- graph setup is currently CLI-owned global initialization
- moving that initialization into `RoutingExecutor::open(...)` is a reasonable interim step for library extraction
- that interim step does **not** make the graph executor-scoped; it only hides the singleton behind the executor API

Short-term design consequence:
- the routing library should reject subsequent `open(...)` calls with a different `tiles_dir`
- silently reusing the first dataset would be incorrect and dangerous

---

## 3. `MapDataGraph` itself is already close to an instance-scoped routing context

Files:
- `src/map_data/graph.rs`
- `src/rmdf/tile_manager.rs`

Current structure:
- `MapDataGraph` stores `tile_manager: RwLock<crate::rmdf::TileManager>`
- methods like `get_closest_to_coords()`, `get_adjacent()`, `get_point_from_tiles()`, and `get_line_from_tiles()` already route through tile-backed lookup
- `TileManager` owns manifest loading, tile loading, adjacency lookup, and nearest-point lookup

This is good news:
- the tile backend is already there
- the bigger problem is lifecycle and access pattern, not the lack of a routing context type

Implication:
- the future refactor does not need to invent a new backend first
- it mainly needs to replace global singleton access with instance-owned shared access

---

## 4. The routing stack reaches into global map data from many places

Files:
- `src/router/generator.rs`
- `src/router/walker.rs`
- `src/router_runner.rs`
- `src/map_data/graph.rs`
- test code in `src/router/navigator.rs`, `src/router/walker.rs`, and `src/router/weights.rs`

Observed runtime pattern:
- route generation frequently calls `MapDataGraph::get()` directly
- walker traversal calls `MapDataGraph::get().get_adjacent(...)`
- closest-point lookup calls `MapDataGraph::get().get_closest_to_coords(...)`

Implication:
- removing the singleton is a multi-file refactor
- this is the main reason it should be treated as a dedicated follow-up instead of being bundled into the workspace split

Expected shape of the future work:
- pass shared routing context explicitly through generator, walker, navigator, and related helpers
- or centralize those operations behind objects that already own the context

---

## 5. The ref types are also globally coupled

File:
- `src/map_data/graph.rs`

Important discovery:
- `MapDataElementRef<T>::get()` resolves through `T::get_from_tiles(...)`
- `MapDataPoint` and `MapDataLine` implement that trait by calling `MapDataGraph::get()`
- `ElementTagValueRef::get()` and `ElementTagSetRef::get()` also call `MapDataGraph::get()`

Why this matters:
- the singleton is not only used in high-level routing code
- it is also embedded in the basic reference/dereference model for points, lines, and tags

Implication:
- a future instance-scoped refactor is not just "stop calling `MapDataGraph::get()` in generator"
- it also needs a new strategy for dereferencing point, line, and tag refs without ambient global state

This is the strongest reason the full refactor is not a quick cleanup.

---

## 6. The short-term executor plan is valid, but it is only an interim containment step

Recommended interim direction for the library split:
- move graph initialization into `RoutingExecutor::open(config)`
- keep the executor/session-shaped public API
- allow reuse of the already-opened graph only when the same `tiles_dir` is requested again
- reject a conflicting `tiles_dir` with a typed initialization/open error

What this buys us now:
- the CLI stops owning graph initialization directly
- the reusable library gets the right public shape
- the singleton becomes an internal implementation detail instead of a caller-facing requirement

What it does **not** buy us yet:
- true executor-scoped routing state
- multiple independent datasets in the same process
- full removal of hidden global map-data assumptions

This is a good transition step, not the end state.

---

## 7. There are still incomplete tile-backed behaviors in the map-data layer

Files:
- `src/map_data/graph.rs`
- `src/rmdf/tile_manager.rs`

Notable TODOs found:
- `src/map_data/graph.rs`: proper tag loading from tiles is still marked TODO
- `src/map_data/graph.rs`: point rules are still returned as `Vec::new()` with a TODO to fetch from tiles
- `src/rmdf/tile_manager.rs`: rules filtering is still TODO
- `src/rmdf/tile_manager.rs`: highway tag filtering is still TODO

Implication:
- the future map-data refactor should not assume tile access semantics are fully complete
- there are two related but separate concerns:
  - architectural cleanup of global access
  - finishing tile-backed filtering and rule-loading behavior

Recommendation:
- keep those concerns separate unless there is a strong reason to merge them

---

## 8. Tests currently depend on the singleton too

Files:
- `src/test_utils.rs`
- routing tests that use `set_graph_static(...)`

Found:
- `set_graph_static(map_data: MapDataGraph) -> &'static MapDataGraph`
- many tests rely on singleton graph setup and then access points through `MapDataGraph::get()`

Implication:
- the future refactor will need a testing story that constructs local routing contexts directly
- test helpers will need to stop assuming process-global graph state

This should improve test isolation, but it will require touching test helpers and routing tests.

---

## 9. The crate is still binary-first today, so this refactor should prepare for a library cleanly

Discovery:
- the current repo still behaves like a CLI-first application
- the routing stack is not yet organized around an explicit library-owned runtime context

Implication for the follow-up:
- removing singleton access should be done in a way that makes future library extraction simpler
- explicit context passing is much more library-friendly than hidden globals

Future library consumers will likely want:
- to create a routing executor/context explicitly
- to submit route requests programmatically
- to receive route results as Rust values without CLI/file-output concerns

---

## Recommended Follow-up Refactor Direction

## Phase 0 - Interim containment during library extraction

Goal:
- keep the public routing API executor-shaped now without doing the full graph refactor yet

Plan direction:
- initialize graph state inside `RoutingExecutor::open(...)`
- store enough internal metadata to detect the opened `tiles_dir`
- reject subsequent opens that try to use a different tiles directory in the same process
- document the temporary limitation: one routing dataset per process

This phase is about **containment**, not architectural completion.

## Phase A - Remove runtime dependence on `MapDataGraph::get()`

Goal:
- stop using `MAP_DATA_GRAPH` / `MapDataGraph::get()` in runtime routing code

Plan direction:
- construct `MapDataGraph` as owned executor state
- pass shared context explicitly through generator, walker, navigator, and related helpers
- stop making routing logic reach outward for ambient global state

## Phase B - Replace globally coupled ref dereferencing

Goal:
- make point, line, and tag refs work without `MapDataGraph::get()`

Possible directions:
- make refs lightweight IDs only and resolve them through explicit context methods
- or give routing objects methods that do the dereferencing on behalf of callers
- avoid baking long-lived ambient graph access into the ref types themselves

This phase is the real heart of the instance-scoping refactor.

## Phase C - Repair test helpers

Goal:
- make tests construct local contexts directly
- remove singleton-based helper setup

Plan direction:
- create test-local graph/context builders
- update routing tests to use explicit owned context instead of static setup

## Phase D - Prepare for later cleanup and naming simplification

Goal:
- make the routing context model obvious and library-friendly

Possible direction:
- keep the `MapDataGraph` name during the mechanical refactor first
- consider renaming later to something like `RoutingContext` only if that genuinely improves clarity

---

## What This Follow-up Refactor Should Not Automatically Try to Do

To keep it manageable, the follow-up should not automatically absorb every remaining concern.
These may need to stay separate unless explicitly combined on purpose:

- full JSON output redesign
- event streaming API design
- broader routing algorithm changes
- all workspace/library extraction work at once
- every remaining tile-feature TODO in the same change

The main purpose of the follow-up should be:
- remove singleton/global map-data access
- make runtime state explicit and instance-scoped
- improve long-term library readiness

---

## Bottom Line

The library split can move forward now with an executor/session API even if the first implementation still hides a process-global `MapDataGraph` internally.

That is acceptable **only** as an interim step, with explicit rejection of conflicting `tiles_dir` openings.

The real follow-up refactor is still needed:
- remove `MapDataGraph` as a global singleton
- remove global dereferencing from map-data refs
- make routing context explicit and instance-scoped
- make the routing core cleanly reusable from a Rust library
