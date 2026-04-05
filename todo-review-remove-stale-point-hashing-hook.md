# Review: stale point-hashing hook after the tiles generation refactor

## Verdict
Relevant cleanup.

## Summary
The architectural question is already settled: RMDF tiles use the writer-built grid-cell spatial index, not a separate point-hashing pass.

What remains is dead plumbing:
- `GenerationGraph::generate_point_hashes()` in `crates/ridi-router-tiles/src/generation/graph.rs` is still empty
- both tiles generation paths still call it
- both call sites still describe it as spatial-index work even though `RmdfWriter::build_spatial_index(...)` now owns that job

This todo is now purely about removing stale hooks and misleading comments.

## Current evidence
- `crates/ridi-router-tiles/src/generation/graph.rs`
  - `generate_point_hashes(&mut self)` is still an empty no-op
  - supported restriction relations are already materialized in `insert_relation(...)`, so point hashing is no longer tied to unfinished relation work
- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
  - still calls `graph.generate_point_hashes();`
  - nearby comment says `Generate point hashes for spatial indexing`
- `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
  - same stale call and comment
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
  - spatial index is built from ordered connected points via `build_point_layout(...)` + `build_spatial_index(...)`
- `crates/ridi-router-common/src/format.rs`
  - RMDF format has `GridCellEntry`
  - there is still no point-hash field

## Related cleanup
`crates/ridi-router-routing/src/map_data/generation_graph.rs` still carries its own placeholder `generate_point_hashes()` and stub `insert_relation(...)`. That file looks increasingly like legacy duplicate scaffolding and should be reviewed separately, but it does not block the tiles-side cleanup above.

## Recommended direction
1. Delete `GenerationGraph::generate_point_hashes()` from `crates/ridi-router-tiles/src/generation/graph.rs`.
2. Remove the call sites from:
   - `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
   - `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
3. Update comments so they say the writer builds the RMDF grid-cell spatial index directly from point coordinates and connectivity.
4. Optionally follow up on the routing-side placeholder copy in `crates/ridi-router-routing/src/map_data/generation_graph.rs`.

## Non-goal
Do not introduce a new point-hash field or pre-write hashing pass unless a real consumer appears.

## Validation
- `rg "generate_point_hashes" crates/ridi-router-tiles crates/ridi-router-routing`
- `cargo test -p ridi-router-tiles`
