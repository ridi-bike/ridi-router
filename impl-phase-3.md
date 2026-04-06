# Phase 3 — Deduplicate manifest types in the routing crate

## Goal
Stop routing from carrying local wrapper modules that only re-export common manifest types.

## Scope
- Delete:
  - `crates/ridi-router-routing/src/rmdf/generator/manifest.rs`
  - `crates/ridi-router-routing/src/rmdf/generator/mod.rs`
- Remove `pub mod generator;` from `crates/ridi-router-routing/src/rmdf/mod.rs`.
- Update routing imports to use `ridi_router_common::manifest::TileManifest` directly.
- Update any tests/helpers in routing that still import manifest types through the deleted wrapper path.

## Why this is separate from Phase 2
It is also cleanup, but it changes a public internal import path used across several routing files. Keeping it separate makes review simpler and isolates any fallout from import-path changes.

## Expected touch points
Based on current usage, at least these files need import updates:
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- `crates/ridi-router-routing/src/routing_api.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- any routing tests that still reference `crate::rmdf::generator::manifest::TileManifest`

## Implementation checklist
1. Replace wrapper-path imports with `ridi_router_common::manifest::TileManifest`.
2. Remove the dead routing `rmdf::generator` wrapper module.
3. Re-run search for `generator::manifest::TileManifest` and clear remaining references.
4. Keep the tiles crate unchanged here; this phase is routing-only.

## Validation
- `rg "generator::manifest::TileManifest|super::generator::manifest::TileManifest|rmdf::generator::manifest::TileManifest" crates/ridi-router-routing`
- `cargo check -p ridi-router-routing --lib --tests`

## Done when
- Routing imports manifest types directly from `ridi_router_common`.
- The routing `rmdf/generator` wrapper modules are deleted.
- `crates/ridi-router-routing/src/rmdf/mod.rs` no longer exposes `generator`.
