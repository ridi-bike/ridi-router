# Implementation Phase 3: reduce adjacency allocation churn with local `SmallVec` cleanup

## Goal
Shrink heap churn in the adjacency path without broad API redesign.

## Why this is phase 3
After phases 1 and 2, we should know how much adjacency call volume remains. This phase then targets the still-visible allocation cost called out in `perf-plan.md`:
- `graph::get_adjacent`: 4.6 GB alloc
- `tile_manager::get_adjacent_by_id`: 3.3 GB alloc

This phase should stay local and measurable.

## Scope

### In scope
1. Add `smallvec = "1"` to the workspace and `smallvec.workspace = true` to `ridi-router-routing`.
2. Replace the hottest temporary adjacency containers with `SmallVec` where road degree is usually small.
3. Start with local/internal buffers only, not a full adjacency API redesign.
4. Keep behavior and route-search APIs unchanged.

### Explicitly out of scope
- collapsing tile-manager and graph into one shared adjacency return type
- caller-filled buffer APIs
- route behavior changes

## Expected code touch points
- `Cargo.toml`
- `crates/ridi-router-routing/Cargo.toml`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

## Implementation steps
1. Add `smallvec` dependencies.
2. Change the raw adjacency temp buffer in `TileManager::get_adjacent_by_id(...)` from `Vec` to `SmallVec`.
3. Change the mapped adjacency temp buffer in `MapDataGraph::get_adjacent(...)` from `Vec` to `SmallVec` where that stays local and simple.
4. Keep the public return type unchanged if that avoids ripple. Convert only at the boundary.
5. Use a conservative inline size such as 8, matching the expected small road degree.
6. Add focused tests only where needed to lock in identical outputs.

## Validation
1. `cargo test -p ridi-router-routing map_data::graph`
2. `cargo test -p ridi-router-routing rmdf::tile_manager`
3. `cargo check --workspace`
4. Profile run:
   ```bash
   RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia
   ```
5. Compare at minimum:
   - alloc totals for `graph::get_adjacent`
   - alloc totals for `tile_manager::get_adjacent_by_id`
   - total route-generation time

## Exit criteria
- adjacency alloc totals drop materially
- the change stays local and easy to back out
- no new wide API churn appears just to support `SmallVec`

## Main risk
Low to medium. The main risk is spreading `SmallVec` through too many layers at once and turning a local cleanup into a broad refactor.
