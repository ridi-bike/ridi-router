# Review: decide whether point hashing stays or is removed

## Verdict
Partially relevant.

## Summary
The codebase already behaves as if **grid-cell indexing is the chosen spatial-index mechanism**, not point hashing. The RMDF writer builds a `GridCellEntry` index directly from point coordinates, and RMDF reading/routing code expects that grid-cell structure.

However, the todo is **not fully obsolete**, because stale no-op `generate_point_hashes()` methods and call sites are still present. That leaves the spatial-index story ambiguous in the generation path even though the actual implementation has moved on.

So the decision is mostly already made by the code, but the cleanup has not been completed.

## Current evidence from the codebase

1. **`generate_point_hashes()` is a placeholder with no implementation**
   - `crates/ridi-router-tiles/src/map_data/generation_graph.rs:162`
   - `crates/ridi-router-routing/src/map_data/generation_graph.rs:162`

   Both define:
   - `pub fn generate_point_hashes(&mut self)`
   - comment: `TODO: Implement spatial hashing if needed`
   - comment: `This may not be needed if we're relying on the grid cell spatial index in build_spatial_index() instead`

   This is direct evidence that point hashing is not implemented.

2. **The placeholder is still called in tile generation flows**
   - `crates/ridi-router-tiles/src/rmdf/generator/mod.rs:936`
   - `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs:431`

   Both paths call `graph.generate_point_hashes();` immediately before returning the graph for RMDF writing.

3. **The actual RMDF spatial index is built independently from graph coordinates**
   - `crates/ridi-router-tiles/src/rmdf/generator/writer.rs:44`
   - `crates/ridi-router-tiles/src/rmdf/generator/writer.rs:153`

   `RmdfWriter::write_tile_from_graph()` calls `build_spatial_index(&graph)`, and `build_spatial_index()`:
   - builds a point-to-lines map from the graph’s lines
   - groups connected points into cells via `GridCellEntry::encode_cell_id(point.lat, point.lon, 100)`
   - sorts cells by `cell_id`
   - writes `GridCellEntry { cell_id, points_offset, points_count }`

   That is a concrete grid-cell spatial index implementation. It does not depend on any hash precomputation from `generate_point_hashes()`.

4. **Point serialization is already aligned to the grid-cell index, not to point hashes**
   - `crates/ridi-router-tiles/src/rmdf/generator/writer.rs:205`
   - `crates/ridi-router-tiles/src/rmdf/generator/writer.rs:219`
   - `crates/ridi-router-tiles/src/rmdf/generator/writer.rs:298`

   The writer explicitly groups points by grid cell and serializes them in sorted cell order so that point records match `points_offset` / `points_count` in the spatial index. That is a strong sign that the real invariant is “points ordered by grid cell”, not “points looked up by hash”.

5. **The file format itself encodes grid-cell entries, not point hashes**
   - `crates/ridi-router-common/src/format.rs:59`
   - `crates/ridi-router-common/src/format.rs:68`

   `GridCellEntry` contains:
   - `cell_id`
   - `points_offset`
   - `points_count`

   There is no point-hash field in the RMDF format here.

6. **The runtime reader/router also expects grid-cell spatial indexing**
   - `crates/ridi-router-routing/src/rmdf/io.rs:73`
   - `crates/ridi-router-routing/src/rmdf/tile_manager.rs:118`
   - `crates/ridi-router-routing/src/rmdf/tile_manager.rs:121`
   - `crates/ridi-router-routing/src/rmdf/tile_manager.rs:123`

   `MappedTile::get_spatial_index()` returns `&[GridCellEntry]`. `TileManager::get_closest_to_coords()` loads that spatial index and computes a grid `cell_id`, although it still falls back to a linear scan over points for now. Even this unfinished consumer is framed around grid cells, not point hashes.

7. **Tests currently pass without any real point hashing implementation**
   - Verified with: `cargo test -p ridi-router-tiles`
   - Result: passed (`86 passed, 0 failed`, plus one ignored integration-style test)

   This is practical evidence that current behavior does not depend on point hashing.

## Why this is only partially relevant, not fully obsolete
The original todo says to make an explicit choice between:
- removing the dead placeholder, or
- implementing point hashing with a real consumer and tests.

That framing is partly stale.

The codebase has already, in practice, chosen the first direction’s underlying design: **grid-cell indexing**. The writer, file format, and reader all revolve around grid cells. There is no sign of an unfinished consumer that specifically needs point hashes.

But the todo is still relevant because the code has not been cleaned up to reflect that decision:
- the placeholder function still exists
- generation still calls it
- comments still speak in hypothetical terms
- there are duplicate no-op implementations in both `ridi-router-tiles` and `ridi-router-routing`

So the open work is no longer “decide the architecture”. It is “remove stale point-hashing hooks and make the grid-cell story explicit”.

## The actual problem
The remaining problem is **code clarity and misleading generation flow**, not missing functionality.

Today, a reader of the generation path sees:
- `graph.generate_point_hashes();`
- followed by a writer that actually builds a grid-cell spatial index on its own.

That implies a dependency that does not exist.

## What behavior is missing or risky
1. **Misleading maintenance signal**
   Future work could preserve or expand a dead API because it looks important.

2. **Wrong implementation direction**
   Someone could try to add point-hash storage later even though the current RMDF format and writer already center on `GridCellEntry`.

3. **Hidden duplication**
   The same dead placeholder exists in both crates, which increases the chance that cleanup happens in one place but not the other.

4. **The todo text is partly wrong now**
   It suggests the design choice is still undecided. The code says otherwise.

## Possible solution options

### Option A: Remove the placeholder and its call sites
Remove `generate_point_hashes()` from both generation graph implementations and delete the two call sites in tile generation.

Also update nearby comments so the generation path clearly says the RMDF writer builds the grid-cell spatial index directly.

### Option B: Keep the method, but rename/reframe it
If the project wants a stable hook in the generation pipeline, keep a method but rename it to something accurate, such as a validation or pre-write preparation step.

This only makes sense if the method gains a real responsibility. Keeping a no-op under the old name does not help.

### Option C: Actually implement point hashing
Add real hash data, a real consumer, tests, and likely format changes.

Current evidence does not justify this. Nothing in the writer, format, or runtime currently needs it.

## Tradeoffs / implications

### Removing the placeholder
Pros:
- aligns code with actual behavior
- reduces ambiguity
- avoids dead API surface
- makes later spatial-index optimization work clearer

Cons:
- requires touching both crate copies of `GenerationGraph`
- any planned future point-hash experiment would need to be reintroduced explicitly

### Implementing point hashing
Pros:
- preserves the original todo’s second branch

Cons:
- no current consumer
- likely unnecessary design complexity
- would diverge from the present RMDF grid-cell format and reader expectations
- adds implementation and test burden without evidence of need

## Recommended direction
Treat point hashing as obsolete and remove the stale placeholder path.

More specifically:
- delete `generate_point_hashes()` from:
  - `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
  - `crates/ridi-router-routing/src/map_data/generation_graph.rs`
- delete its call sites from:
  - `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
  - `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
- update comments in generation/writer code so they clearly describe the actual model: the RMDF writer groups connected points by grid cell and writes `GridCellEntry` metadata.

That would satisfy the real intent of the todo without inventing unused functionality.

## Open questions
1. Why does `ridi-router-routing` still carry its own `GenerationGraph` copy with the same dead placeholder? Is that crate copy still meant to stay in sync with `ridi-router-tiles`, or is it legacy duplication?
2. Should the generation path expose a more explicit name for the real pre-write step, or is direct writer-side indexing clear enough once the dead method is removed?
3. Separately from this todo: should `TileManager::get_closest_to_coords()` start using `GridCellEntry` for a true localized search instead of a full linear scan? That is related spatial-index follow-up work, but distinct from point hashing.

## Suggested implementation starting point
1. Confirm there are no remaining references beyond the four locations already found by `rg`.
2. Remove the no-op method in both `GenerationGraph` implementations.
3. Remove the two generator call sites.
4. Update comments in `crates/ridi-router-tiles/src/rmdf/generator/writer.rs` and the surrounding generation flow so they consistently describe grid-cell indexing.
5. Re-run:
   - `cargo test -p ridi-router-tiles`
   - optionally workspace checks if the routing crate depends on the mirrored API shape.

## Related files to inspect next
- `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
- `crates/ridi-router-routing/src/map_data/generation_graph.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
- `crates/ridi-router-common/src/format.rs`
- `crates/ridi-router-routing/src/rmdf/io.rs`
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
