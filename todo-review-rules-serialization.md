# Review: RMDF rules serialization

## Verdict
**Partially relevant**

## Summary
The todo is **not obsolete**. The core claim is still true: RMDF rule serialization is not implemented, and tile-backed runtime loading still does not hydrate point rules.

However, the todo text is also **too narrow and partly stale**:

- the RMDF format shape for rules already exists via `RuleRecord`, so that part is not undefined anymore
- implementing only `serialize_rules(...)` would **not** complete runtime rule support
- the larger blocker is that tile generation still does not materialize turn restrictions into `MapDataRule`s in the first place

So this should be treated as an **end-to-end tile rules pipeline task**, not just a writer stub fix.

## Current evidence from the codebase

### 1. The writer stub still exists
In `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`:

- `write_tile_from_graph(...)` still calls `serialize_rules(&graph)`
- the header still exposes `rule_count`
- `serialize_rules(...)` still returns an empty vector

Concrete evidence:

- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
  - `serialize_rules(&graph)` is called during file writing
  - `rule_count` is derived from the returned bytes
  - `fn serialize_rules(&self, _graph: &GenerationGraph) -> Result<Vec<u8>> { Ok(Vec::new()) }`

This means generated RMDF tiles currently contain **no serialized rule payload**.

### 2. Point records still do not point at serialized rules
In the same writer file, `serialize_points(...)` still writes placeholder rule offsets:

- `rules_offset: 0, // TODO: Calculate from rules`
- `rules_count: point.rules.len() as u32`

File:
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`

Even if points had rules in memory, the per-point references into the flattened rules section are not computed.

### 3. The RMDF format already has a rule record shape
The todo plan says to “define the serialized RMDF shape for `MapDataRule`”, but that is already partly done.

In `crates/ridi-router-common/src/format.rs`:

- `PointRecord` has `rules_offset` and `rules_count`
- `RuleRecord` already exists with:
  - `from_lines_offset`
  - `from_lines_count`
  - `to_lines_offset`
  - `to_lines_count`
  - `rule_type`
- the RMDF header already includes `rule_count`
- section `RULES` already exists

So the format is **not completely undefined**. The missing part is how to serialize actual `MapDataRule` values into that shape and how to store the variable-length line-ref lists they need.

### 4. Tile generation still does not build rules from relations
This is the biggest reason the todo is broader than it sounds.

In `crates/ridi-router-tiles/src/map_data/generation_graph.rs`:

- `insert_node(...)` initializes `rules: Vec::new()`
- `insert_relation(...)` is still a stub
- the comment explicitly says turn restrictions are skipped for now

Concrete code behavior:
- `insert_relation(&mut self, _relation: OsmRelation) -> Result<(), MapDataError>`
- comment: “For now, we'll skip relation processing as it's not critical for basic routing”

But the tile generation pipeline does call that method:

- `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`

Both insert relations into the generation graph, but the current implementation discards them.

So `serialize_rules(...)` is not the first missing step. In normal tile generation, `point.rules` is not being populated meaningfully before serialization.

### 5. Runtime RMDF reading still has no rule access path
In `crates/ridi-router-routing/src/rmdf/io.rs`, the mapped RMDF reader exposes:

- `get_spatial_index()`
- `get_points()`
- `get_lines()`
- `get_line_refs()`
- tag accessors

There is **no equivalent rule accessor** such as reading the RULES section or resolving a point’s `rules_offset` / `rules_count`.

In `crates/ridi-router-routing/src/rmdf/tile_manager.rs`, there is likewise no rule-loading API.

### 6. Runtime point hydration still hardcodes empty rules
In `crates/ridi-router-routing/src/map_data/graph.rs`, when a point is rebuilt from tiles:

- `rules: Vec::new(), // TODO: Fetch rules from tiles`

That means tile-backed routing still ignores any rule data even if RMDF tiles eventually contained it.

### 7. The routing logic does rely on `MapDataPoint.rules`
The missing pipeline matters because the walker does consume point rules.

In `crates/ridi-router-routing/src/router/walker.rs`:

- it filters `center_point_data.rules` by `MapDataRuleType::NotAllowed`
- it also handles `MapDataRuleType::OnlyAllowed`
- segment choices are directly gated by those rule lists

So the runtime contract already exists. What is missing is getting tile-backed points to contain the rules.

### 8. Existing rule behavior is currently proven mostly through in-memory test helpers, not tile-backed RMDF rules
In `crates/ridi-router-routing/src/test_utils.rs`:

- restriction relations are transformed into `MapDataRule`
- those rules are attached directly to points in test graphs

That explains why walker rule tests can pass even though RMDF rule serialization/loading is unfinished.

Separately, synthetic RMDF fixtures in tile-backed tests use empty rule fields:

- `crates/ridi-router-routing/src/routing_api.rs`
- `crates/ridi-router-cli/tests/generate_route_cli.rs`

Both construct `PointRecord`s with:
- `rules_offset: 0`
- `rules_count: 0`

So current tests do not prove tile-backed rule support.

### 9. Current package tests passing does not mean this todo is covered
Running the suggested package tests today still passes:

- `cargo test -p ridi-router-tiles`
- `cargo test -p ridi-router-routing`

That is consistent with the evidence above: existing tests do not require tile-backed RMDF rules to work end to end.

## Why this todo is only partially accurate

### What is stale in the todo text
The todo says to:

- define the serialized RMDF shape for `MapDataRule`

That is only partly true now. The shared RMDF format already contains the main record shape in `crates/ridi-router-common/src/format.rs::RuleRecord` plus point-level `rules_offset` / `rules_count`.

### What is still genuinely missing
The real missing behavior is:

1. generation-time construction of `MapDataRule`s from OSM restriction relations
2. serialization of those rules into the RMDF `RULES` section
3. correct per-point `rules_offset` bookkeeping
4. RMDF read helpers for the rules section
5. runtime hydration of `MapDataPoint.rules` from tiles
6. tile-backed tests that prove routing behavior changes when rules are present

## The actual problem
Tile-backed routing has an unfinished turn-restriction pipeline.

Today:

- generation accepts relations but drops them in `insert_relation(...)`
- writer emits no rule payloads
- point records do not compute real rule offsets
- runtime does not read rules back
- tile-backed points are materialized with empty `rules`

So the original todo is directionally correct, but it understates the scope.

## What behavior is missing or risky

### Missing behavior
- real RMDF tiles cannot carry usable point rule data end to end
- tile-backed runtime cannot enforce turn restrictions through `MapDataPoint.rules`
- acceptance criteria like “runtime code can read them back” are not met

### Risk
- implementing only `serialize_rules(...)` would create a false sense of completion
- tests may still pass while real tile-backed routing remains incomplete
- the mapping from restriction relations to serialized tile-local line references needs care, especially because rules refer to line refs rather than raw OSM way IDs

## Possible solution options

### Option A: Complete the intended RMDF point-rule pipeline
Keep the existing architecture:

- `MapDataRule` stays attached to `MapDataPoint`
- RMDF keeps `PointRecord.rules_offset` / `rules_count`
- `RuleRecord` remains the serialized unit

Work needed:

1. implement relation-to-rule generation in `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
2. define how `from_lines` / `to_lines` are flattened for RMDF writing
3. finish `serialize_rules(...)` in `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
4. compute real `rules_offset` values per point
5. add RMDF rule readers in `crates/ridi-router-routing/src/rmdf/io.rs`
6. add tile-manager helpers in `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
7. hydrate `rules` in `crates/ridi-router-routing/src/map_data/graph.rs`
8. add tile-backed round-trip and routing tests

### Option B: Replace point-attached runtime rules with a separate restriction index
Instead of loading `Vec<MapDataRule>` onto each point, restrictions could be stored in a different runtime structure keyed by point and line transitions.

This could reduce repeated decoding work, but it would be a larger design change because the current walker already expects `MapDataPoint.rules`.

## Tradeoffs / implications

### Option A tradeoffs
Pros:
- aligns with current runtime walker design
- fits existing RMDF fields already present in `ridi-router-common`
- least disruptive across crates

Cons:
- requires solving variable-length serialization for `from_lines` and `to_lines`
- relation ingestion and tile-local line reference mapping may be tricky
- spans multiple crates (`ridi-router-tiles`, `ridi-router-routing`, `ridi-router-common`)

### Option B tradeoffs
Pros:
- could produce a cleaner dedicated restriction lookup path

Cons:
- conflicts with today’s `MapDataPoint.rules` consumer model
- makes existing RMDF rule fields less useful or requires another format change
- much larger than the original todo suggests

## Recommended direction
Treat this todo as a **broader end-to-end RMDF turn-restriction task** and keep the current architecture.

Practical recommendation:

- keep `RuleRecord` and point-attached rule loading
- do **not** treat `serialize_rules(...)` as an isolated fix
- rewrite the implementation plan around the full pipeline: relation ingestion -> RMDF writing -> RMDF reading -> runtime hydration -> tile-backed tests

## Open questions

1. Is `RuleRecord` sufficient as-is for the intended `MapDataRule` semantics, or is an additional side table for line refs needed?
2. What is the intended encoding for `rule_type` values in RMDF, and is it documented anywhere beyond the enum names?
3. How should `from_lines` / `to_lines` be resolved when restriction-related lines cross tile boundaries?
4. Should backward compatibility be considered for older tiles that have `rule_count == 0` and per-point `rules_count == 0`?
5. Are there any existing production tile datasets that would need regeneration once this is implemented?

## Suggested implementation starting point
1. Start with `crates/ridi-router-tiles/src/map_data/generation_graph.rs::insert_relation(...)`.
   - Without real `MapDataRule`s in the generation graph, serialization work is mostly disconnected from real tile generation.
2. Confirm the intended mapping from OSM restriction relations to `MapDataRuleType::{OnlyAllowed, NotAllowed}`.
   - `crates/ridi-router-routing/src/test_utils.rs` is the best existing reference for current semantics.
3. Then implement the writer side in `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`.
   - finish `serialize_rules(...)`
   - compute per-point `rules_offset`
4. After that, add RMDF read accessors in `crates/ridi-router-routing/src/rmdf/io.rs` and `crates/ridi-router-routing/src/rmdf/tile_manager.rs`.
5. Finally, update `crates/ridi-router-routing/src/map_data/graph.rs` so tile-backed point hydration loads real rules.
6. Add tile-backed tests that fail without the full pipeline.

## Related files to inspect next
- `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
- `crates/ridi-router-common/src/format.rs`
- `crates/ridi-router-routing/src/rmdf/io.rs`
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/router/walker.rs`
- `crates/ridi-router-routing/src/test_utils.rs`
- `crates/ridi-router-routing/src/routing_api.rs`
- `crates/ridi-router-cli/tests/generate_route_cli.rs`
- `todo-point-record-real-offsets.md`
- `todo-review-load-point-rules-from-tiles.md`
