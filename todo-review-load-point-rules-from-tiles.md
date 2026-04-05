# Review: Load point rules from tiles at runtime

## Verdict
Partially relevant.

## Summary
The todo is still relevant in its core claim: tile-backed routing does **not** load `MapDataPoint.rules` at runtime today.

However, the todo text is also **stale / incomplete**. The problem is not only in `crates/ridi-router-routing/src/map_data/graph.rs`, where tile-backed points are currently built with `rules: Vec::new()`. The deeper issue is that the tile pipeline does not currently produce usable rule data either:

- tile generation collects restriction relations, but does not materialize them into graph point rules
- RMDF writing reserves rule-related fields, but does not serialize rules
- RMDF reading exposes no rule accessors

So this is not just a small runtime loading gap. It is an end-to-end missing feature across generation, file format usage, and runtime decoding.

## Current evidence from the codebase

### 1. Runtime point loading still drops rules
In `crates/ridi-router-routing/src/map_data/graph.rs`, `get_point_from_tiles(...)` constructs tile-backed points with an empty rules vector:

- `crates/ridi-router-routing/src/map_data/graph.rs`:
  - `pub fn get_point_from_tiles(&self, tile_id: ..., osm_id: u64) -> MapDataPoint`
  - returned point contains `rules: Vec::new(), // TODO: Fetch rules from tiles`

That directly confirms the todo's immediate observation.

### 2. Router logic does depend on point rules
The missing runtime rules matter because walker logic actively reads `MapDataPoint.rules`:

- `crates/ridi-router-routing/src/router/walker.rs`
  - `get_segments_for_point_with_context(...)` filters by `MapDataRuleType::NotAllowed`
  - `get_fork_segments_for_segment_with_context(...)` filters by both `OnlyAllowed` and `NotAllowed`

So if tile-backed points always have `rules.is_empty()`, turn-restriction behavior from tile data is effectively absent at runtime.

### 3. RMDF format has placeholders for point rules
The file format already reserves space for rule data:

- `crates/ridi-router-common/src/format.rs`
  - `PointRecord` has `rules_offset` and `rules_count`
  - `RmdfHeader` has `rule_count`
  - `RuleRecord` exists
  - section constant `section::RULES` exists

This shows rule storage was anticipated in the format design.

### 4. RMDF reader currently has no rule-reading API
`MappedTile` exposes points, lines, line refs, tag sets, and tag values, but no rule accessor:

- `crates/ridi-router-routing/src/rmdf/io.rs`
  - has `get_points()`, `get_lines()`, `get_line_refs()`, `get_tag_set()`, `get_tag_value()`
  - no `get_rules()` / `get_rule()` / helper for point rule slices

`TileManager` likewise has point/line/tag access, but nothing for rules:

- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
  - `get_point_by_id(...)`
  - `get_line_by_index(...)`
  - `get_adjacent_by_id(...)`
  - no rule lookup API

### 5. RMDF writer does not serialize rules yet
The writer still leaves rule serialization unfinished:

- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
  - when serializing points, it writes:
    - `rules_offset: 0, // TODO: Calculate from rules`
    - `rules_count: point.rules.len() as u32`
  - `fn serialize_rules(&self, _graph: &GenerationGraph) -> Result<Vec<u8>>` returns `Ok(Vec::new())`
  - comment says `TODO: Implement based on MapDataRule structure`

So even if in-memory points had rules, the current tile writer would not emit them into RMDF.

### 6. Generation graph currently ignores relations
The upstream source of turn restrictions is also not wired through.

- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
  - collects restriction relations into the tile
- `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
  - inserts all relations into the generation graph
- but `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
  - `insert_relation(...)` is effectively a stub
  - comment says turn restrictions are skipped for now

That means the tile generation path is not even building `point.rules` before serialization.

### 7. Existing runtime tests pass without this feature
`cargo test -p ridi-router-routing` currently passes.

That does **not** mean the todo is obsolete. It means existing tests do not validate tile-backed rule loading.

There is also synthetic tile test data with no rules:

- `crates/ridi-router-routing/src/routing_api.rs`
  - synthetic fixture uses `rules_offset: 0` and `rules_count: 0`
- `crates/ridi-router-cli/tests/generate_route_cli.rs`
  - fixture point records also use `rules_offset: 0` and `rules_count: 0`

So the current test setup mostly avoids this area.

## The actual problem
The runtime symptom in the todo is real, but it is only the last missing step in a larger unfinished pipeline.

Today, tile-backed routing lacks end-to-end support for point rules because:

1. restriction relations are collected, but not transformed into `MapDataRule`s in generation
2. RMDF writer does not serialize rule payloads or valid per-point rule offsets
3. RMDF reader / tile manager do not expose rule lookups
4. runtime point materialization still hardcodes `rules: Vec::new()`

## What behavior is missing or risky

### Missing behavior
- tile-backed routing does not enforce turn restrictions via `MapDataPoint.rules`
- runtime behavior differs from rule-aware in-memory/test graph behavior
- walker logic can only honor rules in tests or synthetic in-memory setups, not from real tiles

### Risks
- the original todo understates the implementation size
- adding only the runtime loader in `graph.rs` would not fix anything if tiles still contain no serialized rules
- tests may continue passing while real routing behavior remains incomplete, because current fixtures do not exercise tile-backed rules

## Possible solution options

### Option A: Complete the intended RMDF point-rule pipeline
Implement rule support all the way through:

1. materialize `MapDataRule`s in tile generation from restriction relations
2. serialize rules into RMDF
3. add RMDF reader / tile-manager rule accessors
4. hydrate `MapDataPoint.rules` in `get_point_from_tiles(...)`

#### Tradeoffs
- larger change surface
- preserves current `MapDataPoint.rules` / walker design
- aligns with existing RMDF structures (`RuleRecord`, `rules_offset`, `rules_count`)

### Option B: Replace point-attached rules with a different runtime restriction mechanism
Instead of loading `Vec<MapDataRule>` per point, keep restrictions in a separate runtime index keyed by point / line transitions.

#### Tradeoffs
- could be more efficient or easier to query at runtime
- would likely require reworking walker logic and may make existing RMDF rule fields less relevant
- there is no evidence in the current codebase that such a replacement already exists

Given the current code, this would be a redesign, not a continuation of existing functionality.

## Tradeoffs / implications
- The current RMDF format already contains rule-related structures, so completing that path is the least disruptive direction.
- The hard part is likely not runtime loading; it is defining how OSM restriction relations become stable `MapDataRule` records, especially when rules refer to line refs that are tile-local and may cross tile boundaries.
- Cross-tile references need careful handling because `MapDataRule` stores `Vec<MapDataLineRef>`, and line refs are tile-scoped.
- Backward compatibility may matter if older tiles exist with `rule_count == 0` and all point `rules_count == 0`.

## Recommended direction
Treat this todo as an **end-to-end rules pipeline** task, not a `graph.rs` cleanup.

Recommended order:

1. confirm the intended mapping from OSM restriction relations to `MapDataRule`
2. implement relation-to-rule materialization in `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
3. finish RMDF rule serialization in `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
4. add RMDF read helpers in `crates/ridi-router-routing/src/rmdf/io.rs` and `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
5. update `crates/ridi-router-routing/src/map_data/graph.rs::get_point_from_tiles(...)` to hydrate `rules`
6. add tile-backed tests that prove walker behavior changes when rules are present

## Open questions
- Is `crates/ridi-router-common/src/format.rs::RuleRecord` sufficient to encode current `MapDataRule` semantics, especially the `from_lines` and `to_lines` vectors?
- How should rule line references be encoded when a rule references lines in another tile?
- Are there existing tiles in use that would need backward-compatible handling for missing rule sections?
- Should rule loading be eager per point or lazy via a dedicated query path?

## Suggested implementation starting point
Start with the writer/generation side, not `get_point_from_tiles(...)`.

A practical first pass:

1. inspect how restriction relations are expected to map to `MapDataRuleType::{OnlyAllowed, NotAllowed}`
2. implement `GenerationGraph::insert_relation(...)` in `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
3. once in-memory generated points actually have `point.rules`, finish `serialize_rules(...)` and correct `rules_offset`
4. only then add runtime decoding in:
   - `crates/ridi-router-routing/src/rmdf/io.rs`
   - `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
   - `crates/ridi-router-routing/src/map_data/graph.rs`

That order reduces the chance of implementing a reader for data that the writer still never produces.

## Related files to inspect next
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/router/walker.rs`
- `crates/ridi-router-routing/src/rmdf/io.rs`
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- `crates/ridi-router-common/src/format.rs`
- `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
- `crates/ridi-router-tiles/src/map_data/rule.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
- `crates/ridi-router-routing/src/routing_api.rs`
- `crates/ridi-router-cli/tests/generate_route_cli.rs`
