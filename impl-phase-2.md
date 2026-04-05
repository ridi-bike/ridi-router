# Phase 2: Wire real LRU behavior into `ensure_tile_loaded(...)`

## Objective
Make `ensure_tile_loaded(...)` the single recency entry point and enforce immediate oldest-first unloading after overflow.

## Why this phase exists
The approved design says recency must be updated on both load misses and already-loaded hits, and that unloading must happen immediately after inserting a new tile. This is the behavior change phase.

## Primary file
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

## Changes in scope
1. Change `ensure_tile_loaded(tile_id)` so that:
   - on hit: it calls `mark_tile_recent(tile_id)` and returns `Ok(())`
   - on miss: it loads the tile, inserts it into `loaded_tiles`, appends/promotes it as newest, calls `unload_if_needed()`, then returns `Ok(())`
2. Implement `mark_tile_recent(tile_id)` using the fixed array order structure:
   - reverse-scan active entries from newest to oldest
   - treat a missing loaded tile as an invariant violation
   - if already newest, do nothing
   - otherwise rotate the active suffix so the tile becomes newest
3. Implement `unload_if_needed()` so that:
   - it returns early when within the limit
   - if over limit, it unloads exactly `loaded_tile_order[0]`
   - it removes that tile from `loaded_tiles`
   - it rotates the active order left and decrements `loaded_tile_order_len`
   - it logs the unload
4. Remove the old arbitrary eviction behavior based on `HashMap::keys().next()`.
5. Remove the `get_adjacent_by_id(...)`-specific post-loop eviction call, since limit enforcement now belongs in `ensure_tile_loaded(...)`.
6. Keep existing tile-backed accessors calling `ensure_tile_loaded(...)` first so recency is updated consistently through normal access paths.

## Notes
- A newly loaded tile is immediately recent.
- No special protection is needed for the newly inserted tile.
- After every successful `ensure_tile_loaded(tile_id)`, that tile should be newest.
- Overflow after a load should unload exactly one oldest tile.

## Exit criteria
- `ensure_tile_loaded(...)` is the only recency update path.
- Overflow unloads the deterministic oldest tile.
- Old arbitrary eviction logic is gone.
- Registry length and order stay in sync under debug assertions.

## Verification
- `cargo test -p ridi-router-routing tile_manager -- --nocapture`
- Review any touched cross-tile traversal tests to confirm missing-neighbor behavior still works after moving unload enforcement.
