# Map Data Refactor Plan

_Status: draft updated after review pass_

This plan is based on:
- `map-data-refactor-discoveries.md`
- code inspection in `crates/ridi-router-routing`
- CLI integration in `crates/ridi-router-cli`

## Confirmed decisions

These are now settled:

1. **Ref model**: use **lightweight ID refs + explicit context**
2. **Multi-dataset support**: **required**
3. **Public API stability**: **not required** on this branch
4. **Migration style**: several validated phases; phases do **not** need to be independently shippable, and intermediate compile/test breakage is acceptable while the conversion is in flight as long as each phase still has concrete, scoped acceptance criteria for the parts it intentionally changes
5. **Naming**: keep **`MapDataGraph`**
6. **Tests**: rework singleton-based tests in the **same refactor**
7. **Tile TODOs**: explicitly **out of scope**
8. **Sharing**: `RoutingExecutor` / `RoutingContext` should be **shareable in the API sense**, but optimizing or validating **concurrent** route generation is out of scope in this refactor
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
22. **Phase file lists**: "Primary files" lists in the phases are **illustrative**, not exhaustive
23. **Cross-dataset ref/context mismatch**: resolving a ref through the wrong graph/context is an internal bug to call out, but adding protection mechanisms for that case is out of scope in this refactor
24. **Route output materialization**: move context-dependent route/output construction out of `From` impls and make it **explicitly context-aware**
25. **CLI follow-through**: update CLI/consumer integration in the same refactor anywhere executor API changes affect it

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

Large parts of the crate rely on `.get()` on point, line, and tag refs. Illustrative runtime examples include:
- `router/generator.rs`
- `router/walker.rs`
- `router/navigator.rs`
- `router/weights.rs`
- `router/route/*` (including `score.rs`)
- `router/clustering.rs`
- `router/itinerary.rs`
- `route_output.rs`
- `map_data/point.rs`
- `map_data/line.rs`
- `map_data/rule.rs`

So this is a broad internal refactor, not a small patch, and the file lists below should be read as examples rather than exhaustive inventories.

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
- resolving a ref through the wrong graph/context is treated as an internal bug in this refactor, not a case that gets new runtime guards

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

impl RouteComputation {
    fn from_routes(ctx: &RoutingContext<'_>, routes: Vec<RouteWithStats>) -> Self { ... }
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

### 5. Treat wrong-context ref resolution as an internal bug for now

Multiple datasets make it possible to resolve a ref from graph A through graph B. Call that risk out in implementation notes and code review, but keep it as a trusted internal invariant in this refactor.

### 6. Make context-dependent output construction explicit

Context-dependent materialization such as route-output coordinate extraction should not hide behind `From` impls. Build those results in explicit helpers that receive `&RoutingContext`.

## Phase validation scope

Phase validation is scoped to the area intentionally converted in that phase. Until the final cleanup end state, temporary compilation failures or test failures in not-yet-converted code are acceptable.
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

### Primary files (illustrative)

- `crates/ridi-router-routing/src/routing_api.rs`
- `crates/ridi-router-routing/src/router/generator.rs`
- `crates/ridi-router-routing/src/router/walker.rs`
- `crates/ridi-router-routing/src/router/navigator.rs`
- `crates/ridi-router-routing/src/route_output.rs`
- `crates/ridi-router-cli/src/router_runner.rs`

### Changes

- `RoutingExecutor::generate(&self, ...)` creates/borrows a `RoutingContext`
- generator resolves closest points through the context
- walker resolves adjacency through the context
- navigator passes context access into deeper operations instead of depending on implicit ref `.get()` behavior
- route output materialization moves out of `From<RouteWithStats>` and into an explicit context-aware helper such as `RouteComputation::from_routes(&ctx, ...)`
- CLI callers are updated where the executor API or mutability expectations change

### Validation

- runtime generator/walker paths no longer call `MapDataGraph::get()`
- `RoutingContext` is visibly threaded through the main route-generation flow
- route output construction no longer depends on ref `.get()` hidden inside `From` impls
- CLI integration matches the updated executor API

## Phase 4 - Convert lower-level domain helpers away from implicit ref `.get()`

### Primary files (illustrative)

- `crates/ridi-router-routing/src/router/weights.rs`
- `crates/ridi-router-routing/src/router/route/mod.rs`
- `crates/ridi-router-routing/src/router/route/segment.rs`
- `crates/ridi-router-routing/src/router/route/score.rs`
- `crates/ridi-router-routing/src/router/clustering.rs`
- `crates/ridi-router-routing/src/router/itinerary.rs`
- `crates/ridi-router-routing/src/map_data/point.rs`
- `crates/ridi-router-routing/src/map_data/line.rs`
- `crates/ridi-router-routing/src/map_data/rule.rs`

### What changes here

This is where implicit deref-heavy helpers need redesign.

This phase is **not** just call-site replacement. It includes changing method signatures and helper responsibilities wherever apparently-owned domain methods still dereference refs internally.

Examples:
- methods that currently do `point_ref.get().lat`
- methods that currently do `line_ref.get().tags.get().highway()`
- methods like `MapDataPoint::distance_between(&MapDataPointRef)` and `MapDataPoint::bearing(&MapDataPointRef)`
- methods like `MapDataLine::line_id()` / `MapDataLine::get_len_m()`
- methods like `Segment::get_bearing()`
- formatting helpers that dereference refs transitively

### Preferred direction

Where a helper is really a lookup helper, move that logic to the context.

Where a helper is really pure geometry, scoring, clustering, or formatting, make it operate on already-resolved owned values or plain coordinates.

Examples:
- geometry helpers should prefer coordinates or resolved values instead of ref dereferencing
- tag-based stats helpers should use context methods instead of nested `.get()` chains
- converted helpers should either take `&RoutingContext` explicitly or stop doing lookups internally altogether

### Validation

- runtime routing code no longer depends on `MapDataElementRef<T>::get()`
- runtime routing code no longer depends on tag ref `.get()` methods
- converted helper APIs make lookup requirements explicit instead of hiding them inside owned domain methods

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
- clean up temporary scaffolding introduced to support partially converted phases
## Suggested implementation order inside the code

1. make `RoutingExecutor` own `Arc<MapDataGraph>`
2. add `RoutingContext`
3. convert `routing_api.rs`
4. convert `generator.rs`
5. convert `walker.rs`
6. convert `navigator.rs`
7. convert route output construction and CLI callers
8. convert `weights.rs`
9. convert `route/*`, `clustering.rs`, `itinerary.rs`, `point.rs`, `line.rs`, `rule.rs`
10. remove singleton APIs and implicit dereference machinery
11. rework tests and test helpers
12. final cleanup

## Risks and watch-outs

### 1. Large mechanical churn

There are many implicit `.get()` dereferences across the crate.

### 2. Formatting helpers can hide regressions

Some `Debug` / `Display` code dereferences refs transitively. Those need explicit cleanup too.

### 3. Internal mutability remains necessary

`TileManager` is still internally mutable because tile loading and caching mutate state. The new architecture should preserve that internal mutability inside the graph.

### 4. Resolver ergonomics matter

If the resolver is too thin, parameter threading becomes noisy. If it becomes too magical, it starts resembling ambient state again. Keep it small and explicit.

### 5. Multiple datasets introduce wrong-context lookup risk

With multiple graphs, resolving a ref against the wrong `RoutingContext` is now possible. Treat that as an internal invariant for this refactor and call it out clearly during implementation and review, but do not add extra guard mechanisms here.

### 6. Shareable API does not mean concurrent routing is a goal

`RoutingExecutor::generate(&self, ...)` should not require exclusive access, but concurrent route generation behavior/performance is out of scope in this refactor.

### 7. Tests are part of the real work

The refactor is not complete until singleton-shaped test setup is gone.
## Explicit non-goals

Do not merge this refactor with:
- rule loading from tiles
- rules filtering / highway filtering TODOs in `TileManager`
- route output schema redesign
- event streaming work
- algorithm changes
- concurrent route-generation optimization/validation
- unrelated workspace restructuring

## Remaining questions

None that block execution. For clarity, these are now treated as settled planning assumptions:

- phase file lists are illustrative, not exhaustive
- resolving a ref against the wrong graph/context is an internal bug, and adding protection for that case is out of scope here
- `RoutingExecutor::generate(&self, ...)` is about explicit ownership/shareable API shape; concurrent route generation is out of scope
- CLI integration updates are part of this refactor wherever executor API changes affect them
- temporary compilation or test breakage in not-yet-converted areas is acceptable between phases as long as scoped acceptance criteria are still defined and followed

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
- keep phase file lists illustrative rather than exhaustive
- move context-dependent route/output construction out of `From` impls
- include CLI follow-through where executor API changes require it
- accept temporarily broken intermediate states, with validation scoped to the converted area