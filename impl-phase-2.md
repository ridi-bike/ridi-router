# Phase 2: Migrate OSM ingestion and graph-building to the new model

Derived from `todo-plan-convert-map-data-line-helpers-tiles.md`.

## Goal

Move the active tile-generation producers over to the new module layout so the crate builds `GenerationGraph` from `osm_data` types directly.

After this phase, the **single-PBF and multi-PBF generation paths** should both consume:

- `crate::osm_data::*` for raw OSM input types
- `crate::generation::*` for transformed build-time types

## Scope

### In scope

- `crates/ridi-router-tiles/src/osm_data/in_memory_pbf.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/intermediate.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
- import migration from `crate::map_data::*` to `crate::generation::*` / `crate::osm_data::*`

### Not in scope

- Final deletion of the old `map_data` tree
- RMDF writer cleanup details
- Restriction implementation beyond preserving the current explicit stub behavior

## Changes required

### 1. Move all raw OSM imports to `osm_data`

**Current patterns to remove**:

- `crate::map_data::osm::{...}`
- fully-qualified `crate::map_data::osm::OsmRelationMemberType::*`

**Replace with**:

- `crate::osm_data::types::{...}`
- or equivalent `crate::osm_data::*` re-exports if the module exposes them

**Files called out by current code**:

- `crates/ridi-router-tiles/src/osm_data/in_memory_pbf.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/intermediate.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`

## 2. Move graph-building code to `crate::generation`

**Current patterns to remove**:

- `crate::map_data::generation_graph::GenerationGraph`
- `crate::map_data::GenerationGraph`
- `MapDataError`

**Replace with**:

- `crate::generation::GenerationGraph`
- `crate::generation::GenerationError`

**Affected logic**:

- tile graph construction in `pbf_streamer.rs`
- multi-PBF graph construction in `rmdf/generator/mod.rs`
- any helper signatures returning old tiles-private `map_data` types

## 3. Preserve the actual active graph contract

When migrating graph construction, keep the real build-time shape intact:

- insert nodes as `GenerationPoint`
- insert ways as `GenerationLine`
- keep tags in `GenerationTags`
- do **not** reintroduce `point.lines`
- do **not** reintroduce `point.rules`
- do **not** add fake ref wrappers for convenience

## 4. Keep relation handling explicit and unfinished

**Current reality**:

- restriction relations are collected
- they are handed to `insert_relation(...)`
- they are not materialized into RMDF rules yet

**Phase requirement**:

Preserve that reality cleanly during the migration.

That means:

- `insert_relation(...)` may remain a placeholder if it still belongs in the generation flow
- comments should say this is deferred to the separate restriction/rules todo
- do not keep fake generation-side rule types alive to make the API look complete

## 5. Review `GenerationError` call sites while migrating

If a migrated function still returns `Result<_, GenerationError>`, verify that the error surface is still real.

Examples to check:

- `insert_way(...)`
- `insert_relation(...)`
- graph-building helpers in `pbf_streamer.rs`
- graph-building helpers in `rmdf/generator/mod.rs`

If a path can no longer fail in a meaningful way, simplify the API instead of carrying a stale error variant.

## 6. Keep both generation entry points aligned

Both of these paths must end up on the same renamed internal model:

- single-PBF path in `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
- multi-PBF final write path in `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`

Do not fully migrate one path while leaving the other on the old tiles-private model names.

## Acceptance checks

### Code checks

- `cargo check -p ridi-router-tiles`
- `cargo test -p ridi-router-tiles`

### Search checks

Run these and confirm the producer path is clean:

```bash
rg "crate::map_data::osm|crate::map_data::generation_graph|MapDataError" crates/ridi-router-tiles/src/osm_data crates/ridi-router-tiles/src/rmdf/generator
```

Expected result after this phase:

- no active generator code still imports raw OSM or generation graph types from `crate::map_data`

## Dependencies

- Depends on: Phase 1
- Blocks: Phase 3 and Phase 4

## Risks and notes

- `rmdf/generator/mod.rs` and `pbf_streamer.rs` duplicate some logic; migrate both deliberately.
- Multi-PBF code touches more surfaces than the single-PBF path. Do not validate only one path.
- This phase is import-heavy, so use targeted search checks before moving on.