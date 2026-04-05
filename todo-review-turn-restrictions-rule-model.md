# Review: turn restrictions after the tiles generation refactor

This file is the source of truth for the remaining restriction/rules work.

## Verdict
Still relevant.

## Summary
The big tiles refactor is done.

`ridi-router-tiles` is now explicitly generation-shaped:
- `crates/ridi-router-tiles/src/generation/point.rs`
- `crates/ridi-router-tiles/src/generation/generation_line.rs`
- `crates/ridi-router-tiles/src/generation/tags.rs`
- `crates/ridi-router-tiles/src/generation/graph.rs`

That refactor intentionally removed the old tiles-private runtime-shaped model, including generation-side point `rules`.

So the remaining restriction task is **not** to bring the old tiles `map_data` design back.

The real follow-up is:
1. collect and retain restriction information during generation
2. serialize it into RMDF
3. hydrate it back into runtime `MapDataPoint.rules`

## Reviewed decisions
- Keep the new `generation::*` architecture.
- Do **not** reintroduce tiles-private `map_data`.
- Do **not** put `rules` back on `GenerationPoint`.
- Keep the existing runtime rule model for now:
  - `crates/ridi-router-routing/src/map_data/rule.rs`
  - `MapDataRuleType::{OnlyAllowed, NotAllowed}`
  - `MapDataPoint.rules: Vec<MapDataRule>`
- Add a **separate generation-side restriction collection** instead of storing runtime-shaped rules on generation points.
- Use that generation-side restriction data as the source for RMDF rule serialization.
- On the read side, continue hydrating runtime `MapDataRule` values from tiles.

## Current code reality
### 1. Generation-side point rules are gone by design
- `crates/ridi-router-tiles/src/generation/point.rs`
  - `GenerationPoint` only has:
    - `id`
    - `lat`
    - `lon`
    - `residential_in_proximity`
    - `nogo_area`

That is the correct post-refactor shape.

### 2. Restriction ingestion is still stubbed
- `crates/ridi-router-tiles/src/generation/graph.rs`
  - `insert_relation(...)` still does nothing
- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
  - still passes collected restriction relations into `graph.insert_relation(...)`
- `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
  - same

### 3. RMDF rule serialization is still empty
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
  - `serialize_rules(...)` returns `Vec::new()`
  - point records still write `rules_offset: 0` and `rules_count: 0`

### 4. Runtime still expects point-attached rules
- `crates/ridi-router-routing/src/router/walker.rs`
  - routing logic already consumes `MapDataPoint.rules`
- `crates/ridi-router-routing/src/map_data/graph.rs`
  - `get_point_from_tiles(...)` still hydrates `rules: Vec::new()`

### 5. The semantic mapping already exists in tests
- `crates/ridi-router-routing/src/test_utils.rs`
  - already maps OSM restriction relations to runtime rule behavior for test graphs

That test-only mapping is the best reference for first-pass semantics.

## Chosen direction
### Generation side
Add a restriction collection separate from `GenerationPoint`.

The exact type can vary, but it should live with the generation model, for example as data owned by `GenerationGraph`.

It should represent enough information to later serialize RMDF rules without reintroducing old fake runtime surfaces.

### RMDF side
Serialize restrictions from that generation-side collection into:
- `PointRecord.rules_offset` / `rules_count`
- `RuleRecord`
- any additional flattened rule-line side data that turns out to be necessary

### Runtime side
Continue exposing runtime restrictions as:
- `MapDataPoint.rules: Vec<MapDataRule>`

In other words:
- generation stays generation-shaped
- runtime stays runtime-shaped
- RMDF is the bridge

## Open design questions
1. What is the cleanest generation-side restriction structure?
   - via-point keyed records?
   - temporary way/member references resolved at write time?
   - writer-ready line references once line indices are known?
2. Is `RuleRecord` enough as-is, or is an extra flattened line-ref side table needed?
3. How should cross-tile line references be encoded when a rule touches border lines?
4. Should the first implementation cover only simple `via`-node restrictions, matching current test helpers?

## Suggested work order
1. Lift the existing restriction semantics from `crates/ridi-router-routing/src/test_utils.rs` into generation-side code.
2. Add generation-side restriction storage to `crates/ridi-router-tiles/src/generation/graph.rs`.
3. Finish writer-side rule serialization in `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`.
4. Add RMDF rule readers in runtime tile IO.
5. Hydrate `MapDataPoint.rules` in `crates/ridi-router-routing/src/map_data/graph.rs`.
6. Add one generated-tile end-to-end routing test.

## Related narrower todos
- `todo-review-rules-serialization.md` — writer slice
- `todo-review-load-point-rules-from-tiles.md` — runtime loading slice
- `todo-review-point-record-real-offsets.md` — keep this focused on `lines_offset`, not rules

## Done criteria
This todo is done when all of the following are true:
- restriction relations are retained by the generation pipeline
- RMDF tiles serialize real rule data
- runtime tile loading hydrates non-empty `MapDataPoint.rules` when rules exist
- walker behavior changes for generated tiles with restrictions
- no tiles-side code reintroduces the old private `map_data` model just to support rules
