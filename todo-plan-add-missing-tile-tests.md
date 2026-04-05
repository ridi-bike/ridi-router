# Plan: add missing-tile tests

This file is the source of truth for this todo.

## Reviewed decisions

- Keep current behavior.
  - Missing neighbor tiles are treated as dead-ends by filtering out the cross-tile edge.
  - Do not introduce a dedicated routing-level missing-tile error as part of this todo.
- Test scope for this todo: unit test only.
  - Target file: `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- Expected locked-in outcome:
  - `TileManager::get_adjacent_by_id(...)` succeeds even when a neighboring tile file is missing.
  - The cross-tile edge is omitted from the returned adjacency.
- Warning logging is not part of the contract.
  - Do not assert on tracing output.
- Include fixture cleanup work.
  - Prefer a broader shared synthetic RMDF test helper that can be reused by both routing-crate tests and CLI tests.

## Current code reality

Confirmed in the codebase:

- `TileManager::get_adjacent_by_id(...)` computes the other endpoint tile from coordinates and tries to load it.
- If the tile load fails, it logs a warning and `continue`s.
- That means the edge is dropped from adjacency and the caller sees a normal dead-end shape.
- The current placeholder test in `crates/ridi-router-routing/src/rmdf/tile_manager.rs` does not assert anything and passes vacuously.
- The current routing stack does not have a dedicated missing-tile routing error path for this case.

Relevant files inspected:

- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/router/walker.rs`
- `crates/ridi-router-routing/src/router/navigator.rs`
- `crates/ridi-router-routing/src/routing_api.rs`
- `crates/ridi-router-cli/tests/generate_route_cli.rs`
- `thoughts/plans/rmdf_memory_mapped_tiles/06_tile_manager_multi.md`

## Implementation scenarios considered

### Scenario 1: Minimal local test only
Add one fixture directly inside `tile_manager.rs` and replace the empty test.

Pros:
- Smallest code change
- Fastest path to real coverage

Cons:
- Leaves existing synthetic RMDF fixture-writing duplication in place
- Makes later routing/CLI tests more likely to copy more code

### Scenario 2: Shared test fixture support + unit test
Introduce reusable synthetic RMDF fixture helpers, then use them from the new `TileManager` test.

Pros:
- Fixes the immediate missing test
- Reduces duplication already present in routing and CLI tests
- Makes future missing-tile routing/CLI tests easier without redoing fixture code

Cons:
- Slightly broader change than a one-off unit test

### Scenario 3: Add routing/CLI tests now
Rejected for this todo because the agreed scope is unit-only.

## Chosen approach

Use **Scenario 2**.

Implement a real `TileManager` missing-tile unit test, but do it on top of shared synthetic RMDF test helpers instead of adding yet another one-off fixture writer.

## Recommended structure for shared fixture support

Use a dedicated workspace test-support crate rather than trying to share `#[cfg(test)]` modules across crates.

### Why this structure

The existing synthetic RMDF helpers live in:

- `crates/ridi-router-routing/src/routing_api.rs`
- `crates/ridi-router-cli/tests/generate_route_cli.rs`

Those helpers are not reusable across crates in their current form. A dedicated test-support crate is the cleanest shared path.

### Proposed crate

- New crate: `crates/ridi-router-test-support`

### Proposed consumers

- `crates/ridi-router-routing` as a dev-dependency
- `crates/ridi-router-cli` as a dev-dependency

### Proposed contents

Keep it small and focused on synthetic RMDF test data creation:

- temporary test-dir helper
- low-level RMDF binary writing helpers
- manifest writer
- small fixture builders for common patterns

Suggested API shape:

- `unique_test_dir(prefix: &str) -> PathBuf`
- `write_manifest(dir, manifest_spec)`
- `write_tile(dir, tile_id, tile_spec)`
- `create_linear_single_tile_fixture(...)`
- `create_missing_neighbor_fixture(...)`

The goal is not to build a general RMDF authoring library. The goal is to centralize the small amount of synthetic binary-writing code already duplicated in tests.

## Missing-neighbor fixture design

Build a minimal synthetic fixture for the exact contract being tested.

### Tile layout

- tile size: `1.0`
- tile A exists on disk: `tile_200_100.rmdf`
- tile B is intentionally missing from disk: `tile_201_100.rmdf`
- manifest should include both tile metadata entries for realism and future-proofing

### Graph shape inside tile A

Create one center point in tile A with two incident lines:

1. **In-tile line**
   - from center point in tile A
   - to another point still inside tile A
   - this edge should remain in adjacency

2. **Cross-tile line**
   - from the same center point in tile A
   - to a point whose coordinates place it in tile B
   - tile B file is absent
   - this edge should be filtered out

### Important detail

The test should prove both sides of the contract:

- the call does not error
- valid in-tile adjacency survives while the missing-tile adjacency is removed

That is stronger than asserting only `len() == 0`.

## Concrete test to add

Replace the empty test in:

- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

with a real test equivalent to:

1. create synthetic missing-neighbor fixture
2. initialize `TileManager`
3. query `get_adjacent_by_id(tile_a, center_osm_id)`
4. assert the call returns `Ok(...)`
5. assert the result contains the in-tile neighbor tuple
6. assert the result does not contain any tuple pointing to tile B
7. assert the returned adjacency count matches the expected filtered result

## Suggested work steps

1. Add `crates/ridi-router-test-support` to the workspace.
2. Move or re-home the duplicated synthetic RMDF writer logic into that crate.
3. Add a focused missing-neighbor fixture builder.
4. Update `crates/ridi-router-routing/Cargo.toml` to use the new crate as a dev-dependency.
5. Replace `test_missing_tile_handling()` in `crates/ridi-router-routing/src/rmdf/tile_manager.rs`.
6. Keep the test narrowly focused on adjacency filtering behavior.
7. Do not add routing-level or CLI missing-tile assertions in this todo.

## Validation

Primary check:

```bash
cargo test -p ridi-router-routing test_missing_tile_handling -- --nocapture
```

Recommended follow-up check after helper extraction:

```bash
cargo test -p ridi-router-routing
cargo test -p ridi-router-cli
```

## Non-goals

- No dedicated missing-tile routing error
- No routing-level detour test
- No CLI missing-tile test
- No log-capture assertions
- No change to runtime missing-tile behavior

## Done criteria

This todo is done when all of the following are true:

- `test_missing_tile_handling()` has real assertions
- the test uses a fixture where a cross-tile edge points into a missing tile
- `get_adjacent_by_id(...)` returns success
- the in-tile edge remains present
- the missing-tile edge is absent
- synthetic RMDF fixture creation is on a shared path reusable by routing and CLI tests
