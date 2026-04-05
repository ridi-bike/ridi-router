# Plan: turn restrictions pipeline after the tiles generation refactor

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
Use **design C**: store a separate, generation-owned, tile-local restriction collection on `GenerationGraph`, not on `GenerationPoint`.

> generation stays generation-shaped, but relations are materialized early enough that the writer does not need to re-derive routing semantics from raw OSM relations

Recommended shape:
- add a new generation restriction module, e.g. `crates/ridi-router-tiles/src/generation/restriction.rs`
- extend `GenerationGraph` with:
  - `way_line_indices: HashMap<u64, Vec<usize>>`
  - `restrictions_by_via: HashMap<u64, Vec<GenerationRestrictionRule>>`
- keep `GenerationPoint` unchanged

Recommended first-pass types:
```rust
pub enum GenerationRestrictionRuleType {
    OnlyAllowed,
    NotAllowed,
}

pub struct GenerationRestrictionRule {
    pub relation_id: u64,
    pub via_node_id: u64,
    pub rule_type: GenerationRestrictionRuleType,
    pub from_line_indices: Vec<u32>,
    pub to_line_indices: Vec<u32>,
}
```

Implementation shape:
1. During `insert_way(...)`, record every generated line index under the source OSM way ID in `way_line_indices`.
2. During `insert_relation(...)`, parse supported restriction relations and resolve them immediately into **tile-local line indices**.
3. For each `from` or `to` way, only keep the generated line indices that actually touch the `via` node.
4. Store the resulting rule under `restrictions_by_via[via_node_id]`.

That keeps generation-side restriction data separate from points, while still being writer-ready.

### First implementation scope
Support only simple **via-node** turn restrictions with:
- one or more `from` way members
- exactly one `via` node member
- one or more `to` way members
- `type=restriction`
- `restriction` values mapped to existing runtime semantics:
  - `no_left_turn`
  - `no_right_turn`
  - `no_straight_on`
  - `no_u_turn`
  - `no_entry`
  - `no_exit`
  - `only_left_turn`
  - `only_right_turn`
  - `only_straight_on`
  - `only_u_turn`

Out of scope for the first pass:
- `via` way restrictions
- conditional restrictions
- mode-specific restriction variants like `restriction:motorcycle=*`
- `except=*` handling
- any relation shape that cannot be resolved into one via-node-centered rule set

Unsupported relations should be **skipped explicitly**, not half-applied.

### RMDF side
Keep the existing `RuleRecord`. Do **not** add a new RMDF section for rule line refs in the first implementation.

Serialize rules into the existing `RULES` section as:
1. `RuleRecord[]`
2. trailing flattened `u64[]` rule-line-ref payload

Encoding decision:
- `PointRecord.rules_offset` / `rules_count` index into the `RuleRecord[]` prefix
- `RuleRecord.from_lines_offset` / `from_lines_count` index into the trailing flattened `u64[]` payload
- `RuleRecord.to_lines_offset` / `to_lines_count` index into the trailing flattened `u64[]` payload
- those line refs are tile-local generated line indices

So `RuleRecord` remains enough as-is; the extra data lives as a flattened payload in the same `RULES` section.

### Cross-tile implications
Do **not** introduce cross-tile rule references for the first pass.

The tile generation pipeline already builds tiles from buffered bounds, so border ways/nodes are duplicated into neighboring tiles. The rule model should use that and stay tile-local:
- materialize a rule in every tile where the `via` node and the relevant local line copies exist
- skip a rule in a tile if either side resolves to zero local via-adjacent lines
- write only tile-local line indices into RMDF

### Runtime side
Runtime still exposes:
- `MapDataPoint.rules: Vec<MapDataRule>`

Hydration shape:
1. load `RuleRecord`s for the point using `PointRecord.rules_offset` / `rules_count`
2. load the referenced flattened line-index payload slices
3. convert each stored line index into `MapDataLineRef::new(point_tile_id, line_index as u64)`
4. map serialized rule type to `MapDataRuleType::{OnlyAllowed, NotAllowed}`

So runtime stays runtime-shaped; RMDF remains the bridge between the generation model and `MapDataPoint.rules`.

### Important semantic choice
For generated tiles, rules should store only the **via-adjacent generated line indices** for the `from` and `to` sides, not every line belonging to the source OSM ways.

That is smaller, more precise, and matches what the walker actually checks at the junction.

## Resolved design questions
1. Generation-side restriction structure
   - use via-point keyed, writer-ready restriction records on `GenerationGraph`
2. `RuleRecord` sufficiency
   - keep `RuleRecord`; append flattened line refs inside the same `RULES` section
3. Cross-tile references
   - do not encode cross-tile rule refs in the first pass; duplicate tile-local rules instead
4. First implementation scope
   - support simple via-node restrictions only, with one or more `from` ways and one or more `to` ways

## Suggested work order
1. Lift the supported restriction semantics from `crates/ridi-router-routing/src/test_utils.rs` into generation-side parsing/materialization code.
2. Add `way_line_indices` and `restrictions_by_via` to `crates/ridi-router-tiles/src/generation/graph.rs`.
3. Implement `insert_relation(...)` for supported via-node restriction relations only.
4. Finish the writer-side offset work needed for flattened side tables:
   - real `lines_offset` / `lines_count` for point line refs
   - real `rules_offset` / `rules_count` for point rule refs
5. Finish rule serialization in `crates/ridi-router-tiles/src/rmdf/generator/writer.rs` using `RuleRecord[] + flattened rule line refs`.
6. Add RMDF rule accessors in runtime tile IO.
7. Hydrate `MapDataPoint.rules` in `crates/ridi-router-routing/src/map_data/graph.rs`.
8. Add at least one generated-tile end-to-end routing test proving walker behavior changes because of serialized restrictions.

## Related narrower todos
- `todo-review-rmdf-rules-writer.md` — writer slice
- `todo-review-runtime-load-rules-from-tiles.md` — runtime loading slice
- `todo-review-fix-point-line-ref-offsets.md` — keep this focused on `lines_offset`, not rules

## Done criteria
This todo is done when all of the following are true:
- restriction relations are retained by the generation pipeline
- RMDF tiles serialize real rule data
- runtime tile loading hydrates non-empty `MapDataPoint.rules` when rules exist
- walker behavior changes for generated tiles with restrictions
- no tiles-side code reintroduces the old private `map_data` model just to support rules
