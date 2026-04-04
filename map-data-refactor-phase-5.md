# Map Data Refactor - Phase 5: Rework tests and final consolidation

_Status: planned_

## Purpose

Finish the refactor by removing singleton-shaped testing, consolidating the converted architecture, and doing the final cleanup pass.

This phase combines the plan's original final work:

- original phase 6: test and test-helper rewrite
- original phase 7: cleanup and consolidation

## Scope

### In scope

- remove singleton-based test setup
- replace `set_graph_static(...)` usage with local graph-backed harnesses
- make tests pass explicit graph/context state
- ensure tests can safely use multiple graphs in one process
- remove remaining migration scaffolding
- simplify signatures where temporary transition shapes are no longer needed
- audit `Debug` / `Display` for minimal behavior
- perform final validation across routing and CLI integration

### Out of scope

- unrelated workspace restructuring
- algorithm changes
- concurrent route-generation optimization/validation
- tile TODO completion

## Primary files

Illustrative, not exhaustive.

- `crates/ridi-router-routing/src/test_utils.rs`
- routing tests across `crates/ridi-router-routing/src/**`
- `crates/ridi-router-routing/src/routing_api.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/route_output.rs`
- `crates/ridi-router-cli/src/router_runner.rs`

## Implementation details

### 1. Replace process-global test setup with local harnesses

The old pattern of singleton setup is no longer acceptable once runtime code is instance-scoped.

Expected direction:

- remove `set_graph_static(...)`
- stop relying on `MAP_DATA_GRAPH` in tests
- build graph-backed test contexts locally per test or per fixture

Recommended harness shape:

```rust
pub struct RoutingTestContext {
    pub graph: Arc<MapDataGraph>,
}

impl RoutingTestContext {
    pub fn resolver(&self) -> RoutingContext<'_> { ... }
    pub fn point(&self, id: u64) -> MapDataPointRef { ... }
}
```

The exact helper surface can vary, but the key requirement is local ownership plus explicit context access.

### 2. Rework tests to match the explicit-lookup architecture

Tests should follow the same architecture as production code.

That means:

- if runtime code now uses `RoutingContext`, tests should too
- if helper APIs now require resolved values or explicit context, tests should stop calling old singleton-shaped helpers
- multi-graph tests should be possible without process-global setup interference

### 3. Consolidate temporary migration code

After earlier phases, there may still be leftover transition artifacts.

Clean up items include:

- temporary constructors or wrappers that were only needed mid-conversion
- compatibility imports and helper functions
- overly noisy method signatures that can now be simplified after the final architecture settles
- comments or TODOs describing now-removed migration paths

### 4. Audit formatting and debugging behavior

The locked plan direction is minimal `Debug` / `Display`.

Use this phase to ensure:

- formatting does not trigger context-dependent lookup work
- debug output is useful but shallow
- no hidden graph access survives in convenience formatting code

### 5. Run final end-state validation

Unlike earlier phases, this final phase should leave the whole refactor coherent.

Expected finish line:

- routing code is explicit and instance-scoped
- tests are isolated and parallel-safe
- CLI integration matches the final executor API
- no singleton-shaped assumptions remain in runtime or tests

## Acceptance criteria

- tests no longer depend on process-global graph setup
- `set_graph_static(...)` is removed or no longer used
- tests can create multiple graphs in one process safely
- tests are isolated and parallel-safe in architecture
- migration-only scaffolding is removed
- `Debug` / `Display` implementations are minimal and do not hide context-dependent lookup
- final routing and CLI code match the explicit executor/context model
- full intended validation passes for the refactor end state

## Validation checklist

- [ ] `test_utils.rs` no longer exposes singleton-based setup as the standard test path
- [ ] routing tests build local graph-backed harnesses
- [ ] tests can create more than one graph in a single process without hidden coupling
- [ ] no test still relies on singleton open ordering
- [ ] leftover migration helpers/shims are removed
- [ ] `Debug` / `Display` impls are minimal and free of hidden lookup work
- [ ] routing crate tests pass
- [ ] CLI crate tests pass
- [ ] workspace-level validation for the refactor path passes

### Suggested checks

- [ ] `rg "set_graph_static|MAP_DATA_GRAPH|MapDataGraph::get\(" crates/ridi-router-routing/src`
- [ ] `cargo test -p ridi-router-routing`
- [ ] `cargo test -p ridi-router-cli`
- [ ] `cargo test --workspace`
- [ ] optional targeted parallel test run if applicable to the harness design

## Progress checklist

- [ ] design and add local `RoutingTestContext`-style harness helpers
- [ ] remove singleton-based test utilities
- [ ] convert routing tests to local graph/context setup
- [ ] add or update multi-graph tests
- [ ] remove leftover migration scaffolding
- [ ] simplify signatures after the conversion settles
- [ ] audit `Debug` / `Display`
- [ ] run final routing + CLI + workspace validation
- [ ] confirm phase acceptance criteria are met

## Notes and watch-outs

- The refactor is not done until tests stop depending on singleton-shaped setup.
- Avoid leaving old test helpers around “just in case”; they become architecture backdoors.
- Keep cleanup tied to explicit acceptance criteria so this final phase does not turn into vague polish work.
