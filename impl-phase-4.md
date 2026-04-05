# Phase 4: Harden the real-capacity overflow path and close verification gaps

## Objective
Make the loaded tiles registry safe at the exact production boundary where `MAX_LOADED_TILES` loaded tiles transition to `MAX_LOADED_TILES + 1`, and close the remaining verification/doc gaps from review.

## Why this phase exists
Phases 1-3 established the fixed-order LRU registry and synthetic eviction coverage, but the branch still had one important hole:

- the fixed array order structure had room for exactly `MAX_LOADED_TILES`
- the miss path tried to append tile `MAX_LOADED_TILES + 1` before unloading
- that works with tiny test limits backed by a larger array, but it is unsafe at the real production limit

This phase hardens that boundary without changing the approved production cache size or moving away from the fixed-capacity order structure.

## Primary file
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

## Changes in scope
1. Update the miss-path append/unload flow so the fixed order structure never writes past the end of `loaded_tile_order` at the real production limit.
2. Keep deterministic oldest->newest LRU behavior:
   - a newly loaded tile is still treated as newest immediately
   - the oldest resident tile is still the one that gets unloaded
3. Add a regression test that exercises the exact transition from:
   - `MAX_LOADED_TILES`
   - to `MAX_LOADED_TILES + 1`
4. Run at least one nearby routing/graph test target in addition to the tile-manager target.

## Recommended implementation shape
Use the fixed array as the active registry order, not as temporary overflow storage.

That means:
- if the registry is below the logical limit, append normally
- if the registry is already at the logical limit and a new tile is loaded:
  - remember the current oldest tile
  - rotate the active order left
  - place the new tile in the newest slot
  - unload the remembered oldest tile from `loaded_tiles`

This preserves deterministic LRU semantics while keeping the fixed array valid at the production boundary.

## Exit criteria
- Loading tile `MAX_LOADED_TILES + 1` does not overflow the order array.
- The oldest loaded tile is deterministically unloaded.
- The exact production-boundary regression test passes.
- At least one nearby graph/routing test target also passes.

## Verification
```bash
cargo test -p ridi-router-routing tile_manager -- --nocapture
cargo test -p ridi-router-routing map_data::graph::tests::test_get_point_from_tiles -- --nocapture
```
