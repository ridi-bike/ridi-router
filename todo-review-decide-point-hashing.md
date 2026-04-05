# Review: point hashing after the tiles generation refactor

## Verdict
Relevant, but now as cleanup.

## Summary
The major `tiles` refactor already made the architectural choice.

`ridi-router-tiles` now uses an explicit generation model under `crates/ridi-router-tiles/src/generation`, and the RMDF writer builds the spatial index directly from point coordinates with `GridCellEntry`.

That means the old point-hashing question is no longer open architecture work. The remaining issue is that `GenerationGraph::generate_point_hashes()` is still an empty leftover, and both generator paths still call it.

## Current evidence
- `crates/ridi-router-tiles/src/generation/graph.rs`
  - `generate_point_hashes(&mut self)` is empty.
- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
  - still calls `graph.generate_point_hashes();`
- `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
  - still calls `graph.generate_point_hashes();`
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
  - `build_spatial_index()` groups connected points by `GridCellEntry::encode_cell_id(...)`
- `crates/ridi-router-common/src/format.rs`
  - the format has `GridCellEntry`
  - there is no point-hash field

## What changed after the refactor
Before the refactor, this sat next to an ambiguous `map_data` model.

After the refactor:
- tiles generation is explicitly `GenerationGraph` + `GenerationPoint` + `GenerationLine`
- the writer owns spatial-index construction
- nothing in the tiles path consumes point hashes

So the real work is to remove the dead hook and update comments.

## Recommended direction
1. Delete `GenerationGraph::generate_point_hashes()` from `crates/ridi-router-tiles/src/generation/graph.rs`.
2. Remove the call sites from:
   - `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
   - `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
3. Update nearby comments so they clearly say the RMDF writer builds the grid-cell spatial index directly.
4. Optionally clean up the mirrored placeholder in `crates/ridi-router-routing/src/map_data/generation_graph.rs` too, if that copy is still expected to track the tiles-side generation flow.

## Non-goal
Do not add a point-hash format or a new pre-write hashing pass unless a real consumer appears.

## Validation
- `rg "generate_point_hashes" crates/ridi-router-tiles crates/ridi-router-routing`
- `cargo test -p ridi-router-tiles`
