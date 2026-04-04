# Map Data Refactor Plan

_Status: draft updated after clarification_

This plan is based on:
- `map-data-refactor-discoveries.md`
- code inspection in `crates/ridi-router-routing`
- CLI integration in `crates/ridi-router-cli`

## Confirmed decisions

These are now settled:

1. **Ref model**: use **lightweight ID refs + explicit context**
2. **Multi-dataset support**: **required**
3. **Public API stability**: **not required** on this branch
4. **Migration style**: several validated phases; phases do **not** need to be independently shippable
5. **Naming**: keep **`MapDataGraph`**
6. **Tests**: rework singleton-based tests in the **same refactor**
7. **Tile TODOs**: explicitly **out of scope**
8. **Sharing**: executor/context should be **shareable**
9. **Context type**: use **`RoutingContext`**
10. **Context ownership**: use a **borrowed** `RoutingContext<'_>` over `&MapDataGraph`
11. **Context flow**: pass `RoutingContext` into the methods that need it instead of storing it in every routing object
12. **Lookup return style**: return **owned values first**, like today, to keep the refactor focused on architecture
13. **Executor API**: `RoutingExecutor::generate(&self, ...)`
14. **Formatting/debug**: keep `Debug` / `Display` **minimal** instead of deep context-dependent formatting
15. **Constructor surface**: keep `open(...)` and add a lower-level `RoutingExecutor::new(graph: Arc<MapDataGraph>)`, but keep that constructor **internal/test-oriented for now**
16. **Helper redesign style**: option **A** — keep domain methods and add context where needed
17. **`RoutingContext` visibility**: keep it **internal**, not part of the crate's public API for now
18. **Lookup API layering**: keep low-level primitive lookup methods on `MapDataGraph`, with `RoutingContext` mostly delegating to them
19. **Migration compatibility**: remove singleton APIs and ref `.get()` methods **as soon as explicit replacements exist**, rather than keeping long-lived compatibility shims
20. **Lookup failure semantics**: keep the current internal assumption that refs are valid; invalid internal lookups may still fail hard in this refactor
21. **Performance scope**: keep `RoutingContext` thin first; no request-local memoization/caching in this refactor

## Goal

Remove the remaining process-global map-data architecture from routing so routing state becomes explicit, instance-scoped, and safe for multiple datasets in one process.

## What the codebase already tells us

### Public API exposure is limited

`crates/ridi-router-routing/src/lib.rs` does not publicly expose `map_data` internals. Most of this refactor is internal even if `RoutingExecutor` changes.

### The current executor only hides the singleton

`crates/ridi-router-routing/src/routing_api.rs` has the right shape already, but it still uses:
- `MapDataGraph::init(...)`
- `MapDataGraph::get()`
- process-global `OPEN_TILES_DIR`

So the executor is not yet the owner of routing state.

### The strongest coupling is in ref dereferencing

`crates/ridi-router-routing/src/map_data/graph.rs` still embeds global access in:
- `MapDataElementRef<T>::get()`
- `MapDataPoint` / `MapDataLine` trait-based dereferencing via `MapDataGraph::get()`
- `ElementTagValueRef::get()`
- `ElementTagSetRef::get()`

That means the refactor is not just a generator/walker cleanup. The basic point/line/tag dereference model must change.

### The routing stack is full of implicit dereferencing

Large parts of the crate rely on `.get()` on point, line, and tag refs:
- `router/generator.rs`
- `router/walker.rs`
- `router/navigator.rs`
- `router/weights.rs`
- `router/route/*`
- `router/itinerary.rs`
- `map_data/point.rs`
- `map_data/line.rs`
- `map_data/rule.rs`

So this is a broad internal refactor, not a small patch.

### Tests are singleton-shaped too

`crates/ridi-router-routing/src/test_utils.rs` still provides `set_graph_static(...)`, and many tests fetch refs through `MapDataGraph::get()`.

### Tile behavior completion is separate

`TileManager` / `MapDataGraph` still have TODOs for rule loading/filtering and highway filtering. Those are out of scope here.

## Target architecture

## Core idea

Keep refs as pure IDs and make all dereferencing explicit through an instance-owned context wrapper.

### Proposed ownership model

- `RoutingExecutor` owns `Arc<MapDataGraph>`
- routing operations borrow a small helper wrapper named `RoutingContext`
- `RoutingContext` itself borrows `&MapDataGraph`
- refs remain lightweight IDs:
  - `MapDataPointRef`
  - `MapDataLineRef`
  - `ElementTagValueRef`
  - `ElementTagSetRef`
- dereferencing happens through `RoutingContext`, not through global state

### Tentative shape

```rust
pub struct RoutingExecutor {
    graph: Arc<MapDataGraph>,
}

pub struct RoutingContext<'a> {
    graph: &'a MapDataGraph,
}

impl RoutingExecutor {
    pub fn new(graph: Arc<MapDataGraph>) -> Self { ... }
    pub fn generate(&self, request: RouteRequest) -> Result<RouteComputation, RoutingError> { ... }
}

impl RoutingContext<'_> {
    pub fn point(&self, point_ref: &MapDataPointRef) -> MapDataPoint { ... }
    pub fn line(&self, line_ref: &MapDataLineRef) -> MapDataLine { ... }
    pub fn tag_set(&self, tag_ref: &ElementTagSetRef) -> ElementTagSet { ... }
    pub fn tag_value(&self, tag_ref: &ElementTagValueRef) -> Option<String> { ... }
    pub fn adjacent(&self, point_ref: &MapDataPointRef) -> Vec<(MapDataLineRef, MapDataPointRef)> { ... }
    pub fn closest_to_coords(...) -> Option<MapDataPointRef> { ... }
}
```

Routing objects like `Generator`, `Walker`, and `Navigator` receive `&RoutingContext` in the methods that need lookup access.

## Why this direction fits your choices

This model:
- fully removes process-global state
- naturally supports multiple datasets in one process
- keeps refs small and cheap
- keeps dataset ownership explicit
- avoids putting `Arc<MapDataGraph>` inside every ref
- gives cleaner ergonomics than threading raw graph everywhere

## Recommended design principles

### 1. Keep refs dumb

Refs should identify data, not own access to it.

### 2. Keep ownership explicit

The resolver/context should borrow or wrap a graph already owned by the executor or test harness.

### 3. Prefer owned results for this refactor

Resolver methods should return owned `MapDataPoint`, `MapDataLine`, and tag values first. Performance-oriented borrowed/view designs can come later.

### 4. Avoid restoring ambient access through helpers

The resolver can reduce parameter noise, but it should still be visibly threaded through the routing stack.

## Proposed phases

## Phase 1 - Introduce instance-owned executor state

### Changes

In `crates/ridi-router-routing/src/routing_api.rs`:
- remove `OPEN_TILES_DIR`
- stop using `MapDataGraph::init(...)`
- construct `MapDataGraph` directly in `RoutingExecutor::open(...)`
- store `Arc<MapDataGraph>` in `RoutingExecutor`
- allow opening different datasets in the same process

In `crates/ridi-router-routing/src/map_data/graph.rs`:
- keep normal constructors for direct graph creation
- do **not** plan on long-lived singleton compatibility shims; remove singleton access as soon as the first explicit replacements land

### Validation

- two executors can be opened with different `tiles_dir` values in one process
- routing no longer depends on process-global open order

## Phase 2 - Add the explicit resolver/context layer

### Changes

Add internal `RoutingContext` with explicit lookup methods that mostly delegate to low-level `MapDataGraph` primitives:
- `point(...)`
- `line(...)`
- `tag_set(...)`
- `tag_value(...)`
- `adjacent(...)`
- `closest_to_coords(...)`

This creates a clean landing zone before the larger mechanical conversion.

### Validation

- every current global dereference path has an explicit resolver equivalent
- new code can use the resolver without calling `MapDataGraph::get()`

## Phase 3 - Convert entrypoints and high-level routing flow

### Primary files

- `crates/ridi-router-routing/src/routing_api.rs`
- `crates/ridi-router-routing/src/router/generator.rs`
- `crates/ridi-router-routing/src/router/walker.rs`
- `crates/ridi-router-routing/src/router/navigator.rs`

### Changes

- `RoutingExecutor::generate(...)` creates/borrows a resolver
- generator resolves closest points through the resolver
- walker resolves adjacency through the resolver
- navigator passes resolver access into deeper operations instead of depending on implicit ref `.get()` behavior

### Validation

- runtime generator/walker paths no longer call `MapDataGraph::get()`
- resolver is visibly threaded through the main route-generation flow

## Phase 4 - Convert lower-level domain helpers away from implicit ref `.get()`

### Primary files

- `crates/ridi-router-routing/src/router/weights.rs`
- `crates/ridi-router-routing/src/router/route/mod.rs`
- `crates/ridi-router-routing/src/router/route/segment.rs`
- `crates/ridi-router-routing/src/router/itinerary.rs`
- `crates/ridi-router-routing/src/map_data/point.rs`
- `crates/ridi-router-routing/src/map_data/line.rs`
- `crates/ridi-router-routing/src/map_data/rule.rs`

### What changes here

This is where implicit deref-heavy helpers need redesign.

Examples:
- methods that currently do `point_ref.get().lat`
- methods that currently do `line_ref.get().tags.get().highway()`
- formatting helpers that dereference refs transitively

### Preferred direction

Where a helper is really a lookup helper, move that logic to the resolver.

Where a helper is really pure geometry or formatting, make it operate on already-resolved owned values.

Examples:
- geometry helpers should prefer coordinates or resolved values instead of ref dereferencing
- tag-based stats helpers should use resolver methods instead of nested `.get()` chains

### Validation

- runtime routing code no longer depends on `MapDataElementRef<T>::get()`
- runtime routing code no longer depends on tag ref `.get()` methods

## Phase 5 - Remove singleton APIs and globally coupled dereference machinery

### Changes

In `crates/ridi-router-routing/src/map_data/graph.rs`:
- delete `MAP_DATA_GRAPH`
- delete `MapDataGraph::init(...)`
- delete `MapDataGraph::get()`
- remove or redesign `MapDataElement::get_from_tiles(...)`
- remove or redesign `MapDataElementRef<T>::get()`
- remove or redesign `ElementTagValueRef::get()`
- remove or redesign `ElementTagSetRef::get()`

### End state

- refs are plain IDs only
- all real data access goes through explicit resolver/context methods
- no runtime path reaches into ambient process state

### Validation

- no `MapDataGraph::get()` remains outside deleted code
- no runtime `.get()`-style implicit dereference remains for map-data refs/tags

## Phase 6 - Rework tests and test helpers

### Changes

In `crates/ridi-router-routing/src/test_utils.rs` and routing tests:
- remove `set_graph_static(...)`
- stop relying on `MAP_DATA_GRAPH`
- build local graph-backed harnesses
- pass resolver/context explicitly in tests

### Recommended test harness shape

A small helper like:

```rust
pub struct RoutingTestContext {
    pub graph: Arc<MapDataGraph>,
}

impl RoutingTestContext {
    pub fn resolver(&self) -> RoutingContext<'_> { ... }
    pub fn point(&self, id: u64) -> MapDataPointRef { ... }
}
```

### Validation

- tests no longer depend on process-global setup
- tests can safely create multiple graphs in one process
- tests are isolated and parallel-safe

## Phase 7 - Cleanup and consolidation

### Cleanup items

- simplify signatures after the conversion settles
- remove migration-only compatibility shims
- audit `Debug` / `Display` impls
- rename the resolver type if needed after usage becomes clear

## Suggested implementation order inside the code

1. make `RoutingExecutor` own `Arc<MapDataGraph>`
2. add `RoutingContext` / `MapResolver`
3. convert `routing_api.rs`
4. convert `generator.rs`
5. convert `walker.rs`
6. convert `navigator.rs`
7. convert `weights.rs`
8. convert `route/*`, `itinerary.rs`, `point.rs`, `line.rs`, `rule.rs`
9. remove singleton APIs and implicit dereference machinery
10. rework tests and test helpers
11. final cleanup

## Risks and watch-outs

### 1. Large mechanical churn

There are many implicit `.get()` dereferences across the crate.

### 2. Formatting helpers can hide regressions

Some `Debug` / `Display` code dereferences refs transitively. Those need explicit cleanup too.

### 3. Internal mutability remains necessary

`TileManager` is still internally mutable because tile loading and caching mutate state. The new architecture should preserve that internal mutability inside the graph.

### 4. Resolver ergonomics matter

If the resolver is too thin, parameter threading becomes noisy. If it becomes too magical, it starts resembling ambient state again. Keep it small and explicit.

### 5. Tests are part of the real work

The refactor is not complete until singleton-shaped test setup is gone.

## Explicit non-goals

Do not merge this refactor with:
- rule loading from tiles
- rules filtering / highway filtering TODOs in `TileManager`
- route output redesign
- event streaming work
- algorithm changes
- unrelated workspace restructuring

## Remaining questions

None at the architecture/planning level. The refactor scope, migration shape, and implementation direction are now clarified enough to execute against this plan.

### Final locked direction

- wrapper name: `RoutingContext`
- borrowed `RoutingContext<'_>`
- pass context into methods instead of storing it in routing objects
- `RoutingExecutor::generate(&self, ...)`
- minimal `Debug` / `Display`
- helper redesign option A: keep domain methods and add context where needed
- internal `RoutingContext` and internal/test-oriented `RoutingExecutor::new(...)`
- `RoutingContext` delegates to low-level `MapDataGraph` lookup primitives
- remove singleton/ref `.get()` compatibility paths as soon as replacements exist
- keep current fail-hard internal lookup assumptions for this refactor
- keep `RoutingContext` thin; no request-local caching in this refactor