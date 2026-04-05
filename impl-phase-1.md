# Implementation phase 1 — generation-side restriction materialization

## Goal
Retain supported OSM turn restrictions inside the tiles generation model without reintroducing runtime-shaped point rules.

## Why this is phase 1
Everything downstream depends on having concrete, tile-local restriction data on `GenerationGraph`.

## Scope
Implement only the first-pass relation shapes from `todo-plan-turn-restrictions-pipeline.md`:
- `type=restriction`
- one or more `from` ways
- exactly one `via` node
- one or more `to` ways
- supported `restriction=*` values mapping to:
  - `OnlyAllowed`
  - `NotAllowed`

Explicitly skip, but also **count and `warn!`** on:
- `via` way restrictions
- conditional restrictions
- `restriction:*`
- `except=*`
- malformed or partially resolvable relations

## Main changes

### 1. Add a generation restriction module
Create `crates/ridi-router-tiles/src/generation/restriction.rs` with first-pass types close to the todo:
- `GenerationRestrictionRuleType`
- `GenerationRestrictionRule`

Likely fields:
- `relation_id: u64`
- `via_node_id: u64`
- `rule_type`
- `from_line_indices: Vec<u32>`
- `to_line_indices: Vec<u32>`

### 2. Extend `GenerationGraph`
Update `crates/ridi-router-tiles/src/generation/graph.rs` to add:
- `way_line_indices: HashMap<u64, Vec<usize>>`
- `restrictions_by_via: HashMap<u64, Vec<GenerationRestrictionRule>>`

Do **not** change `GenerationPoint`.

### 3. Record way -> generated line indices during `insert_way(...)`
While generating lines for an `OsmWay`, retain the emitted line indices under the source OSM way ID.

This must use the final generated line indices, not way-member order alone.

### 4. Implement `insert_relation(...)`
Use `crates/ridi-router-routing/src/test_utils.rs` as the first-pass semantics reference, but tighten it for tiles:
- parse relation members
- require exactly one `via` node
- map `restriction=*` to generation rule type
- resolve each `from`/`to` way to generated line indices
- keep only via-adjacent generated lines for the `via` node
- skip the relation if either side resolves to zero local via-adjacent lines
- store the rule under `restrictions_by_via[via_node_id]`
- emit `warn!` with relation ID and skip reason for every unsupported/skipped relation
- maintain per-tile skip counters so generation reports how many relations were rejected by category

### 5. Export new module types
Update `crates/ridi-router-tiles/src/generation/mod.rs` to expose the new restriction types needed by the writer.

## Suggested implementation notes
- Add small helpers inside `graph.rs` or `restriction.rs` for:
  - supported restriction type mapping
  - relation member extraction
  - filtering generated lines that touch the `via` node
- Normalize all stored line refs to tile-local generated line indices.
- Unsupported relations should be skipped deliberately, not partially materialized.
- Skips must be visible: `warn!` each skipped relation with a clear reason, and keep counters for summary reporting.

## Tests for this phase
Add generation-focused unit tests that prove:
- supported `no_*` and `only_*` relations materialize rules
- only via-adjacent line indices are retained
- multi-`to` relations flatten correctly
- malformed or unsupported relations are skipped cleanly
- skipped relations increment the expected counters and produce `warn!` output
- no changes are required to `GenerationPoint`

## Done when
- `insert_relation(...)` is no longer a stub
- `GenerationGraph` retains writer-ready tile-local restriction rules
- relation semantics match the current runtime test mapping for supported via-node cases
- unsupported relation shapes are explicitly ignored, counted, and logged with `warn!`

## Likely files
- `crates/ridi-router-tiles/src/generation/restriction.rs` (new)
- `crates/ridi-router-tiles/src/generation/graph.rs`
- `crates/ridi-router-tiles/src/generation/mod.rs`
- tests near `graph.rs` or new restriction module tests
