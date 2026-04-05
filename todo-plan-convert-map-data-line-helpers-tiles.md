# Plan: refactor `ridi-router-tiles` to an explicit generation model

This file is the source of truth for this todo.

It supersedes the narrow helper-cleanup framing from:
- `todo-review-convert-map-data-line-helpers-tiles.md`

It also absorbs the same root problem described by these related tiles-crate reviews:
- `todo-review-convert-map-data-point-helpers-tiles.md`
- `todo-review-remove-map-data-element-ref-get-tiles.md`

It does **not** attempt to complete the separate turn-restriction / RMDF-rules pipeline todos:
- `todo-review-rules-serialization.md`
- `todo-review-turn-restrictions-rule-model.md`

## Reviewed decisions

- Target the **ideal end state**, not a narrow helper patch.
- In `crates/ridi-router-tiles`, stop modeling generation code as a sibling of routing's runtime `map_data` layer.
- Rename the internal module from `map_data` to **`generation`**.
- Move raw OSM types into **`osm_data`**.
- Rename `MapDataPoint` to **`GenerationPoint`**.
- Keep `LineDirection`, but move it to **`generation_line.rs`**.
- Replace generation-side tag types with generation-oriented names:
  - `ElementTags` -> **`GenerationTags`**
  - `ElementTagSetRef` -> **`TagSetId`**
  - `ElementTagValueRef` -> **`TagValueId`**
  - `ElementTagSet` -> **`TagSet`**
- Remove tiles-side legacy shared-lineage types and surfaces that no longer serve the active generator:
  - `map_data/graph.rs`
  - `MapDataElementRef<T>`
  - `MapDataPointRef`
  - `MapDataLineRef`
  - `MapDataLine`
  - `MapDataRule`
  - `point.lines`
  - `point.rules`
- Rename `MapDataError` to **`GenerationError`**.
- RMDF writer should consume the renamed generation model directly.
- Keep restriction handling as a **separate later todo**.
  - Do not add missing restriction functionality in this refactor.
  - Do not redesign the restriction pipeline here.
- Do **not** force a shared crate with routing in this todo.
- Use **intentional cleanup**, not a compatibility-preserving halfway state.
- After this refactor, reassess what overlap with `ridi-router-routing` is still worth sharing.

## Current code reality

Confirmed in the codebase:

### 1. The active tile-generation path is already generation-shaped
The current production path in `ridi-router-tiles` does **not** rely on `MapDataLine` or dereferenceable refs.

Active flow today:
- `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
  - owns `GenerationGraph`
  - stores `GenerationLine { from_node_id, to_node_id, ... }`
  - stores points directly in a vector
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
  - serializes lines by building a `node_map`
  - resolves endpoints explicitly from `from_node_id` / `to_node_id`

So the live generation architecture already uses explicit build-time data.

### 2. The old tiles `map_data` layer is mostly stale shared lineage
The remaining `map_data` files in `tiles` still mirror the routing crate's runtime model shape, but they no longer behave like a real runtime graph layer.

Examples:
- `crates/ridi-router-tiles/src/map_data/graph.rs`
  - contains tag-table code that is still used
  - also contains `MapDataElementRef<T>::get()` which is a panic stub
- `crates/ridi-router-tiles/src/map_data/line.rs`
  - still has helper methods built on `.get()` dereference
- `crates/ridi-router-tiles/src/map_data/point.rs`
  - still carries `lines` and `rules`
  - still has helper methods built on `.get()` dereference
- `crates/ridi-router-tiles/src/map_data/rule.rs`
  - exists even though generation-side rules are not materialized

This is the wrong model for the current crate purpose.

### 3. The old helper todos are symptoms, not the root problem
The earlier helper todos are real, but they are downstream symptoms of the larger issue:
- the tiles crate still contains a stale runtime-shaped internal model
- that model drifted from the actual generation flow
- some of its APIs are invalid in this crate because refs are not dereferenceable

### 4. Restriction relations are collected, but not processed
The code currently **does** collect restriction relations upstream:
- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`

Those relations are passed into:
- `GenerationGraph::insert_relation(...)`

But `insert_relation(...)` is still a stub:
- `crates/ridi-router-tiles/src/map_data/generation_graph.rs`

So restrictions are currently:
- collected
- handed off
- then dropped

### 5. RMDF rules are still unfinished and out of scope here
Current writer/runtime reality:
- `serialize_rules(...)` returns an empty vector
- point records write placeholder rule offsets
- runtime tile loading still hydrates points with empty rules

This refactor should **not** try to solve that unfinished restriction pipeline.

## Why this refactor is worth doing

This todo is worth doing because it fixes the real architectural mismatch:

- the tiles crate is a **generation** crate
- the routing crate is a **runtime graph access** crate
- keeping a fake shared internal shape in `tiles` is what created the dead helpers, fake refs, panic stubs, and misleading module names

The right fix is not “clean up a few helpers.”
The right fix is:
- rename the tiles-side model to match its true purpose
- remove stale runtime-shaped leftovers
- leave restriction support as its own later pipeline task

## Implementation scenarios considered

### Scenario 1: narrow helper cleanup
Examples:
- remove `line_id()`
- change geometry helper signatures
- delete `MapDataElementRef<T>::get()`

Rejected because:
- it fixes symptoms, not structure
- it preserves the misleading `map_data` / `graph.rs` lineage in `tiles`
- it still leaves dead or legacy types around

### Scenario 2: split `graph.rs` into `tags.rs` and `refs.rs`
This would be a safer transitional cleanup.

Rejected for this todo because:
- it still preserves ref-based lineage that the ideal end state should remove
- it is a good migration tactic, but not the desired destination

### Scenario 3: make `tiles` explicitly generation-shaped
Chosen approach.

This matches the actual active code path and removes the stale shared model instead of patching around it.

## Chosen approach

Refactor `crates/ridi-router-tiles` so its internal model is explicitly generation-specific.

### High-level target shape

Replace the old tiles-private `map_data` subtree with generation-oriented modules, for example:

- `crates/ridi-router-tiles/src/generation/mod.rs`
- `crates/ridi-router-tiles/src/generation/graph.rs`
- `crates/ridi-router-tiles/src/generation/point.rs`
- `crates/ridi-router-tiles/src/generation/generation_line.rs`
- `crates/ridi-router-tiles/src/generation/tags.rs`

And move raw OSM structs under `osm_data`, for example:
- `crates/ridi-router-tiles/src/osm_data/types.rs`

The exact file split can vary a little, but the architectural intent should stay fixed:
- raw OSM input types live in `osm_data`
- generation-time transformed types live in `generation`
- there is no tiles-side `graph.rs` pretending to be a runtime graph layer

## Proposed renamed model

### Generation domain

#### `GenerationPoint`
Replacement for tiles-side `MapDataPoint`.

Keep only fields needed by generation + RMDF writing:
- `id`
- `lat`
- `lon`
- `residential_in_proximity`
- `nogo_area`

Remove:
- `lines`
- `rules`

Reason:
- writer already reconstructs point->line relationships from generated lines
- restriction rules are not implemented in the real generation pipeline yet

#### `GenerationLine`
Keep the active explicit build-time shape:
- `from_node_id`
- `to_node_id`
- `direction`
- `tags`

Move `LineDirection` here too.

#### `GenerationGraph`
Keep the active generation graph as the center of the tiles build path.

It should own:
- generated points
- generated lines
- generation tag tables

It should not pretend to expose runtime-style dereference helpers or runtime graph semantics.

#### `GenerationTags`
Replacement for `ElementTags`.

Purpose:
- own the build-time tag value table and tag-set table used during generation and RMDF serialization

#### `TagSetId`, `TagValueId`, `TagSet`
Generation-oriented replacements for the old `ElementTag*` names.

These are still useful in generation because the writer serializes tag value tables and tag set tables.

### OSM domain

Move raw OSM structs from `map_data/osm.rs` into `osm_data`, such as:
- `OsmNode`
- `OsmWay`
- `OsmRelation`
- related member enums / structs

Reason:
- they are input-domain types, not generation-domain transformed types
- this better reflects the real pipeline: OSM input -> generation model -> RMDF output

## What should be deleted

Delete the tiles-side surfaces that only exist because of old shared lineage:

- `crates/ridi-router-tiles/src/map_data/graph.rs`
- `crates/ridi-router-tiles/src/map_data/line.rs`
- `crates/ridi-router-tiles/src/map_data/point.rs`
- `crates/ridi-router-tiles/src/map_data/rule.rs`
- `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
- `crates/ridi-router-tiles/src/map_data/osm.rs`
- `crates/ridi-router-tiles/src/map_data/mod.rs`

And remove these type families from `tiles` entirely:
- `MapDataElementRef<T>`
- `MapDataPointRef`
- `MapDataLineRef`
- `MapDataLine`
- `MapDataRule`
- helper methods that depend on fake dereference behavior
- fake tag getter APIs that return placeholder data

## RMDF writer contract after refactor

The writer should consume the generation model directly.

### What changes
- stop referencing old `map_data::*` paths
- use `generation::*` types directly
- use `GenerationPoint` and `GenerationLine` paths in helper types and local maps
- use `LineDirection` from `generation/generation_line.rs`
- use `GenerationTags` / `TagSet` / `TagSetId` directly

### What should stay explicit
This refactor should keep rule handling explicit and unfinished rather than fake:
- points should write `rules_offset = 0`
- points should write `rules_count = 0`
- `serialize_rules(...)` should remain empty
- comments should point to the separate restriction/rules todo rather than implying the old fake point-rule model is real

This is intentionally better than preserving `point.rules.len()` from a generation model that does not actually materialize restrictions.

## Generation error cleanup

Rename `MapDataError` to `GenerationError`.

During the refactor, review whether the current enum still matches the real generation responsibilities.

### Important constraint
Do not keep stale error variants just because they belonged to the old shared lineage.

That means:
- restriction-specific variants that only make sense once real restriction materialization exists should be reconsidered
- if some variants are currently unused and belong to future restriction work, they should be removed and reintroduced later when that work is real
- if some current `Result` return paths cannot actually fail anymore, simplify those APIs instead of carrying a fake error surface forever

## Scope boundaries

### In scope
- rename tiles-private module structure toward `generation`
- move raw OSM types into `osm_data`
- rename point/tag/error types to generation-oriented names
- update generator and writer code to use the new generation model
- remove legacy ref-based and rule-based generation artifacts from `tiles`
- remove dead helper/debug APIs that only existed for the old model
- clean imports, comments, and naming so the tiles crate no longer advertises the wrong architecture

### Out of scope
- implementing turn restriction materialization
- implementing RMDF rule serialization
- implementing RMDF rule loading in routing
- changing routing crate architecture in the same todo
- introducing a new shared crate for generation/runtime map-data code

## Related todos after this refactor

### Likely superseded by this plan
If this plan is implemented fully, these narrower tiles-side todos should be considered resolved by removal/replacement of the old model:
- `todo-review-convert-map-data-point-helpers-tiles.md`
- `todo-review-remove-map-data-element-ref-get-tiles.md`

The original line-helper review that led to this plan is also superseded by this broader refactor.

### Explicitly not solved here
These should remain separate follow-up work:
- `todo-review-rules-serialization.md`
- `todo-review-turn-restrictions-rule-model.md`

## Suggested work steps

1. Create the new tiles-internal `generation` module.
2. Move `GenerationGraph` into `generation/graph.rs`.
3. Extract/move `GenerationLine` and `LineDirection` into `generation/generation_line.rs`.
4. Rename tiles-side `MapDataPoint` to `GenerationPoint` and move it into `generation/point.rs`.
5. Move tag-table code out of old `map_data/graph.rs` into `generation/tags.rs`.
6. Rename tag types:
   - `ElementTags` -> `GenerationTags`
   - `ElementTagSetRef` -> `TagSetId`
   - `ElementTagValueRef` -> `TagValueId`
   - `ElementTagSet` -> `TagSet`
7. Move raw OSM types from `map_data/osm.rs` into `osm_data`.
8. Rename `MapDataError` to `GenerationError` and prune stale variants.
9. Update all internal tiles-crate imports from `crate::map_data::*` to `crate::generation::*` or `crate::osm_data::*`.
10. Update `rmdf/generator/mod.rs` and `rmdf/generator/pbf_streamer.rs` to build the new generation model.
11. Update `rmdf/generator/writer.rs` to consume `GenerationGraph`, `GenerationPoint`, `GenerationLine`, and the renamed tag types directly.
12. Make point rule fields explicit zeros in writer output instead of reading fake generation-side rule vectors.
13. Keep `insert_relation(...)` as an explicit TODO-owned placeholder only if it still belongs in the generation pipeline shape.
14. Remove legacy tiles-only surfaces:
    - old `map_data` module tree
    - fake ref wrappers
    - dead helper methods
    - dead debug implementations tied to dereference
    - fake tag getter APIs
15. Clean comments so they describe the real architecture, not the old shared lineage.
16. Re-run tests and fix any internal compile fallout.
17. Add a short follow-up note in the final change summary: after the generation refactor lands, reassess whether any non-runtime data types still overlap enough with routing to share later.

## Validation

Primary checks:

```bash
cargo test -p ridi-router-tiles
```

Recommended broader regression checks:

```bash
cargo test -p ridi-router-routing
cargo test -p ridi-router-cli
```

Helpful codebase sanity checks after the refactor:

```bash
rg "crate::map_data::|super::graph::|MapDataElementRef|MapDataPointRef|MapDataLineRef|MapDataRule|point\.rules|point\.lines" crates/ridi-router-tiles/src
```

Expected outcome of that search:
- no remaining tiles-side references to the removed legacy generation artifacts
- only routing crate keeps runtime `MapData*` graph types

## Non-goals

- No turn restriction implementation
- No RMDF rules serialization completion
- No routing-side rule loading work
- No shared `map_data` crate introduction
- No attempt to keep tiles-side legacy names alive for compatibility
- No transitional `refs.rs` layer unless needed only as a very short-lived migration step during implementation

## Done criteria

This todo is done when all of the following are true:

- `crates/ridi-router-tiles` no longer has a private `map_data` module that pretends to be a runtime graph model
- tiles generation code uses a clearly named `generation` module
- raw OSM types live under `osm_data`
- `GenerationPoint` replaces tiles-side `MapDataPoint`
- `LineDirection` lives with generation-line code
- generation tag types use generation-oriented naming
- the RMDF writer consumes only the generation model directly
- tiles-side legacy refs and fake dereference surfaces are gone
- tiles-side `MapDataLine` and `MapDataRule` legacy generation artifacts are gone
- tiles-side point `lines` and `rules` fields are gone
- `GenerationError` replaces `MapDataError`
- comments and names describe the real architecture accurately
- `cargo test -p ridi-router-tiles` passes
- recommended routing/CLI regression checks pass
