# Review: 02-routing-border-point-lookup-plan

## Verdict

Implemented correctly overall.

I did not find any blocking issues in the staged border-point lookup behavior. The core change matches the plan, and the focused behavior is covered by tests.

## What matches the plan

### Staged endpoint resolution exists

`TileManager` now has a private staged helper at `crates/ridi-router-routing/src/rmdf/tile_manager.rs:733-780`.

It does the expected sequence:
1. probe current tile first
2. short-circuit when the derived tile is still the current tile
3. keep coordinate-derived neighbor selection for stage two
4. distinguish missing-manifest neighbor vs cold loaded neighbor vs loud invariant failure

### Both adjacency paths use the staged logic

Both callers route through the helper:
- `get_adjacent_by_id_if_loaded(...)` at `crates/ridi-router-routing/src/rmdf/tile_manager.rs:786-839`
- `get_adjacent_by_id(...)` at `crates/ridi-router-routing/src/rmdf/tile_manager.rs:842-903`

Behavior matches the plan:
- loaded-only path returns `Ok(None)` for cold cross-tile continuation
- loading path only loads the neighbor on the second stage
- missing-manifest neighbor still gets filtered as a dead-end
- returned `MapDataPointRef` uses the tile where the point was actually found

### Loud failure is preserved

The invariant failure is still loud when the endpoint is missing in both current and derived neighbor tiles:
- helper failure message at `crates/ridi-router-routing/src/rmdf/tile_manager.rs:778-780`
- loading-path failure after neighbor load at `crates/ridi-router-routing/src/rmdf/tile_manager.rs:889-891`

### Graph and routing layers preserve the intended flow

`MapDataGraph::get_adjacent(...)` still tries the loaded-only path first, then falls back to the loading path at `crates/ridi-router-routing/src/map_data/graph.rs:500-515`.

`RoutingContext` behavior remains compatible, and the overlap case is exercised in tests at `crates/ridi-router-routing/src/routing_context.rs:553-576`.

### Test coverage is strong

I found direct coverage for the main planned cases:
- current-tile duplicate wins
- cold neighbor returns `None` in loaded-only path
- warm cross-tile lookup still works
- missing in both tiles still fails loudly
- graph-level current-first behavior
- routing-context cached adjacency preserves current-tile overlap

Fixtures supporting this were added in `crates/ridi-router-test-support/src/lib.rs:85-92` and `:444-604`.

## Minor deviations / gaps

### Helper signature differs from the plan

The plan suggested a helper that accepts a "whether neighbor loading is allowed" flag.

The implementation does not pass that flag directly. Instead, the helper returns:
- `Resolved(...)`
- `ColdNeighbor(...)`
- `MissingManifestNeighbor(...)`

and each caller decides what to do.

This is a design variation, not a problem. It preserves the intended behavior cleanly.

### `test_utils.rs` was listed as a touchpoint, but the feature does not appear to depend on it

I did not find feature-specific border-overlap changes there. That is harmless, but it does not match the touchpoint list in the plan.

### The planned end-to-end "short border excursion" coverage is only partially represented

The current tests prove the important part: duplicated border endpoints can resolve in the current tile without loading the neighbor.

What I did not find is a higher-level route/integration test that exercises that exact scenario through a fuller routing flow. The existing coverage is still good, but this specific plan item looks only partially realized.

## Validation

Ran:
- `cargo test -q -p ridi-router-routing`

Result:
- `161 passed`
- `0 failed`
- `2 ignored`

## Final assessment

Status: **plan mostly fully implemented**.

No blocking findings. The only notable items are a small helper-shape deviation and a possible missing higher-level integration test for the "short border-crossing-returning" scenario.