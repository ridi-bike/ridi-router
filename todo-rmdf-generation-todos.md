# TODO: RMDF generation follow-ups

## Goal

Address the remaining generation-side TODOs so the RMDF writing path is less placeholder-driven and better aligned with the runtime data model.

This is separate from the routing refactor. It focuses on generation-time completeness.

## Files

- `crates/ridi-router-routing/src/map_data/generation_graph.rs`
- `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`

## Current TODOs

### In `generation_graph.rs`

- proper turn restriction storage/processing is not implemented
- point hashing is still a placeholder and may or may not be needed

### In `writer.rs`

- `PointRecord.lines_offset` is still written as `0`
- `PointRecord.rules_offset` is still written as `0`
- rules serialization is still stubbed out

## Why this matters

Right now the generation side still has a few structural gaps:

- turn restrictions are not carried through cleanly
- rule serialization is incomplete
- point record offsets are partly placeholder values
- it is not yet obvious which TODOs are truly required and which should be deleted as obsolete

This makes it harder to finish tile-backed rule loading on the runtime side.

## Split the work into clear decisions

### A. Turn restrictions and rule model

#### Problem

`insert_relation(...)` currently skips relation processing.

If turn restrictions are meant to influence runtime routing, generation needs a concrete model for:

- how restrictions are represented in memory
- how they attach to points/lines
- how they are written into RMDF
- how runtime code reads them back

#### Plan

- decide the canonical in-memory shape for restriction/rule data
- store the needed relation-derived rule data in `GenerationGraph`
- connect the writer to that shape
- connect runtime tile loading to the same serialized representation

#### Acceptance

- relation-derived routing restrictions survive generation and load back into runtime structures
- test fixtures cover at least one restriction case

---

### B. Point record offsets must become real offsets

#### Problem

`writer.rs` still writes placeholder values for:

- `lines_offset`
- `rules_offset`

That should be replaced with real offsets based on the serialized side tables.

#### Plan

- make point serialization and side-table serialization share one consistent ordering
- calculate line-ref offsets from the flattened line-ref table
- calculate rules offsets from the flattened rules table
- verify offsets and counts round-trip correctly

#### Acceptance

- no placeholder `0` offsets remain where real offsets are required
- point records point at valid serialized sections
- reader/runtime code can consume the produced layout reliably

---

### C. Rules serialization must stop being a stub

#### Problem

`serialize_rules(...)` currently returns an empty byte vector.

That blocks full tile-backed rule loading.

#### Plan

- define the serialized RMDF representation for `MapDataRule`
- serialize `from_lines`, `to_lines`, and `rule_type`
- write reader-side support if not already present
- validate with round-trip tests

#### Acceptance

- rules are serialized into RMDF output
- rules can be read back and attached to points at runtime
- runtime no longer needs `rules: Vec::new()` as a placeholder

---

### D. Decide whether point hashing stays or is deleted

#### Problem

`generate_point_hashes()` is still a placeholder, but the comment already suggests it may be unnecessary if grid-cell spatial indexing is the real plan.

#### Plan

Make an explicit decision:

- if grid-cell indexing is sufficient, delete the dead placeholder API
- if extra hashing is still needed, implement it with a concrete consumer and tests

#### Acceptance

- no ambiguous placeholder remains
- the code clearly reflects the chosen spatial-index approach

## Suggested implementation order

1. decide rule/restriction data shape
2. implement rules serialization
3. calculate real record offsets
4. implement runtime rule loading against the written format
5. remove or implement point hashing

## Acceptance criteria

- relation/rule data has a defined path from OSM input to RMDF output
- writer no longer emits placeholder offsets for data that exists
- rules serialization is real, not stubbed
- runtime tile-backed rule loading has a concrete serialized source
- obsolete TODOs are removed rather than left ambiguous

## Suggested checks

- `cargo test -p ridi-router-tiles`
- `cargo test -p ridi-router-routing`
- add fixture-based round-trip tests: OSM -> RMDF -> runtime load
- spot-check generated RMDF records in a small synthetic fixture

## Non-goals

- changing the routing executor/context architecture
- broad routing heuristic changes
- unrelated RMDF format redesign beyond what is needed to serialize current rule data
