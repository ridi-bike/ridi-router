# Implementation phase 4 — end-to-end validation and routing behavior tests

## Goal
Prove that generated tiles with serialized restrictions actually change walker behavior.

## Why this is phase 4
Phases 1-3 build the pipeline. This phase verifies the whole path from relation ingestion to routing decisions.

## Scope
Add at least one generated-tile end-to-end test that:
1. builds tiles from OSM-like input with a supported restriction relation
2. loads the generated tile(s) through runtime tile IO
3. verifies the via point has hydrated rules
4. verifies walker behavior changes because of those rules

## Main changes

### 1. Build a focused generated-tile fixture
Use the smallest possible topology that exercises one restriction clearly, for example:
- a `no_left_turn` junction
- or an `only_right_turn` junction

This test should go through the real generation writer + runtime reader path, not only in-memory test graph helpers.

### 2. Assert data-level correctness
Before checking walker behavior, assert the intermediate runtime state:
- via point exists
- `MapDataPoint.rules` is non-empty
- hydrated rule types and line refs match expectations

### 3. Assert behavior-level correctness
Use the existing walker/router APIs to prove:
- the forbidden move is rejected for `NotAllowed`
- only permitted exits remain for `OnlyAllowed`

### 4. Add regression coverage for tile-local behavior
If cheap, add one more test for a border/buffer case where the rule is materialized independently per tile copy of the via node.

This is optional for the first pass if fixture setup becomes heavy.

## Suggested implementation notes
- Keep this phase focused on proof, not feature expansion.
- Reuse the restriction semantics already covered in walker unit tests where possible.
- Prefer one strong end-to-end test plus one narrower serialization/regression test over many overlapping cases.
- Add one pipeline-level assertion for the unsupported path too: skipped relations should produce `warn!` output and increment the skip counters defined in phase 1.

## Tests for this phase
Minimum:
- generated-tile `no_*` or `only_*` routing test

Nice to have:
- explicit unsupported-relation skip test at pipeline level, including `warn!` output and counter increments
- tile-border duplication test for a via node near a tile edge

## Done when
- there is at least one generated-tile test proving walker behavior changes because of serialized restrictions
- runtime hydration is exercised through real tile files, not only test-only in-memory graph builders
- the implementation satisfies the todo done criteria end to end

## Likely files
- generated-tile integration or high-level tests under the existing routing/tiles test structure
- possibly `crates/ridi-router-routing/src/router/walker.rs` tests if that is where end-to-end routing assertions already live
- test support helpers if fixture generation needs them
