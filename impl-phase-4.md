# Implementation Phase 4: conditional collapse of the double materialization layer and final perf sweep

## Goal
Only if phases 1-3 leave adjacency as a first-order hotspot, remove the remaining double-materialization overhead between tile manager and graph.

## Why this is phase 4
This is the most invasive part of option 3. It should happen only after we have numbers from the earlier phases.

By then we will know whether the remaining cost is mostly:
- repeated calls
- lock contention
- or the fact that adjacency still gets built twice per lookup

## Scope

### In scope
1. Decide from fresh profiling whether adjacency still needs a deeper refactor.
2. If yes, collapse one materialization layer by choosing one of these bounded designs:
   - tile manager returns final adjacency refs directly, or
   - graph and tile manager share one adjacency container type alias
3. Keep the redesign narrow to adjacency only.
4. Finish with a full perf re-run and compare against the original baseline from `perf.md`.

### Explicitly out of scope
- eager per-tile adjacency precompute
- broad graph API redesign unrelated to adjacency
- speculative shared memoization beyond what the profile proves necessary

## Expected code touch points
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- possibly `crates/ridi-router-routing/src/routing_context.rs` if the final adjacency container type changes

## Implementation steps
1. Re-profile after phase 3.
2. If adjacency is no longer first-order, skip the code refactor and keep this phase as measurement plus closeout.
3. If adjacency still matters, pick one narrow design:
   - `TileManager` returns final `(MapDataLineRef, MapDataPointRef)` pairs directly, or
   - introduce a shared adjacency type alias and remove one conversion layer
4. Update tests covering:
   - same-tile adjacency
   - cross-tile adjacency
   - missing-neighbor behavior
5. Run a final before/after summary against the original baseline.

## Validation
1. `cargo test -p ridi-router-routing`
2. `cargo check --workspace`
3. Final profile run:
   ```bash
   RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia
   ```
4. Compare the original baseline to the final state for:
   - `graph::get_adjacent`
   - `tile_manager::get_adjacent_by_id`
   - walker hotspot totals
   - route-generation wall time

## Exit criteria
- either adjacency is no longer worth deeper work and phase 4 is skipped cleanly,
- or one materialization layer is removed with behavior unchanged and a measurable perf gain.

## Main risk
Medium. This is where the code can get more coupled if the final adjacency type crosses too many boundaries. Keep the design narrow and stop once the profile says the hotspot is no longer first-order.
