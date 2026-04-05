# Phase 1: Build the loaded tiles registry foundation

## Objective
Add the deterministic order-tracking state and helper surface that the real LRU policy will use.

## Why this phase exists
The current `TileManager` only has `loaded_tiles: HashMap<TileId, MappedTile>`. Before changing behavior, the registry needs an explicit oldest->newest order structure plus a safe place for its invariants and test hooks.

## Primary file
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

## Changes in scope
1. Extend `TileManager` state with:
   - `loaded_tiles: HashMap<TileId, MappedTile>` (kept as-is)
   - `loaded_tile_order: [TileId; Self::MAX_LOADED_TILES]`
   - `loaded_tile_order_len: usize`
2. Initialize the new order state in both constructors.
3. Add the helper surface for registry operations:
   - `mark_tile_recent(tile_id)`
   - `append_loaded_tile(tile_id)` or equivalent
   - `unload_if_needed()` stub or full implementation entry point
4. Add debug assertions for the registry invariants where practical:
   - map length matches active order length
   - active order contains each loaded tile exactly once
   - order is oldest -> newest
5. Add test-only hooks needed by later phases:
   - small custom limit support for tests
   - `loaded_tile_count()`
   - `is_tile_loaded(tile_id)`
   - optional order inspection helper if tests would benefit from it

## Notes
- Keep the field name `loaded_tiles`.
- Keep `MAX_LOADED_TILES` as the production constant.
- Prefer a test-only field/constructor/helper over making production behavior runtime-configurable.
- Do not add heap-backed recency tracking.

## Exit criteria
- `TileManager` has explicit order state.
- Constructors initialize that state correctly.
- Registry helpers and invariants exist.
- Test-only observability/limit control is available for later eviction tests.

## Verification
- `cargo test -p ridi-router-routing tile_manager -- --nocapture`
- If needed, add a small helper-focused unit test that only checks registry invariants without touching real eviction scenarios yet.
