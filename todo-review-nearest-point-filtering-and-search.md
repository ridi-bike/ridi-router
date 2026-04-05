# Review: nearest-point filtering in `TileManager::get_closest_to_coords(...)`

## Verdict
Relevant.

## Summary
The tile-backed closest-point lookup still ignores two API inputs and still uses the simplest search strategy:

- `_limit_to_hw_tags` is unused
- `_rules` is unused
- lookup still scans only the containing tile linearly

The most important live mismatch is highway filtering. Callers already pass `Some(&WP_LOOKUP_ALLOWED_HWS)` from `crates/ridi-router-routing/src/routing_api.rs` and `crates/ridi-router-routing/src/router/generator.rs`, but `TileManager::get_closest_to_coords(...)` still ignores it. That means start/finish and generated waypoint lookup can snap to points whose adjacent lines are outside the intended highway set.

Turn-restriction hydration is no longer the blocker it used to be. RMDF rule serialization and runtime rule loading now exist. The remaining `_rules` question is about nearest-point semantics, not missing restriction plumbing.

## Current evidence
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
  - `get_closest_to_coords(...)` still takes `_rules` and `_limit_to_hw_tags`
  - both are unused
  - TODOs still mention ring search, rules filtering, and highway tag filtering
- `crates/ridi-router-routing/src/map_data/graph.rs`
  - forwards `_limit_to_hw_tags` unchanged
- `crates/ridi-router-routing/src/routing_api.rs`
  - start and finish lookup pass `Some(&WP_LOOKUP_ALLOWED_HWS)`
- `crates/ridi-router-routing/src/router/generator.rs`
  - waypoint generation also passes `Some(&WP_LOOKUP_ALLOWED_HWS)`
- Existing runtime tile access already exposes what phase 1 needs:
  - point -> adjacent line refs via `get_adjacent_by_id()`
  - line -> tag set via `get_line_by_index()` / `get_tag_set_record()`
  - tag values via `get_tag_value(...)`

## Scope split
### Phase 1: fix the live correctness gap
Implement `limit_to_hw_tags` filtering now.

A point should be eligible only if at least one connected line has a `highway` tag contained in the allowed set. This is already enough to make start/finish and generated waypoint snapping match caller intent.

### Phase 2: decide `_rules` semantics explicitly
The `_rules` parameter should not stay as a vague placeholder. Decide one of:
- nearest lookup should honor RouterRules-based tag avoidance/preferences
- nearest lookup should not interpret RouterRules at all, and the parameter should be removed from this layer

Do not conflate this with turn-restriction hydration; that pipeline is already present.

### Phase 3: improve search strategy
Independently of filtering:
- use the spatial index instead of a full scan
- define border behavior clearly
- decide whether correctness requires cross-tile expansion or whether same-tile lookup is acceptable for now

## Validation
- add focused tests in `crates/ridi-router-routing/src/rmdf/tile_manager.rs` for:
  - mixed adjacent highway tags
  - residential avoidance plus highway filtering
  - a routing-API level case proving `WP_LOOKUP_ALLOWED_HWS` is honored for start/finish lookup
- optional later tests for border-adjacent search once search scope is defined
