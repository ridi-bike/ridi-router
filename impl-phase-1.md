# Phase 1 — Remove the stale point-hashing hook in `ridi-router-tiles`

## Goal
Delete the unused pre-write point-hashing step and make the tiles pipeline describe the real ownership model: RMDF spatial indexing is built in the writer from point coordinates and connectivity.

## Scope
- Delete `GenerationGraph::generate_point_hashes()` from `crates/ridi-router-tiles/src/generation/graph.rs`.
- Remove the stale call sites from:
  - `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
  - `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
- Update nearby comments so they say the RMDF writer builds the grid-cell spatial index directly from graph data during tile writing.

## Why this is its own phase
This is the smallest, safest change and it enforces the main constraint early:
- no new point-hash field
- no pre-write hashing pass
- RMDF indexing stays owned by `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`

## Implementation checklist
1. Remove the empty `generate_point_hashes()` method.
2. Remove both invocations after restriction materialization.
3. Rewrite comments around graph construction and tile writing so they describe the current behavior accurately.
4. Confirm nothing in `writer.rs` expects a precomputed hash field.

## Validation
- `rg "generate_point_hashes" crates/ridi-router-tiles crates/ridi-router-routing`
- `cargo test -p ridi-router-tiles`

## Done when
- `generate_point_hashes` no longer exists anywhere.
- Tile generation comments no longer mention a pre-write hashing step.
- Tiles tests still pass.
