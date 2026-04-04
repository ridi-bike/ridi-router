# Map Data Refactor - Phase 2: Convert top-level routing flow

_Status: planned_

## Purpose

Move the main route-generation path onto explicit graph/context ownership.

This phase converts the high-level runtime path first:

- `RoutingExecutor::generate(&self, ...)`
- start/finish lookup through `RoutingContext`
- context threading through generator/walker/navigator
- explicit context-aware route output construction
- CLI follow-through where executor API changes require it

## Scope

### In scope

- change `RoutingExecutor::generate` from `&mut self` to `&self`
- create or borrow `RoutingContext` inside `generate(...)`
- resolve closest points through the context
- thread `&RoutingContext` through the main route-generation flow
- convert route output materialization away from hidden `From`-based dereferencing
- update CLI callers for executor API changes

### Out of scope

- full lower-level helper redesign in `weights`, `route/*`, `clustering`, `itinerary`, `point`, `line`, `rule`
- deleting ref/tag `.get()` machinery globally
- full test-harness rewrite

## Primary files

Illustrative, not exhaustive.

- `crates/ridi-router-routing/src/routing_api.rs`
- `crates/ridi-router-routing/src/router/generator.rs`
- `crates/ridi-router-routing/src/router/walker.rs`
- `crates/ridi-router-routing/src/router/navigator.rs`
- `crates/ridi-router-routing/src/route_output.rs`
- `crates/ridi-router-cli/src/router_runner.rs`

## Implementation details

### 1. Make route generation borrow executor state

The executor API should become:

```rust
pub fn generate(&self, request: RouteRequest) -> Result<RouteComputation, RoutingError>
```

Why this matters:

- it reflects explicit ownership instead of mutable singleton orchestration
- it keeps the API shareable in shape
- it avoids implying that route generation needs exclusive executor access

Concurrent routing optimization/validation is still out of scope. This phase is about API ownership shape, not concurrency work.

### 2. Resolve entrypoint lookups through `RoutingContext`

In `routing_api.rs`, replace direct `MapDataGraph::get()` closest-point lookups with context calls.

Expected direction:

- construct `RoutingContext` from `self.graph`
- use `ctx.closest_to_coords(...)` for start/finish lookup
- pass context downstream instead of letting deeper code pull map data implicitly

### 3. Thread context through generator, walker, and navigator

The main runtime path should visibly receive `&RoutingContext` where lookup access is needed.

Preferred style:

- do not store `RoutingContext` permanently in every routing object
- pass `&RoutingContext` into methods that need it
- make lookup needs obvious in method signatures

Examples of what should change in this phase:

- generator start/finish resolution helpers
- walker adjacency lookup
- navigator operations that currently rely on ref `.get()` as hidden graph access

### 4. Make route output materialization explicit and context-aware

The plan explicitly calls out route/output construction as a special case.

Expected direction:

- stop using `From<RouteWithStats>` or similar conversions when they hide lookup work
- add an explicit helper such as `RouteComputation::from_routes(&ctx, routes)`
- keep output construction visibly context-aware

This keeps route materialization aligned with the rest of the architecture: lookups are explicit, not ambient.

### 5. Update CLI integration in the same phase

Any executor API change that affects the CLI must be carried through here.

Likely changes:

- adjust `router_runner.rs` to the new open/generate expectations
- remove any assumptions about process-global graph lifetime or open order
- avoid reintroducing hidden singleton behavior in CLI glue code

## Dependencies and handoff to the next phase

This phase is complete when the main runtime path is visibly context-driven, even if some deeper helper methods still use implicit dereferencing internally.

Phase 3 should be able to assume:

- top-level route generation does not depend on `MapDataGraph::get()`
- the remaining architectural work is concentrated in lower-level helpers and domain methods

## Acceptance criteria

- `RoutingExecutor::generate(&self, ...)` is implemented
- `routing_api.rs` resolves start/finish via `RoutingContext`
- generator/walker/navigator receive `&RoutingContext` in the main runtime flow where lookups happen
- the runtime generator/walker path no longer calls `MapDataGraph::get()`
- route output construction no longer depends on ref `.get()` hidden inside `From` impls
- CLI integration matches the updated executor API and ownership model
- `RoutingContext` is visibly threaded through the main route-generation flow

## Validation checklist

- [x] `RoutingExecutor::generate` takes `&self`
- [x] start/finish lookup uses `RoutingContext`
- [x] generator uses context for nearest-point/lookup operations
- [x] walker uses context for adjacency lookup
- [x] navigator call paths accept/pass `&RoutingContext` where lookup access is needed
- [x] `route_output.rs` uses an explicit context-aware construction path
- [x] CLI caller compiles against the new executor API
- [x] no top-level runtime path depends on singleton open order

### Suggested checks

- [ ] `rg "MapDataGraph::get\(" crates/ridi-router-routing/src/routing_api.rs crates/ridi-router-routing/src/router/generator.rs crates/ridi-router-routing/src/router/walker.rs crates/ridi-router-routing/src/router/navigator.rs crates/ridi-router-routing/src/route_output.rs crates/ridi-router-cli/src/router_runner.rs`
- [x] targeted route-generation tests still pass on the converted path
- [x] `cargo test -p ridi-router-routing`
- [x] `cargo test -p ridi-router-cli`

## Progress checklist

- [x] change `RoutingExecutor::generate` to `&self`
- [x] construct `RoutingContext` inside `generate(...)`
- [x] replace entrypoint closest-point lookups with context calls
- [x] thread `&RoutingContext` through generator
- [x] thread `&RoutingContext` through walker
- [x] thread `&RoutingContext` through navigator
- [x] move route output materialization to explicit context-aware helper(s)
- [x] update CLI integration
- [x] confirm phase acceptance criteria are met

## Notes and watch-outs

- Keep the threading explicit. Do not replace singleton access with a different hidden dependency.
- It is acceptable if some lower-level helpers still need later conversion; this phase is about the top-level flow first.
- Be careful with `From` impls and formatting helpers. They can hide ref lookups and make the phase look more complete than it really is.
