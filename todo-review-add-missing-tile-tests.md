# Review: missing-tile behavior tests

## Verdict
Relevant

## Summary
This todo is still valid.

The codebase currently implements a missing-neighbor fallback in `TileManager`: when adjacency crosses into another tile and that tile cannot be loaded, the line is skipped and treated like a dead-end. That behavior is real, but it is not locked in by meaningful tests.

There is even a placeholder unit test for this exact case, but it contains no assertions and therefore passes without testing anything. The todo text is basically correct, although it understates one important detail: the current routing stack does not report a special "missing tile" error. Instead, it relies on adjacency shrinking and the normal dead-end / no-route flow.

## Current evidence from the codebase

- The missing-neighbor behavior is implemented in `crates/ridi-router-routing/src/rmdf/tile_manager.rs`.
  - `TileManager::get_adjacent_by_id(...)` computes the other endpoint's tile and, if it differs, tries to load that tile.
  - If loading fails, it logs a warning and `continue`s, which drops that edge from adjacency:
    - `crates/ridi-router-routing/src/rmdf/tile_manager.rs:223-237`
- The intended semantics are explicitly documented in code comments there:
  - `// Tile missing - skip this line (dead-end)`
  - `crates/ridi-router-routing/src/rmdf/tile_manager.rs:231-236`
- `MapDataGraph::get_adjacent(...)` depends directly on `TileManager::get_adjacent_by_id(...)`, so routing inherits that filtered adjacency behavior:
  - `crates/ridi-router-routing/src/map_data/graph.rs:442-481`
- The walker already treats "no available next segment" as a normal dead-end, not a panic:
  - `crates/ridi-router-routing/src/router/walker.rs:293-327`
- The navigator already handles `WalkerMoveResult::DeadEnd` by backtracking instead of crashing:
  - `crates/ridi-router-routing/src/router/navigator.rs:280-288`
- There is a direct TODO-shaped gap in test coverage:
  - `crates/ridi-router-routing/src/rmdf/tile_manager.rs:361-365` contains `test_missing_tile_handling()` with only comments and no assertions.
- That empty test currently passes vacuously. Running
  - `cargo test -p ridi-router-routing test_missing_tile_handling -- --nocapture`
  reports the test as passing even though it does not exercise behavior.
- Existing synthetic routing fixtures are single-tile fixtures, so they do not cover missing adjacent tiles:
  - `crates/ridi-router-routing/src/routing_api.rs:288-389`
  - `crates/ridi-router-cli/tests/generate_route_cli.rs:173-333`
- Existing CLI tests cover missing `manifest.json` / invalid tiles dir, but not missing adjacent neighbor tiles during traversal:
  - `crates/ridi-router-cli/tests/generate_route_cli.rs:398-467`

## The actual problem

The missing behavior is not the runtime fallback itself; that already exists.

The real problem is that the fallback is unverified. A future refactor could easily change `get_adjacent_by_id(...)` from "skip missing neighbor edge" to "bubble up error" or "panic via unwrap/expect in a new path," and the current test suite would not catch it.

## What behavior is missing or risky

1. **Unit-level adjacency proof is missing.**
   There is no test showing that a cross-tile edge is removed from adjacency when the destination tile file is absent.

2. **Routing-level behavior is not explicitly locked in.**
   The todo says route generation should not panic and should behave intentionally. That is still unproven for the missing-neighbor case.

3. **The current todo text is slightly stale/incomplete.**
   It says the code "tries to treat missing neighbor tiles as dead-ends," which is true, but the practical mechanism is narrower: `TileManager` filters out those edges, and then normal dead-end handling in walker/navigation takes over. There is no dedicated missing-tile routing error path here.

## Possible solution options

### Option A: Add focused `TileManager` unit tests
Create a tiny two-tile synthetic fixture where one line from tile A points to a point in tile B, then omit tile B from disk.

Test that:
- `get_adjacent_by_id(...)` returns the in-tile neighbors
- the cross-tile edge is absent
- the call succeeds without error

### Option B: Add a routing-level synthetic test in `routing_api.rs`
Build a synthetic fixture that requires or attempts a cross-tile step, then remove the neighboring tile.

Test that:
- route generation does not panic
- the result is either no route or a route that avoids the missing edge, depending on fixture design

### Option C: Do both
Add one unit test in `tile_manager.rs` and one integration-style test in `routing_api.rs` or CLI tests.

## Tradeoffs / implications

- **Unit-only tests** are simpler and directly verify the fallback logic, but they do not prove end-to-end routing behavior.
- **Routing-level tests** better match user-visible behavior, but they are harder to design because the fixture must make the intended outcome unambiguous.
- **Both layers** give the best protection and make future refactors safer.

## Recommended direction

Do both, but start with the unit test in `crates/ridi-router-routing/src/rmdf/tile_manager.rs`.

That is the narrowest place where the intended contract is implemented. Once that is in place, add one routing-level test only if the fixture can clearly show the expected result:
- either "no route found, but no crash"
- or "detour route succeeds without the missing edge"

Given the current code, the most realistic guaranteed behavior is probably the first one unless a detour fixture is deliberately constructed.

## Open questions

1. Should the expected routing outcome be **"no route"** or **"detour succeeds"**?
   - The answer depends entirely on fixture design.
2. Is warning logging part of the contract?
   - The code emits a warning in `TileManager`, but current tests do not capture tracing output.
3. Should this be tested only in `ridi-router-routing`, or also in CLI tests?
   - If the goal is behavioral lock-in, routing crate tests are probably enough first.

## Suggested implementation starting point

1. Start in `crates/ridi-router-routing/src/rmdf/tile_manager.rs`.
2. Replace the empty `test_missing_tile_handling()` with a real synthetic fixture-based test.
3. Reuse the existing synthetic RMDF-writing approach already present in:
   - `crates/ridi-router-routing/src/routing_api.rs:288-389`
   - `crates/ridi-router-cli/tests/generate_route_cli.rs:173-333`
4. Build a minimal fixture with:
   - tile A on disk
   - manifest listing tile A and tile B
   - a border-crossing line in tile A whose other endpoint lands in tile B
   - tile B intentionally absent from disk
5. Assert that `get_adjacent_by_id(...)` succeeds and excludes the missing-tile edge.
6. Then decide whether to add a second routing-level test using the same fixture pattern.

## Related files to inspect next

- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/router/walker.rs`
- `crates/ridi-router-routing/src/router/navigator.rs`
- `crates/ridi-router-routing/src/routing_api.rs`
- `crates/ridi-router-cli/tests/generate_route_cli.rs`
- `thoughts/plans/rmdf_memory_mapped_tiles/06_tile_manager_multi.md`
