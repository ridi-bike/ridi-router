# Phase 4 — Move canonical OSM types into `ridi-router-common`

## Goal
Create one shared OSM model in `crates/ridi-router-common` and make both routing and tiles import it directly.

## Scope
- Add a shared OSM module in `crates/ridi-router-common`.
- Move the canonical definitions there:
  - `OsmNode`
  - `OsmWay`
  - `OsmRelationMemberType`
  - `OsmRelationMemberRole`
  - `OsmRelationMember`
  - `OsmRelation`
  - `OsmWay::{is_one_way,is_roundabout}`
- Update both crates to import these types directly from `ridi_router_common`.
- Delete duplicate local definitions:
  - `crates/ridi-router-routing/src/map_data/osm.rs`
  - `crates/ridi-router-tiles/src/osm_data/types.rs`
- Update module exports/imports so there are no local shims or compatibility re-exports.

## Why this is the final phase
This is the widest refactor. It touches both crates and many imports, so it is safest after the smaller cleanup phases are complete.

## Likely implementation shape
1. Add `crates/ridi-router-common/src/osm.rs`.
2. Export it from `crates/ridi-router-common/src/lib.rs`.
3. Move the shared structs/enums/impls into that module.
4. Update all routing imports from `crate::map_data::osm::...` to `ridi_router_common::osm::...`.
5. Update all tiles imports from `crate::osm_data::...` to `ridi_router_common::osm::...` where they refer to OSM model types.
6. Keep tiles-only helpers such as `in_memory_pbf` local under `crates/ridi-router-tiles/src/osm_data/`.
7. Delete the duplicate files and remove their module exports.

## Important constraint
Do not leave behind re-export compatibility layers. After this phase, call sites should import the OSM model from `ridi_router_common` directly.

## Validation
- `rg "map_data::osm::|osm_data::types::|crate::osm_data::\{.*Osm|crate::map_data::osm::" crates/ridi-router-routing crates/ridi-router-tiles`
- `cargo check -p ridi-router-common -p ridi-router-routing -p ridi-router-tiles --lib --tests`
- `cargo test -p ridi-router-tiles`

## Done when
- `ridi-router-common` owns the canonical OSM model.
- Routing and tiles compile against the shared types.
- The duplicate local OSM type files are gone.
- No local shim module remains for those types.
