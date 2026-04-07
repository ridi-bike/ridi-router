# Implementation Phase 1: low-risk point hydration wins

## Goal
Cut the hottest obvious waste in `graph::get_point_from_tiles` without changing routing behavior or cache policy yet.

## Why this is phase 1
This phase is the cleanest, lowest-risk slice:
- it removes known over-fetch work
- it avoids a major hot call (`get_adjacent_by_id`) on the point hydration path
- it adds the zero-rules fast path called out in `perf-plan.md`
- it should produce measurable wins before deeper refactors

## Scope

### In scope
1. Rewrite `graph::get_point_from_tiles` to build `lines` directly from:
   - `PointRecord.lines_offset`
   - `PointRecord.lines_count`
   - `MappedTile::get_line_refs()`
2. Stop calling `tile_manager::get_adjacent_by_id(...)` from point hydration.
3. Add a zero-rules fast path when `point_record.rules_count == 0`.
4. Add a zero-lines fast path when `point_record.lines_count == 0`.
5. Pre-size output vectors where counts are already known.
6. Keep output shape of `MapDataPoint` unchanged.

### Explicitly out of scope
- zero-copy rule section access
- lock strategy changes
- task-local caches
- caller API cleanup
- point indexing

## Expected code touch points
- `crates/ridi-router-routing/src/map_data/graph.rs`
- existing graph tests around point hydration
- possibly small helper additions in `tile_manager.rs` if direct loaded-tile access needs a cleaner helper

## Deliverables
- `get_point_from_tiles` no longer depends on `get_adjacent_by_id` for `MapDataPoint.lines`
- points with no rules return `rules: Vec::new()` without rule hydration
- tests proving:
  - points still hydrate the same line refs
  - points with rules still hydrate rules correctly
  - points without rules keep empty rule lists
  - cross-tile adjacency behavior for `graph.get_adjacent(...)` remains unchanged

## Validation
1. Run targeted tests for graph and tile manager.
2. Run `cargo check --workspace`.
3. Re-run the current perf workflow and compare:
   - `graph::get_point_from_tiles`
   - `tile_manager::get_adjacent_by_id`
   - `tile_manager::get_rules_for_point`
4. Confirm that `get_adjacent_by_id` call count drops materially during route generation.

## Exit criteria
- behavior unchanged
- `get_point_from_tiles` is simpler and allocates less on the common path
- point hydration no longer pulls in border-crossing adjacency logic just to get current point line refs

## Main risk
Very low. The main thing to watch is preserving the exact `MapDataLineRef` values currently exposed by `MapDataPoint.lines`.

## Rollback plan
If anything subtle appears, keep the fast paths and revert only the direct-line-ref rewrite.
