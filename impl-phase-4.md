# Phase 4: Delete the legacy `map_data` tree and finish validation

Derived from `todo-plan-convert-map-data-line-helpers-tiles.md`.

## Goal

Remove the obsolete tiles-side `map_data` lineage completely and prove that `ridi-router-tiles` now uses the generation-oriented architecture end to end.

This is the cleanup and validation phase.

## Scope

### In scope

- delete old tiles-side `map_data` files
- remove any temporary compatibility glue left from earlier phases
- prune stale `GenerationError` variants and simplify APIs where appropriate
- clean comments and names that still advertise the wrong architecture
- run the validation and regression checks from the source todo

### Not in scope

- turn restriction implementation
- RMDF rules serialization completion
- routing crate architecture changes
- introducing a shared generation/runtime crate

## Changes required

### 1. Delete the obsolete tiles-side module tree

**Delete**:

- `crates/ridi-router-tiles/src/map_data/graph.rs`
- `crates/ridi-router-tiles/src/map_data/line.rs`
- `crates/ridi-router-tiles/src/map_data/point.rs`
- `crates/ridi-router-tiles/src/map_data/rule.rs`
- `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
- `crates/ridi-router-tiles/src/map_data/osm.rs`
- `crates/ridi-router-tiles/src/map_data/mod.rs`

**Also remove**:

- any `mod map_data;` entry in `crates/ridi-router-tiles/src/lib.rs`
- any temporary re-export or shim added during migration

## 2. Remove the legacy type family from tiles entirely

After the file deletion, confirm there are no remaining tiles-side references to:

- `MapDataElementRef<T>`
- `MapDataPointRef`
- `MapDataLineRef`
- `MapDataLine`
- `MapDataRule`
- fake dereference helpers
- fake tag getter APIs

## 3. Prune `GenerationError` to the real surface

Now that all active call sites are migrated, do a final error review.

**Required review questions**:

- Which variants are still used by active generation code?
- Which variants only make sense once real restriction materialization exists?
- Which `Result` return types can be simplified because the operations no longer fail meaningfully?

**Required outcome**:

Do not keep stale error variants as architectural debris.

## 4. Clean architecture comments and naming

Review comments in these areas at minimum:

- `crates/ridi-router-tiles/src/generation/**`
- `crates/ridi-router-tiles/src/osm_data/**`
- `crates/ridi-router-tiles/src/rmdf/generator/**`

Remove wording that still suggests:

- tiles owns a runtime-style graph layer
- point rules are already a real generation-side structure
- dereferenceable ref wrappers are part of the intended design

## 5. Run the validation checks from the source todo

### Primary check

```bash
cargo test -p ridi-router-tiles
```

### Recommended broader regression checks

```bash
cargo test -p ridi-router-routing
cargo test -p ridi-router-cli
```

### Required search sanity check

```bash
rg "crate::map_data::|super::graph::|MapDataElementRef|MapDataPointRef|MapDataLineRef|MapDataRule|point\.rules|point\.lines" crates/ridi-router-tiles/src
```

**Expected result**:

- no remaining tiles-side references to removed legacy generation artifacts
- only the routing crate keeps runtime `MapData*` graph types

## 6. Capture the follow-up note in the final summary

When the implementation is complete, add a short note to the final change summary:

> After the tiles generation refactor landed, reassess whether any non-runtime data types still overlap enough with `ridi-router-routing` to share later.

That note belongs in the final human summary, not as a compatibility layer in code.

## Acceptance checks

This phase is done when all of the following are true:

- `crates/ridi-router-tiles` no longer declares a private `map_data` module
- the active tiles code imports only `generation` and `osm_data` for this domain split
- legacy tiles-side refs and fake dereference surfaces are gone
- tiles-side `MapDataLine` and `MapDataRule` are gone
- tiles-side point `lines` and `rules` fields are gone
- `GenerationError` is the only remaining error type for this internal model
- `cargo test -p ridi-router-tiles` passes
- recommended routing and CLI regression checks pass

## Dependencies

- Depends on: Phase 1, Phase 2, and Phase 3
- Blocks: none

## Risks and notes

- This phase will expose any missed imports immediately; run the search sanity check before and after deletion.
- Do not stop at “it compiles” if legacy files still exist. The point of this phase is actual removal.
- If a temporary migration shim survives into this phase, treat that as unfinished work, not as an acceptable end state.