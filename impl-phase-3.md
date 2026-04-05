# Phase 3: Update the RMDF writer to consume the generation model directly

Derived from `todo-plan-convert-map-data-line-helpers-tiles.md`.

## Goal

Make `crates/ridi-router-tiles/src/rmdf/generator/writer.rs` depend only on the renamed generation model.

After this phase, the writer should serialize RMDF from:

- `GenerationGraph`
- `GenerationPoint`
- `GenerationLine`
- `LineDirection`
- `GenerationTags`
- `TagSet` / `TagSetId` / `TagValueId`

It should no longer reach back into old tiles-side `map_data` names or fake point/rule fields.

## Scope

### In scope

- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
- writer-local helper comments and type names
- any tests directly coupled to writer type names

### Not in scope

- implementing turn restrictions
- implementing RMDF rule serialization
- routing-side RMDF rule loading

## Changes required

### 1. Switch writer imports to `crate::generation`

**Current patterns to remove**:

- `use crate::map_data::GenerationGraph;`
- `crate::map_data::point::MapDataPoint`
- `crate::map_data::line::LineDirection`

**Replace with**:

- `use crate::generation::{...};`
- writer-local references to `GenerationPoint`
- writer-local references to `LineDirection` from `generation/generation_line.rs`

## 2. Remove writer dependence on fake point fields

**Current issue**:

The writer still contains comments and code paths that acknowledge `GenerationGraph` does not populate `point.lines`, but it also still reads `point.rules.len()` when serializing points.

**Required fix**:

- keep line reconstruction explicit via a local `point_lines_map`
- stop reading `point.rules`
- stop assuming generation-side point rule vectors are real

**Point serialization contract after this phase**:

- `lines_offset` may remain computed the same way it is today if that logic is unchanged in this refactor
- `lines_count` comes from the explicit point-to-lines map
- `rules_offset = 0`
- `rules_count = 0`

## 3. Make unfinished rule handling explicit

**Required shape**:

- `serialize_rules(...)` stays empty
- comments explain that rules serialization belongs to the separate restriction/rules todo
- do not mention the deleted tiles-side `MapDataRule` model as if it were still authoritative

This is better than preserving a fake model and serializing counts from data that the generation pipeline does not materialize.

## 4. Rename tag serialization to the new types

Update the writer to serialize tag data from the renamed generation types directly.

That includes:

- `GenerationTags`
- `TagSet`
- `TagSetId`
- `TagValueId`

**Places to update**:

- tag value iteration
- tag set record serialization
- line record `tag_set_index` access

## 5. Use the moved `LineDirection`

Update line serialization so the direction mapping reads from the new `LineDirection` location instead of `crate::map_data::line::LineDirection`.

The serialized values should stay the same:

- `BothWays => 0`
- `OneWay => 1`
- `Roundabout => 2`

## 6. Clean comments to describe the real architecture

Fix comments such as:

- references to `point.lines` as if it were a real field that is merely unpopulated
- comments that imply a runtime graph contract inside the tiles crate
- comments that point to `MapDataRule` as the basis for future serialization

The writer comments should clearly describe the actual pipeline:

`OSM input -> generation model -> RMDF serialization`

## Acceptance checks

### Code checks

- `cargo check -p ridi-router-tiles`
- `cargo test -p ridi-router-tiles`

### Search checks

Run:

```bash
rg "crate::map_data::|point\.rules|point\.lines|MapDataRule" crates/ridi-router-tiles/src/rmdf/generator/writer.rs
```

Expected result after this phase:

- no old tiles-side `map_data` imports remain in the writer
- no writer serialization code reads fake generation-side point rule/line fields

## Dependencies

- Depends on: Phase 1 and Phase 2
- Blocks: Phase 4

## Risks and notes

- The writer has ordering-sensitive point and line-ref serialization. Keep naming refactors separate from ordering logic changes.
- Keep this phase focused on model migration, not on redesigning RMDF layout behavior.
- If tests expose pre-existing writer issues, fix only what is needed for the renamed generation contract.