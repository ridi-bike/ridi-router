# Review: load point rules from tiles at runtime

## Verdict
Relevant, but blocked on the broader restrictions pipeline.

## Summary
Runtime tile loading still does not hydrate `MapDataPoint.rules`.

That is still true in `crates/ridi-router-routing/src/map_data/graph.rs`, where `get_point_from_tiles(...)` returns `rules: Vec::new()`.

But after the `tiles` refactor, this is no longer a small runtime-only gap. `ridi-router-tiles` no longer keeps generation-side `point.rules` at all. `GenerationPoint` is intentionally just point geometry + flags.

So this file now tracks the **runtime loading slice** only. It depends on the broader follow-up in `todo-review-turn-restrictions-rule-model.md`.

## Current evidence
- `crates/ridi-router-routing/src/map_data/graph.rs`
  - `get_point_from_tiles(...)` still returns `rules: Vec::new()`
- `crates/ridi-router-routing/src/rmdf/io.rs`
  - no rule accessor exists yet
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
  - no rule-loading helper exists yet
- `crates/ridi-router-tiles/src/generation/graph.rs`
  - `insert_relation(...)` is still a stub
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
  - `serialize_rules(...)` still returns an empty vector
- `crates/ridi-router-common/src/format.rs`
  - RMDF still has `PointRecord.rules_offset` / `rules_count` and `RuleRecord`

## What changed after the refactor
Do **not** try to fix this by putting `rules` back onto `GenerationPoint`.

That would undo the generation-model cleanup.

The runtime still wants `MapDataPoint.rules`, but generation-side restriction state should now live in a separate generation-only structure and only be turned into runtime `MapDataRule` values when reading tiles back.

## Scope of this file
This todo starts **after** tiles can actually serialize restriction data.

Runtime-side work:
1. add RMDF rule access in `crates/ridi-router-routing/src/rmdf/io.rs`
2. add tile-manager helpers in `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
3. hydrate `MapDataPoint.rules` in `crates/ridi-router-routing/src/map_data/graph.rs`
4. add tile-backed walker tests that prove restrictions affect routing

## Blocker
Blocked on:
- `todo-review-turn-restrictions-rule-model.md`
- `todo-review-rules-serialization.md`

## Validation
A generated tile with a simple restriction should change walker behavior without relying on `crates/ridi-router-routing/src/test_utils.rs`.
