# Implementation phase 3 — runtime tile IO and `MapDataPoint.rules` hydration

## Goal
Load serialized RMDF rules back into the runtime graph so routing can consume them through the existing `MapDataPoint.rules` model.

## Why this is phase 3
The runtime already expects point-attached rules. After phase 2, the tile format contains enough data to hydrate them.

## Scope
Add rule accessors in runtime tile IO and hydrate `MapDataRule` values in `MapDataGraph::get_point_from_tiles(...)`.

## Main changes

### 1. Extend mapped-tile IO
Update `crates/ridi-router-routing/src/rmdf/io.rs` with accessors for the `RULES` section, likely including:
- full `RuleRecord[]` view using `header.rule_count`
- full flattened rule-line-ref payload slice
- optionally a helper to slice a point's rules directly

The payload starts after `rule_count * size_of::<RuleRecord>()` bytes within the `RULES` section.

### 2. Add TileManager helpers
Update `crates/ridi-router-routing/src/rmdf/tile_manager.rs` with helpers that can:
- load a point's `RuleRecord`s
- read the referenced `from` and `to` payload slices
- return runtime-friendly data for hydration

### 3. Hydrate runtime rules in `MapDataGraph`
Update `crates/ridi-router-routing/src/map_data/graph.rs` so `get_point_from_tiles(...)`:
- loads the point record
- loads adjacent lines as today
- loads the point's `RuleRecord`s
- loads the referenced flattened payload slices
- converts stored line indices into `MapDataLineRef::new(point_tile_id, line_index as u64)`
- maps serialized rule types to `MapDataRuleType::{OnlyAllowed, NotAllowed}`
- fills `MapDataPoint.rules`

### 4. Preserve current runtime shape
Do **not** redesign walker or runtime rule structures in this phase.

The intended output remains:
- `MapDataPoint.rules: Vec<MapDataRule>`

## Suggested implementation notes
- Keep the line refs tile-local in the first pass:
  - `MapDataLineRef::new(point_tile_id, line_index as u64)`
- If the RMDF rule payload is malformed or out of bounds, fail loudly at IO boundaries rather than silently producing partial rules.
- Prefer helper methods in tile IO / TileManager over embedding raw byte slicing in `MapDataGraph`.

## Tests for this phase
Add runtime-loading tests that prove:
- a point with serialized rules hydrates non-empty `MapDataPoint.rules`
- `from_lines` and `to_lines` resolve to the expected tile-local `MapDataLineRef`s
- `OnlyAllowed` and `NotAllowed` decode correctly
- points with no rules still hydrate normally

If there is already tile fixture support, use it. Otherwise build a focused synthetic RMDF fixture in test support.

## Done when
- runtime tile IO can read `RuleRecord[]` and payload slices
- `MapDataGraph::get_point_from_tiles(...)` hydrates real `MapDataRule`s
- runtime still uses the existing `MapDataPoint.rules` API unchanged

## Likely files
- `crates/ridi-router-routing/src/rmdf/io.rs`
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- related runtime-loading tests
