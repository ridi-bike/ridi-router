# Review: real point-record offsets in RMDF generation

## Verdict
Still relevant for `lines_offset`.

The `rules_offset` part should stay with the separate restrictions pipeline.

## Summary
There is still a real writer bug in `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`:
- point records are written with `lines_offset: 0`
- line refs are serialized as a flattened side table
- runtime tile loading slices that side table using `lines_offset` + `lines_count`

So `lines_offset` still needs a real implementation.

By contrast, `rules_offset` is no longer a small local writer fix. After the tiles refactor, generation-side points do not carry rules at all. Rule offsets only make sense once the separate restriction pipeline exists.

## Current evidence
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
  - `serialize_points()` still writes `lines_offset: 0`
  - `serialize_line_refs()` already flattens per-point line refs in file order
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
  - `get_adjacent_by_id()` slices line refs using `point.lines_offset` and `point.lines_count`
- `crates/ridi-router-common/src/format.rs`
  - `PointRecord` expects real `lines_offset` / `lines_count`

## Scope decision
### In scope here
- compute correct `lines_offset` values
- keep point ordering identical between point serialization and line-ref serialization
- add a round-trip test that proves adjacency reads the expected line refs

### Out of scope here
- `rules_offset`
- restriction materialization
- RMDF rule serialization
- runtime rule loading

Those belong to:
- `todo-review-turn-restrictions-rule-model.md`
- `todo-review-rules-serialization.md`
- `todo-review-load-point-rules-from-tiles.md`

## Recommended direction
1. Factor the shared point ordering used by:
   - `build_spatial_index()`
   - `serialize_points()`
   - `serialize_line_refs()`
2. While flattening line refs, record each point's starting offset and count.
3. Use that metadata when writing `PointRecord`.
4. Add a tiny writer/reader round-trip test.

## Validation
- each point record has the expected `lines_offset` / `lines_count`
- `TileManager::get_adjacent_by_id()` reads the right line slice for every point in the fixture
- `cargo test -p ridi-router-tiles`
- `cargo test -p ridi-router-routing`
