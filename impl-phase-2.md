# Phase 2: Filtering semantics in `TileManager`

## Goal
Implement the filtering behavior first, with tests that prove the stable-v1 semantics before the spatial-index search path is swapped in.

## Why this phase exists
The hardest correctness risk is filtering semantics, not cell lookup. The repo already has the tag lookup primitives needed by `TileManager`:
- `get_tag_set_record(...)`
- `get_tag_value(...)`

So this phase should lock in behavior before the search algorithm changes.

## Files
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- unit tests in the same file

## Planned changes
1. Add small internal helpers in `tile_manager.rs`:
   - `AvoidTagSet`
   - `collect_avoid_tags(rules: &RouterRules) -> AvoidTagSet`
   - `should_check_limit_tags(limit_to_hw_tags, avoid_tags) -> bool`
   - `adjacent_line_tags(...) -> Result<...>`
   - `point_matches_filters(...) -> Result<bool>`
   - `haversine_distance_m(...) -> f32`
2. Implement stable-v1 filter semantics for each candidate point:
   - skip disconnected points
   - honor `avoid_proximity_to_residential`
   - reject the point if **any adjacent line** matches an avoided `highway`, `surface`, or `smoothness`
   - when allowlist checking is active, keep the point only if at least one adjacent line has an allowed `highway`
   - preserve the `check_limit_tags` nuance: disable allowlist checking if every allowed highway is already avoided by rules
3. Switch final candidate ranking from planar sqrt-on-lat/lon to Haversine distance.
4. Keep search temporarily simple for this phase:
   - continue scanning the full tile
   - apply the new filters to every candidate

## Tests to add in this phase
Add focused unit tests in `crates/ridi-router-routing/src/rmdf/tile_manager.rs` for:
1. highway allowlist honors mixed adjacent tags
2. highway allowlist rejects nearest disallowed point in favor of farther allowed point
3. rules filtering rejects the whole point when any adjacent line matches an avoided tag
   - cover `highway`
   - cover `surface`
   - cover `smoothness`
4. residential avoidance still applies after highway filtering

These tests should be laid out so they do not depend on the ring-scan implementation yet.

## Validation
- New unit tests fail before implementation and pass after it.
- `MapDataGraph` / `RoutingContext` remain forwarding-only.
- No public API changes.

## Exit criteria
- `_rules` and `limit_to_hw_tags` are no longer ignored.
- Filtering semantics match the todo document’s stable-v1 requirements.
- Final ranking uses Haversine distance.
