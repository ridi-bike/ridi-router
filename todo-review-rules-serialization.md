# Review: RMDF rules serialization after the tiles generation refactor

## Verdict
Relevant, but now as a writer-side subtask of the broader restrictions pipeline.

## Summary
`serialize_rules(...)` in `crates/ridi-router-tiles/src/rmdf/generator/writer.rs` still returns an empty vector, so generated RMDF tiles still contain no usable restriction payload.

But after the tiles refactor, this should **not** be solved by re-adding `rules` to `GenerationPoint`.

The generation model is now intentionally split as:
- `GenerationPoint`
- `GenerationLine`
- `GenerationTags`

Restriction materialization needs its own generation-side structure, and this file should stay focused on the writer side of that pipeline.

## Current evidence
- `crates/ridi-router-tiles/src/generation/graph.rs`
  - `insert_relation(...)` is still a stub
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
  - `serialize_rules(...)` still returns `Vec::new()`
  - point records still write `rules_offset: 0` and `rules_count: 0`
- `crates/ridi-router-common/src/format.rs`
  - RMDF already has `RuleRecord`
  - RMDF already has `PointRecord.rules_offset` / `rules_count`

## What this file should cover
Writer-side work only:
1. decide what generation-side restriction data the writer consumes
2. serialize that into RMDF `RULES`
3. compute real per-point `rules_offset` / `rules_count`
4. add writer round-trip coverage

## What this file should not decide alone
This file is **not** the source of truth for overall restriction modeling.

The broader decisions live in:
- `todo-review-turn-restrictions-rule-model.md`

Runtime loading lives in:
- `todo-review-load-point-rules-from-tiles.md`

## Important post-refactor constraint
Do **not** reintroduce generation-side `point.rules` just to make serialization easy.

That would fight the new `generation::*` architecture.

## Recommended direction
Use a separate generation-side restriction collection owned by `GenerationGraph` or a sibling helper, then make the writer serialize from that structure.

The runtime can still hydrate back into `MapDataPoint.rules` later.

## Validation
- generated tiles contain non-zero `rule_count` when restrictions are present
- point records contain real `rules_offset` / `rules_count`
- runtime read-side tests can consume the serialized rules once the loader exists
