# Phase 3: Prove the policy with synthetic eviction tests

## Objective
Lock down the new loaded tiles registry behavior with deterministic, small, synthetic tests that do not depend on Montenegro data.

## Why this phase exists
The TODO requires test evidence for true LRU behavior, not just a code change. The tests must prove eviction order, hit-based promotion, hot-tile survival, and preservation of existing traversal behavior.

## Primary files
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- optionally `crates/ridi-router-test-support/src/lib.rs` if shared multi-tile fixture helpers make the tests much cleaner

## Changes in scope
1. Add or extend synthetic fixtures so tests can load a tiny set of tiles with a tiny limit such as `3`.
2. Add focused tests for the required scenarios:
   - overflow unloads the oldest tile
   - touching an already loaded tile makes it newest
   - a hot tile survives while colder tiles are unloaded
   - existing non-eviction behavior still works (`missing-neighbor handling`, `cross-tile traversal`)
3. Use normal tile-backed access paths where possible so the tests prove real runtime behavior, not only helper internals.
4. Keep Montenegro-based tests as optional regression coverage, but not as the primary evidence for this work.

## Recommended test shape
- Build a tiny synthetic multi-tile setup.
- Use the test-only small limit from Phase 1.
- Assert on loaded count and specific tile residency after each step.
- Prefer simple tile labels in comments/assert messages (A/B/C/D) even if the real IDs are numeric `TileId`s.

## Exit criteria
- Tests prove deterministic LRU behavior.
- Tests are synthetic and stable.
- Existing traversal behavior remains covered.
- The target command passes locally.

## Verification
```bash
cargo test -p ridi-router-routing tile_manager -- --nocapture
```

Also run any nearby routing or graph tests that exercise tile-backed access if they are affected by tile residency changes.
