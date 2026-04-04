# TODO: Runtime tile behavior follow-up

## Goal

Finish the remaining runtime tile-backed behavior that was explicitly left out of the map-data architecture refactor.

This doc covers the TODOs that still affect lookup quality and semantic correctness in the routing runtime.

## Files

- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`

## Current TODOs

### In `tile_manager.rs`

- expanding-ring nearest search is not implemented
- rules filtering is not implemented
- highway tag filtering is not implemented
- cache eviction is not true LRU yet
- missing-tile scenario still needs explicit test coverage

### In `map_data/graph.rs`

- `get_point_from_tiles(...)` still returns `rules: Vec::new()` instead of loading point rules from tile data

## Why this matters

The architecture is now explicit and instance-scoped, but some runtime lookup behavior is still incomplete.

That means the code can be structurally correct while still being semantically weaker than intended in areas like:

- choosing the best nearest point
- respecting route rules during closest-point lookup
- filtering by highway constraints
- honoring turn restrictions or point-level rules loaded from tiles
- predictable tile-cache behavior under larger workloads

## Split the work into small follow-ups

### A. Load point rules from tiles

#### Problem

`MapDataGraph::get_point_from_tiles(...)` currently constructs `MapDataPoint` with:

- `rules: Vec::new()`

That means rule-aware runtime logic only works fully in test/in-memory paths, not in real tile-backed point loading.

#### Plan

- inspect RMDF point/rule storage that already exists or is intended to exist
- add tile-manager accessors for point rule lookup
- materialize `Vec<MapDataRule>` in `get_point_from_tiles(...)`
- verify walkers/navigators receive real rule data from tiles, not empty vectors

#### Acceptance

- tile-backed points expose real `MapDataRule` values
- runtime rule logic works for tile-backed routing, not only tests

---

### B. Implement nearest-point filtering semantics

#### Problem

`TileManager::get_closest_to_coords(...)` still has TODOs for:

- expanding ring search
- rules filtering
- highway tag filtering

The current path falls back to a simple scan and does not fully apply the intended filters.

#### Plan

1. replace the current one-cell/linear-scan approach with an expanding search strategy
2. define clear filtering order:
   - skip disconnected points
   - apply residential/nogo filters as needed
   - apply rule-based eligibility
   - apply optional highway tag restrictions
3. return the nearest candidate that survives filtering
4. keep the behavior deterministic and testable

#### Acceptance

- closest-point lookup respects `RouterRules`
- closest-point lookup respects `limit_to_hw_tags`
- search quality does not depend on luck within a single scanned bucket
- targeted tests cover filtering combinations

---

### C. Implement real tile-cache eviction behavior

#### Problem

`evict_if_needed()` currently removes an arbitrary tile rather than the least-recently-used tile.

That is acceptable for small tests but not ideal for predictable routing behavior on larger datasets.

#### Plan

- add last-access tracking to tile cache state
- update access metadata on load and read
- evict the true least-recently-used tile once above the limit
- keep the implementation simple first; correctness matters more than micro-optimization

#### Acceptance

- eviction order is deterministic
- hot tiles are retained under repeated access
- unit tests cover at least one overflow/eviction case

---

### D. Add missing-tile behavior tests

#### Problem

There is still a TODO for explicitly testing the case where an adjacent tile is missing.

The code currently tries to treat that as a dead-end, which is a reasonable policy, but it should be locked in by tests.

#### Plan

- add a fixture with a cross-tile edge where the neighbor tile is absent
- verify adjacency lookup treats the edge as unavailable instead of panicking
- verify route generation degrades safely

#### Acceptance

- missing neighbor tile behavior is covered by tests
- behavior is intentional and documented

## Suggested implementation order

1. missing-tile test coverage
2. point rule loading from tiles
3. highway/rule filtering in closest-point lookup
4. expanding search strategy
5. true LRU eviction

## Acceptance criteria

- `MapDataPoint.rules` is populated from tile-backed data
- `TileManager::get_closest_to_coords(...)` applies intended filters
- nearest-point lookup uses a better-than-current search strategy
- tile eviction is LRU rather than arbitrary
- missing-tile behavior is tested
- routing crate tests still pass

## Suggested checks

- `cargo test -p ridi-router-routing`
- targeted tests for nearest-point filtering
- targeted tests for point-rule loading
- targeted tests for missing-tile cross-border adjacency
- optional benchmark/trace check for lookup behavior on larger fixtures

## Non-goals

- changing the explicit executor/context architecture
- adding wrong-context runtime guards
- broad routing algorithm redesign
