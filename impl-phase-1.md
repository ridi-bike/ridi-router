# Phase 1: Establish the new `generation` and `osm_data` type layout

Derived from `todo-plan-convert-map-data-line-helpers-tiles.md`.

## Goal

Create the new tiles-internal domain layout so the crate stops introducing new code under the stale `map_data` model.

This phase is about **moving and renaming the core types first**:

- generation-time transformed types live under `crates/ridi-router-tiles/src/generation`
- raw OSM input types live under `crates/ridi-router-tiles/src/osm_data`
- new code stops depending on fake ref-based tiles-side graph types

## Scope

### In scope

- Create `crates/ridi-router-tiles/src/generation/mod.rs`
- Create `crates/ridi-router-tiles/src/generation/graph.rs`
- Create `crates/ridi-router-tiles/src/generation/point.rs`
- Create `crates/ridi-router-tiles/src/generation/generation_line.rs`
- Create `crates/ridi-router-tiles/src/generation/tags.rs`
- Create `crates/ridi-router-tiles/src/osm_data/types.rs`
- Update `crates/ridi-router-tiles/src/osm_data/mod.rs`
- Update `crates/ridi-router-tiles/src/lib.rs`
- Rename `MapDataError` to `GenerationError`
- Rename generation tag types to generation-oriented names

### Not in scope

- Deleting the old `map_data` files yet
- Full producer/writer migration
- Restriction implementation
- RMDF rule serialization

## Changes required

### 1. Create the new generation module tree

**Files**:

- `crates/ridi-router-tiles/src/generation/mod.rs`
- `crates/ridi-router-tiles/src/generation/graph.rs`
- `crates/ridi-router-tiles/src/generation/point.rs`
- `crates/ridi-router-tiles/src/generation/generation_line.rs`
- `crates/ridi-router-tiles/src/generation/tags.rs`

**Required shape**:

- `GenerationGraph` lives in `generation/graph.rs`
- `GenerationPoint` lives in `generation/point.rs`
- `GenerationLine` and `LineDirection` live together in `generation/generation_line.rs`
- `GenerationTags`, `TagSetId`, `TagValueId`, and `TagSet` live in `generation/tags.rs`
- `generation/mod.rs` re-exports the active generation model cleanly

**Important rule**:

Do **not** recreate the old tiles-side runtime graph shape inside the new module. The new files should not introduce:

- `MapDataElementRef<T>`
- `MapDataPointRef`
- `MapDataLineRef`
- dereference-based helper methods
- fake `.get()` surfaces

## 2. Rename and prune the point model

**Source today**:

- `crates/ridi-router-tiles/src/map_data/point.rs`
- `crates/ridi-router-tiles/src/map_data/generation_graph.rs`

**Target**:

`GenerationPoint` should keep only:

- `id`
- `lat`
- `lon`
- `residential_in_proximity`
- `nogo_area`

**Delete from the new type**:

- `lines`
- `rules`
- helper/debug behavior that assumes those fields are real

**Rationale**:

The RMDF writer already reconstructs point-to-line relationships from generated lines. Restriction rules are not materialized in the active generation pipeline.

## 3. Move and rename the line model

**Source today**:

- `crates/ridi-router-tiles/src/map_data/line.rs`
- `crates/ridi-router-tiles/src/map_data/generation_graph.rs`

**Target**:

- Keep `GenerationLine { from_node_id, to_node_id, direction, tags }`
- Keep `LineDirection`
- Remove `MapDataLine` from the active path entirely

**Important rule**:

Do not carry over helper methods that depend on point ref dereferencing, such as length helpers implemented through `.get()`.

## 4. Move and rename the tag-table model

**Source today**:

- `crates/ridi-router-tiles/src/map_data/graph.rs`

**Rename exactly**:

- `ElementTags` -> `GenerationTags`
- `ElementTagSetRef` -> `TagSetId`
- `ElementTagValueRef` -> `TagValueId`
- `ElementTagSet` -> `TagSet`

**Design constraint**:

The new tag types should only expose behavior that is real in the generation pipeline. Do not preserve placeholder getters that fabricate missing data.

## 5. Move raw OSM types into `osm_data`

**Files**:

- `crates/ridi-router-tiles/src/osm_data/types.rs`
- `crates/ridi-router-tiles/src/osm_data/mod.rs`

**Move**:

- `OsmNode`
- `OsmWay`
- `OsmRelation`
- `OsmRelationMember`
- `OsmRelationMemberType`
- `OsmRelationMemberRole`

**Rationale**:

These are input-domain types. They should not remain under a tiles-private `map_data` module.

## 6. Introduce `GenerationError`

**Source today**:

- `crates/ridi-router-tiles/src/map_data/mod.rs`

**Target**:

Move the error enum into the new generation module and rename it to `GenerationError`.

**Review requirement**:

While moving it, mark every variant as one of:

- still real in the current generation pipeline
- restriction-future-only and removable
- no longer needed because the API can be simplified

Do not preserve stale variants just because they existed before.

## 7. Update top-level module wiring

**Files**:

- `crates/ridi-router-tiles/src/lib.rs`
- `crates/ridi-router-tiles/src/osm_data/mod.rs`

**Required result**:

- `lib.rs` declares `mod generation;`
- `osm_data/mod.rs` exposes the new OSM type module
- the active tiles-internal domain names are now visible from their correct module roots

## Acceptance checks

### Code checks

- `cargo check -p ridi-router-tiles`

### Structural checks

- The new files exist under `crates/ridi-router-tiles/src/generation`
- The new files exist under `crates/ridi-router-tiles/src/osm_data`
- `GenerationPoint` has no `lines` field
- `GenerationPoint` has no `rules` field
- No new generation module file depends on `MapDataElementRef<T>` or tiles-side ref aliases

## Dependencies

- Depends on: none
- Blocks: Phase 2 and Phase 3

## Risks and notes

- This phase will create broad import churn; keep the renames mechanical and explicit.
- If short-lived compatibility glue is needed during the move, remove it before Phase 4 completes.
- Do not let temporary wrappers become the final architecture.