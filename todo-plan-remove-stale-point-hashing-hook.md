# Plan: remove stale point-hashing hook

## Scope

### Tiles
- Delete `GenerationGraph::generate_point_hashes()` from `crates/ridi-router-tiles/src/generation/graph.rs`.
- Remove stale call sites from:
  - `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
  - `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
- Update nearby comments to say the RMDF writer builds the grid-cell spatial index directly from point coordinates and connectivity.

### Routing cleanup
- Delete legacy `crates/ridi-router-routing/src/map_data/generation_graph.rs`.
- Remove `pub mod generation_graph;` from `crates/ridi-router-routing/src/map_data/mod.rs`.
- Remove now-unused `MapDataError` from `crates/ridi-router-routing/src/map_data/mod.rs`.
- Remove now-unused routing-side `ElementTags` helpers from `crates/ridi-router-routing/src/map_data/graph.rs`:
  - `get_or_create(...)`
  - `get_tag_value_ref(...)`

### Manifest dedupe
- Delete routing wrapper modules that only re-export common manifest types:
  - `crates/ridi-router-routing/src/rmdf/generator/manifest.rs`
  - `crates/ridi-router-routing/src/rmdf/generator/mod.rs`
- Remove `pub mod generator;` from `crates/ridi-router-routing/src/rmdf/mod.rs`.
- Update routing imports to use `ridi_router_common::manifest::TileManifest` directly.

### OSM type dedupe
- Add a shared OSM model module in `crates/ridi-router-common`.
- Move the canonical definitions there:
  - `OsmNode`
  - `OsmWay`
  - `OsmRelationMemberType`
  - `OsmRelationMemberRole`
  - `OsmRelationMember`
  - `OsmRelation`
  - `OsmWay::{is_one_way,is_roundabout}`
- Update both crates to import the shared types directly from `ridi-router-common`.
- Delete the duplicate local definitions:
  - `crates/ridi-router-routing/src/map_data/osm.rs`
  - `crates/ridi-router-tiles/src/osm_data/types.rs`
- Update module exports/imports accordingly so there are no local shims or re-export compatibility layers.

## Constraints
- Do not introduce any new point-hash field or pre-write hashing pass.
- Keep RMDF spatial indexing owned by `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`.

## Validation
- `rg "generate_point_hashes" crates/ridi-router-tiles crates/ridi-router-routing`
- `cargo test -p ridi-router-tiles`
- `cargo check -p ridi-router-common -p ridi-router-routing -p ridi-router-tiles --lib --tests`
