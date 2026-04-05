# Phase 1: Synthetic RMDF fixture foundation

## Goal
Make the synthetic RMDF test helper capable of expressing the data that nearest-point filtering now depends on.

## Why this phase exists
`TileManager::get_closest_to_coords(...)` currently ignores tag-based filtering and only does a full-tile scan. The existing helper in `crates/ridi-router-test-support/src/lib.rs` can write points, lines, line refs, and rules, but it cannot write tag values/tag sets, and it also cannot directly exercise the spatial-index path.

## Files
- `crates/ridi-router-test-support/src/lib.rs`
- optional small test touch-ups in existing synthetic-fixture callers if defaults need to be made explicit

## Planned changes
1. Extend `ridi_router_test_support::rmdf::TileSpec` with optional RMDF sections needed by nearest-point tests:
   - `spatial_index: Vec<GridCellEntry>`
   - `tag_values: Vec<String>`
   - `tag_sets: Vec<TagSetRecord>`
2. Update `write_tile(...)` to serialize sections in real RMDF order:
   - spatial index
   - points
   - lines
   - line refs
   - tag value `StringEntry` records + string pool
   - tag sets
   - rules + rule-line-ref payload
3. Set RMDF header counts and section offsets correctly:
   - `spatial_grid_cell_count`
   - `tag_value_count`
   - `tag_set_count`
   - existing rule count
4. Keep empty-section defaults so current tests keep working without change.
5. Add tiny helper utilities as needed for:
   - building `StringEntry` offsets
   - creating `TagSetRecord` values with `TagSetRecord::NONE`
   - building `GridCellEntry` values for tests

## Notes
- This phase intentionally broadens the helper slightly beyond the todo’s minimum tag support. Without optional `spatial_index` support, the ring-scan path is hard to test directly.
- Reuse the serialization layout from `crates/ridi-router-tiles/src/rmdf/generator/writer.rs` as the reference shape.

## Validation
- Existing synthetic-fixture users still pass with empty tags/spatial index.
- A small helper-level regression test or read-back smoke test confirms:
  - tag values can be read through `MappedTile::get_tag_value(...)`
  - tag sets can be read through `MappedTile::get_tag_set(...)`
  - spatial index can be read through `MappedTile::get_spatial_index(...)`

## Exit criteria
- Tests can create tagged lines and optional grid-cell slices without hand-writing RMDF bytes.
- No public API changes outside test support.
