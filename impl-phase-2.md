# Phase 2 — Remove dead routing-side generation graph code

## Goal
Delete the legacy routing generation-graph path that is no longer part of the live data flow.

## Scope
- Delete `crates/ridi-router-routing/src/map_data/generation_graph.rs`.
- Remove `pub mod generation_graph;` from `crates/ridi-router-routing/src/map_data/mod.rs`.
- Remove the now-unused `MapDataError` type from `crates/ridi-router-routing/src/map_data/mod.rs`.
- Remove the now-unused routing-side `ElementTags` helpers from `crates/ridi-router-routing/src/map_data/graph.rs`:
  - `get_or_create(...)`
  - `get_tag_value_ref(...)`
- Clean up any comments in `map_data/graph.rs` that only existed to justify those dead helpers.

## Why this comes before broader dedupe work
This phase shrinks the routing crate first. That makes later import dedupe easier because the old code path and its helper APIs are already gone.

## Implementation checklist
1. Verify `generation_graph.rs` has no live callers outside its own module export.
2. Delete the file and module export.
3. Remove `MapDataError` and fix any compile fallout.
4. Delete the unused `ElementTags` helper methods and any now-unused imports or fields they depended on.
5. Re-run search to confirm there are no references to:
   - `generation_graph`
   - `MapDataError`
   - removed helper methods

## Validation
- `rg "generation_graph|MapDataError|get_or_create\(|get_tag_value_ref\(" crates/ridi-router-routing`
- `cargo check -p ridi-router-routing --lib --tests`

## Done when
- The routing crate has no `generation_graph` module.
- `MapDataError` is gone.
- `ElementTags` only contains behavior needed by the runtime/test graph code that still exists.
