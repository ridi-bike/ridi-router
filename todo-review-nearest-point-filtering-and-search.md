# Review: nearest-point filtering in `TileManager::get_closest_to_coords(...)`

## Verdict
Relevant.

## Summary
The tile-backed closest-point lookup still ignores the filtering inputs carried by the API.

Today it only filters out:
- disconnected points (`lines_count == 0`)
- residential-proximate points when requested

It still does **not** honor:
- `limit_to_hw_tags`
- `RouterRules`
- spatial-index based search beyond a single-tile linear scan

The tiles refactor changed one important detail: `ridi-router-tiles` no longer keeps generation-side point rules on `GenerationPoint`. So any future rule-aware nearest lookup depends on the unfinished RMDF rules pipeline, not on reviving old tiles-side point fields.

## Current evidence
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
  - `get_closest_to_coords(...)` still accepts `_rules` and `_limit_to_hw_tags`
  - both parameters are unused
  - code still contains TODOs for ring search, rules filtering, and highway tag filtering
- `crates/ridi-router-routing/src/map_data/graph.rs`
  - `get_point_from_tiles(...)` still hydrates `rules: Vec::new()`
- `crates/ridi-router-tiles/src/generation/graph.rs`
  - `insert_relation(...)` is still a stub
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
  - `serialize_rules(...)` still returns an empty vector
  - point records still write `rules_offset: 0` and `rules_count: 0`

## What changed after the refactor
The old tiles-private `map_data` model is gone.

That means this todo should now be split more clearly:
1. **nearest-point semantic filtering that can be implemented today**
2. **rule-aware filtering that stays blocked on RMDF rules support**
3. **search-strategy improvements that are independent from both**

## Recommended direction
### Phase 1: unblocked correctness
Implement `limit_to_hw_tags` filtering now.

Use adjacent line records and tag sets already available through runtime tile access. A point should be eligible only if it has at least one connected line whose `highway` tag is in the allowed set.

### Phase 2: separate search work
Improve nearest search independently:
- use the spatial index instead of a full scan
- define border behavior explicitly
- decide whether cross-tile lookup is required for correctness or only optimization

### Phase 3: rule-aware lookup
Only add rule-based filtering after the restrictions pipeline exists end to end.

That work is tracked by:
- `todo-plan-turn-restrictions-pipeline.md`
- `todo-review-rmdf-rules-writer.md`
- `todo-review-runtime-load-rules-from-tiles.md`

## Key scope note
Do **not** block `limit_to_hw_tags` on turn-restriction work.

The concrete mismatch already exercised by callers is highway-tag filtering, and that can be implemented from existing line/tag data.

## Validation
Add focused tests in `crates/ridi-router-routing/src/rmdf/tile_manager.rs` for:
- mixed adjacent highway tags
- residential avoidance plus highway filtering
- border-adjacent cases once search scope is defined
