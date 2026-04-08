# Implementation Phase 2: add a loaded-only read fast path before the tile-manager write lock

## Goal
Make warm adjacency lookups cheaper by avoiding the tile-manager write lock when already loaded tiles can answer the request.

## Why this is phase 2
Phase 1 attacks repeated calls. Phase 2 attacks the cost of the calls that remain.

The current `MapDataGraph::get_adjacent(...)` always takes `self.tile_manager.write().unwrap()`, even when the needed tiles are already in memory. That is a bad shape for a hot path under Rayon parallelism.

## Scope

### In scope
1. Add a loaded-only adjacency method in `crates/ridi-router-routing/src/rmdf/tile_manager.rs`:
   ```rust
   pub fn get_adjacent_by_id_if_loaded(
       &self,
       tile_id: TileId,
       osm_id: u64,
   ) -> Result<Option<Vec<(TileId, usize, TileId, u64)>>>;
   ```
2. Define exact loaded-only behavior:
   - `Ok(None)` if the center tile is not loaded
   - `Ok(None)` if the answer requires another tile that is not loaded
   - `Ok(Some(...))` only when the full answer can be built from loaded tiles only
3. Change `MapDataGraph::get_adjacent(...)` in `crates/ridi-router-routing/src/map_data/graph.rs` to:
   - take `tile_manager.read()` first
   - try `get_adjacent_by_id_if_loaded(...)`
   - map and return on success
   - fall back to `tile_manager.write()` plus `get_adjacent_by_id(...)` on miss
4. Add tests for warm same-tile and cross-tile adjacency behavior.
5. Add loaded-only path tests that verify:
   - `Ok(None)` when a required tile is not yet loaded
   - `Ok(Some(...))` once both needed tiles are warmed
6. Keep current missing-neighbor behavior unchanged on the fallback path.

### Explicitly out of scope
- route-search API changes
- shared caches
- `SmallVec`
- file-format changes

## Expected code touch points
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- graph cross-tile adjacency tests
- tile-manager synthetic cross-tile tests

## Implementation steps
1. Extract the loaded-only portion of `get_adjacent_by_id(...)` so it can run from `&self` without loading tiles.
2. Reuse the existing loaded-tile helpers and `LoadedTile.point_index` instead of duplicating lookup logic.
3. In the loaded-only path, stop immediately with `Ok(None)` when a required tile is not already loaded.
4. Keep the current write-path implementation as the correctness fallback.
5. Add tests that warm the needed tiles first, then confirm the loaded-only path returns the same adjacency.
6. Add focused loaded-only tests for the boundary cases:
   - center tile missing from the loaded set
   - cross-tile neighbor tile not yet loaded
   - both tiles loaded, returning `Ok(Some(...))`
7. Add at least one test proving graph-level behavior still matches `test_get_adjacent_keeps_cross_tile_behavior`.

## Validation
1. `cargo test -p ridi-router-routing map_data::graph`
2. `cargo test -p ridi-router-routing rmdf::tile_manager`
3. `cargo check --workspace`
4. Profile run:
   ```bash
   RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia
   ```
5. Compare at minimum:
   - `graph::get_adjacent`
   - `tile_manager::get_adjacent_by_id`
   - route wall time
   - any lock-heavy tile-manager helpers that show up in Hotpath
6. Specifically verify the new loaded-only tests cover both `Ok(None)` and `Ok(Some(...))` boundary cases.

## Exit criteria
- warm adjacency lookups no longer require the write lock
- cross-tile behavior stays identical
- the fallback boundary is simple and easy to reason about

## Main risk
Low to medium. The key risk is getting the loaded-only boundary wrong for cross-tile cases and accidentally changing behavior instead of only changing lock shape.
