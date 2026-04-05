# Review: turn restriction and rule model for RMDF generation

## Verdict
Partially relevant.

## Summary
The todo is **still relevant in its end-to-end goal**, but part of its wording is now stale.

What is still true:
- restriction relations are collected during tile generation
- `GenerationGraph::insert_relation(...)` is still a stub in the tile pipeline
- RMDF rule serialization is still unfinished
- tile-backed runtime loading still drops rules

What is stale or already implicitly decided:
- the codebase already has an in-memory rule shape: `MapDataPoint.rules: Vec<MapDataRule>` with `MapDataRuleType::{OnlyAllowed, NotAllowed}`
- router logic already consumes that model
- test utilities already contain a concrete relation-to-rule mapping for OSM restriction relations

So the open work is **not** “invent a rule model from scratch”. The real gap is that the existing model has not been wired through the generation -> RMDF -> tile runtime path.

## Current evidence from the codebase

### 1. The generation graph still skips relation processing
In the tile-generation crate, `insert_relation(...)` is still unimplemented:

- `crates/ridi-router-tiles/src/map_data/generation_graph.rs:153-159`
  - `pub fn insert_relation(&mut self, _relation: OsmRelation) -> Result<(), MapDataError>`
  - comment says turn restrictions are skipped for now

The same stub also exists in the routing crate copy:
- `crates/ridi-router-routing/src/map_data/generation_graph.rs:153-159`

That directly matches the todo's core complaint.

### 2. Restriction relations are still collected upstream
The tile pipeline does not ignore restriction relations entirely; it gathers them and passes them into the graph:

- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs:560-591`
  - restriction relations are added to the intermediate tile
- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs:929-933`
  - those relations are later passed to `graph.insert_relation(...)`
- `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs:342-373`
  - streaming path also collects only restriction relations
- `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs:424-428`
  - and passes them into `insert_relation(...)`

So the pipeline already reaches the handoff point, but stops there.

### 3. The in-memory rule model already exists
The todo says to “decide the in-memory shape”, but there is already a concrete shape in both crates:

- `crates/ridi-router-tiles/src/map_data/rule.rs:7-18`
- `crates/ridi-router-routing/src/map_data/rule.rs:7-18`
  - `MapDataRuleType::{OnlyAllowed, NotAllowed}`
  - `MapDataRule { from_lines: Vec<MapDataLineRef>, to_lines: Vec<MapDataLineRef>, rule_type }`

And points already carry rules:
- `crates/ridi-router-tiles/src/map_data/point.rs:16-23`
- `crates/ridi-router-routing/src/map_data/point.rs:15-22`
  - `pub rules: Vec<MapDataRule>`

That means the model is not missing at the type level.

### 4. Runtime routing logic already consumes point rules
The router is already written to honor point-attached rules:

- `crates/ridi-router-routing/src/router/walker.rs:55-90`
  - `get_segments_for_point_with_context(...)` filters using `MapDataRuleType::NotAllowed`
- `crates/ridi-router-routing/src/router/walker.rs:110-161`
  - `get_fork_segments_for_segment_with_context(...)` applies both `OnlyAllowed` and `NotAllowed`

So “runtime code can consume the generated rule data” is partly already true: the runtime code can consume this model, but only when the rules exist in memory.

### 5. There is already a concrete OSM-relation -> MapDataRule mapping in tests
The test helper in the routing crate already performs a real translation from OSM restriction relations into `MapDataRule`s:

- `crates/ridi-router-routing/src/test_utils.rs:448-528`
  - filters `type=restriction`
  - maps `no_*` restrictions to `MapDataRuleType::NotAllowed`
  - maps `only_*` restrictions to `MapDataRuleType::OnlyAllowed`
  - extracts `from` way, `via` node, and `to` ways
  - attaches the resulting rule to the `via` point

This is strong evidence that the intended semantic model is already known in the codebase, at least for node-via restrictions.

### 6. Existing routing tests validate rule behavior, but through the test-only path
There are walker tests for restriction behavior:

- `crates/ridi-router-routing/src/router/walker.rs:802-959` and following tests
  - examples include `rule_no_left`, `rule_no_straight`, `rule_no_right`, `rule_no_u`, etc.

These tests prove the rule model itself is used by routing logic. They do **not** prove the tile generation / RMDF path works.

### 7. RMDF format reserves rule support, but the writer still does not serialize it
The file format already includes rule-related structures:

- `crates/ridi-router-common/src/format.rs:77-88`
  - `PointRecord` has `rules_offset` and `rules_count`
- `crates/ridi-router-common/src/format.rs:140-151`
  - `RuleRecord` exists
- `crates/ridi-router-common/src/format.rs:153-160`
  - `section::RULES` exists
- `crates/ridi-router-common/src/format.rs:38-49`
  - `RmdfHeader` has `rule_count`

But the writer is still unfinished:

- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs:228-236`
  - point records are written with `rules_offset: 0`
  - `rules_count` comes from `point.rules.len()`
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs:376-379`
  - `serialize_rules(...)` returns `Ok(Vec::new())`

So the format supports rules in principle, but generated tiles do not actually contain them.

### 8. Tile-backed runtime loading still discards rules
Even on the read side, rule hydration is still missing:

- `crates/ridi-router-routing/src/rmdf/io.rs:73-159`
  - `MappedTile` exposes spatial index, points, lines, line refs, tag sets, and tag values
  - there is no rule accessor
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs:67-220`
  - there is no rule lookup helper
- `crates/ridi-router-routing/src/map_data/graph.rs:358-367`
  - `get_point_from_tiles(...)` builds `MapDataPoint { ..., rules: Vec::new(), ... }`

So the runtime tile path still cannot load restrictions, even if the writer were finished.

## The actual problem
The todo is still relevant because the tile pipeline has an unfinished restriction path.

More precisely, the missing work is:
1. convert restriction relations into `MapDataRule`s during generation
2. attach those rules to generated points (likely the `via` point)
3. serialize them into the RMDF rules section
4. load them back from tiles at runtime

The todo text is partially wrong because it frames this as needing a brand new model. In reality, the model already exists and is already used in tests/runtime logic. The unfinished part is the production pipeline.

## What behavior is missing or risky

### Missing behavior
- generated RMDF tiles do not preserve restriction relations as runtime-usable rules
- tile-backed routing does not enforce turn restrictions
- runtime behavior differs between:
  - in-memory / test-constructed graphs
  - tile-backed graphs loaded from generated RMDF

### Risks
- the todo may encourage redesigning the rule model unnecessarily
- implementing only `insert_relation(...)` would still be incomplete without writer and reader work
- existing routing tests can give false confidence, because they exercise a test-only relation mapping instead of the RMDF path
- the current `RuleRecord` format may be awkward for variable-length `from_lines` / `to_lines` lists, especially if rules can refer to lines across tiles

## Possible solution options

### Option A: adopt the existing point-attached `MapDataRule` model end to end
Use the already-existing semantic model consistently:
- materialize `MapDataRule`s in `GenerationGraph::insert_relation(...)`
- attach them to `MapDataPoint.rules`
- serialize them into RMDF
- load them back into `MapDataPoint.rules` for tile-backed routing

#### Tradeoffs / implications
- smallest conceptual change, because it matches current router logic
- aligns with existing test helpers and walker behavior
- still requires solving RMDF encoding for variable-length line lists and offsets
- cross-tile references need careful handling because `MapDataRule` stores `MapDataLineRef`s

### Option B: keep RMDF restrictions in a separate runtime structure
Instead of hydrating `MapDataPoint.rules`, restrictions could be stored in a separate index keyed by `(via point, from line, to line)` transitions.

#### Tradeoffs / implications
- may be simpler for tile IO and lazy loading
- would diverge from the current `MapDataPoint.rules` contract used by the walker
- would require router refactoring, not just pipeline completion
- there is no evidence in the current code that such a replacement is already underway

## Recommended direction
Treat the todo as a **pipeline-completion task**, not a type-design task.

Recommended direction:
1. keep the existing `MapDataRule` / `MapDataPoint.rules` model
2. lift the relation-to-rule mapping approach from `crates/ridi-router-routing/src/test_utils.rs` into generation code
3. finish RMDF rule serialization around the existing `RuleRecord` section
4. add tile-side rule loading so `get_point_from_tiles(...)` no longer hardcodes `rules: Vec::new()`
5. add one end-to-end tile fixture that proves a generated restriction affects routing

This is the most practical path because it reuses the model the router already understands.

## Open questions
- Is the intended first implementation limited to simple `via`-node restrictions, matching the existing test helper, or must it also cover `via`-way restrictions immediately?
- Is `crates/ridi-router-common/src/format.rs::RuleRecord` sufficient as-is for the current `MapDataRule` shape, or does it also need a side table for flattened line refs?
- How should cross-tile line references be encoded when a rule attached to one point refers to lines whose canonical indices live in another tile?
- Should the rule materialization logic live directly in `GenerationGraph::insert_relation(...)`, or in a dedicated helper used by both `ridi-router-tiles` and `ridi-router-routing` test scaffolding?
- Is the duplicate `generation_graph.rs` in both crates intentional long-term, or should the restriction logic eventually be shared?

## Suggested implementation starting point
Start from the existing test-only mapping, because it already captures the intended semantics.

Practical first steps:
1. inspect `crates/ridi-router-routing/src/test_utils.rs:448-528`
   - treat this as the current reference behavior for simple restriction relations
2. implement equivalent relation materialization in `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
   - likely by building way -> line mappings and attaching rules to the `via` point
3. once generated points actually contain `rules`, finish `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
   - serialize the rules section
   - compute correct per-point `rules_offset` / `rules_count`
4. then add read-side support in:
   - `crates/ridi-router-routing/src/rmdf/io.rs`
   - `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
   - `crates/ridi-router-routing/src/map_data/graph.rs`
5. add one tile-backed routing test that uses generated data rather than the test-only in-memory path

## Related files to inspect next
- `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
- `crates/ridi-router-common/src/format.rs`
- `crates/ridi-router-routing/src/test_utils.rs`
- `crates/ridi-router-routing/src/router/walker.rs`
- `crates/ridi-router-routing/src/rmdf/io.rs`
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `todo-review-load-point-rules-from-tiles.md`
- `todo-review-point-record-real-offsets.md`
