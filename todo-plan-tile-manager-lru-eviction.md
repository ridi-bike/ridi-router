# Plan: real LRU behavior for the loaded tiles registry in `TileManager`

This file is the source of truth for follow-up work on this TODO.
It replaces `todo-review-tile-manager-lru-eviction.md`.

## Verdict

Relevant. Work should be done.

## Goal

Make `TileManager` enforce a real, deterministic least-recently-used policy for its **loaded tiles registry** so that:

- loaded tile count stays bounded everywhere, not only on one call path
- recently used tiles stay loaded
- colder tiles are unloaded first
- behavior is deterministic and test-covered

## Confirmed decisions

These decisions were reviewed and approved.

1. Keep the field name `loaded_tiles`.
2. Refer to the subsystem as the **loaded tiles registry**.
3. A newly loaded tile counts as recently used immediately.
4. Recency should be updated through `ensure_tile_loaded(...)` on both:
   - load miss
   - already-loaded hit
5. Eviction/unloading should happen immediately after loading/inserting a new tile and checking the max limit.
6. Keep `MAX_LOADED_TILES` as a constant for now.
7. Do not add special handling for the newly inserted tile.
8. Prefer an explicit ordered structure over timestamp metadata.
9. Use a fixed-capacity order structure, not a heap-backed growable one:
   - `loaded_tile_order: [TileId; MAX_LOADED_TILES]`
   - `loaded_tile_order_len: usize`
10. Rename helpers toward registry semantics:
   - `evict_if_needed` -> `unload_if_needed`
   - `mark_accessed` -> `mark_tile_recent`
11. Allow test-only hooks/helpers.
12. Tests must prove all of:
   - overflow unloads the oldest tile
   - touching an already loaded tile makes it newest
   - a hot tile survives while colder tiles are unloaded

## Why this matters

`MappedTile` holds:

- an open `File`
- a memory map

So this is not just a generic cache cleanup task. The loaded tiles registry controls:

- how many tile files remain open
- how many tile mmaps remain alive
- whether repeated local routing work keeps hot tiles resident

## Current problems in code

File: `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

1. `loaded_tiles` is just `HashMap<TileId, MappedTile>`.
2. There is no ordered recency tracking.
3. `evict_if_needed()` unloads an arbitrary tile via `keys().next()`.
4. Unloading is only triggered from `get_adjacent_by_id(...)`.
5. `ensure_tile_loaded(...)` does not enforce the limit.
6. Most tile-backed access paths do not affect recency except indirectly through initial loading.
7. Existing tests do not lock down true LRU behavior.

## Chosen implementation shape

### 1. Keep `loaded_tiles`

Keep:

```rust
loaded_tiles: HashMap<TileId, MappedTile>
```

Do not rename this field.

### 2. Add explicit order tracking for the loaded tiles registry

Add something like:

```rust
loaded_tile_order: [TileId; Self::MAX_LOADED_TILES],
loaded_tile_order_len: usize,
```

Notes:

- `TileId` is `Copy + Default`, so a fixed array is practical.
- Only the prefix `[..loaded_tile_order_len]` is active.
- Index `0` is oldest.
- Index `loaded_tile_order_len - 1` is newest.

### 3. Centralize recency updates in `ensure_tile_loaded(...)`

`ensure_tile_loaded(tile_id)` becomes the single place that updates recency.

Behavior:

- if tile is already loaded:
  - mark it recent
  - return `Ok(())`
- if tile is not loaded:
  - load tile
  - insert into `loaded_tiles`
  - append/promote it as newest in the order structure
  - call `unload_if_needed()`
  - return `Ok(())`

This gives true LRU without adding per-`get_` recency code everywhere.

### 4. Use order-based promotion, not timestamps

Implement `mark_tile_recent(tile_id)` against the order array.

Expected behavior:

- reverse-scan the active order from newest to oldest to find `tile_id`
- if not found, that is a bug/invariant violation for loaded tiles
- if already newest, do nothing
- otherwise rotate the active suffix left by 1 so that the accessed tile becomes newest

Example:

- before: `[A, B, C, D]`
- access `B`
- after: `[A, C, D, B]`

Implementation note:

- use in-place slice rotation on the active suffix rather than rebuilding a new vector
- at this fixed size, moving a handful of `TileId` values is acceptable

### 5. Unload oldest tile when over limit

Implement `unload_if_needed()` so that it enforces the limit immediately after a new load.

Behavior:

- if `loaded_tiles.len() <= MAX_LOADED_TILES`, return
- otherwise:
  - oldest tile is `loaded_tile_order[0]`
  - remove it from `loaded_tiles`
  - rotate the active order left by 1
  - decrement `loaded_tile_order_len`
  - log the unload

Because the order array capacity matches `MAX_LOADED_TILES`, a new tile load that would overflow should be handled as:

1. insert into `loaded_tiles`
2. temporarily append/promote in order logic
3. `unload_if_needed()` removes the oldest resident tile

No special protection is needed for the new tile. Since new loads are marked recent immediately, they naturally land at the newest end.

## Perf expectations

The chosen design is intentionally simple and should be fine at this scale.

### Why it is acceptable

- `MAX_LOADED_TILES` is only `100`
- `TileId` is very small (`u16 + u16`)
- the expensive work is tile file open + mmap, not moving tiny identifiers
- reverse scanning from the newest end should often find hits quickly because routing is geographically local

### Expected common case

For many accesses:

- the tile is already newest -> no-op
- or close to newest -> short reverse scan, short rotate

That matches the expected locality of adjacent-grid traversal.

## Files to change

### Primary

- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

### Test support

- likely `crates/ridi-router-routing/src/rmdf/tile_manager.rs` test module
- possibly `crates/ridi-router-test-support/src/lib.rs` if an additional synthetic multi-tile fixture helper is useful

## Planned code changes

### A. `TileManager` state

Extend `TileManager` with ordered registry state:

- `loaded_tiles: HashMap<TileId, MappedTile>`
- `loaded_tile_order: [TileId; MAX_LOADED_TILES]`
- `loaded_tile_order_len: usize`
- initialize the order array to `TileId::default()` values
- initialize length to `0`

### B. Helper methods

Add/adjust helpers with clear responsibilities:

- `mark_tile_recent(tile_id)`
- `append_loaded_tile(tile_id)` or equivalent small helper if useful
- `unload_if_needed()`

Keep helper naming aligned with the loaded tiles registry terminology.

### C. `ensure_tile_loaded(...)`

Make this the central registry-entry point.

Required behavior:

- on hit: call `mark_tile_recent(tile_id)`
- on miss:
  - load the tile
  - insert into `loaded_tiles`
  - register/promote tile as newest
  - call `unload_if_needed()`

### D. Accessors

The tile-backed accessors should continue calling `ensure_tile_loaded(...)` first.

That means true LRU recency will naturally cover:

- `get_point_by_id`
- `get_line_by_index`
- `get_rules_for_point`
- `get_closest_to_coords`
- `get_adjacent_by_id`
- `get_tag_value`
- `get_tag_set_record`

No extra recency helper calls should be added inside each accessor unless needed for borrow-checker structure.

### E. Remove old arbitrary eviction logic

Delete the current arbitrary `HashMap::keys().next()` unloading logic.

## Test plan

Use synthetic fixtures and test-only hooks. Do not rely on optional Montenegro data for this work.

### Test-only helpers to add

Add test-only observability helpers on `TileManager`, for example:

- `loaded_tile_count() -> usize`
- `is_tile_loaded(tile_id: TileId) -> bool`

### Test-only limit control

Add a test-only way to use a smaller limit, so eviction tests can run with a tiny synthetic setup instead of generating 101 real tiles.

Acceptable approaches:

- test-only constructor with a custom limit
- test-only constant override pattern scoped to tests
- test-only `TileManager` field if that is cleaner

Do not change production behavior or make the runtime limit configurable as part of this TODO.

### Required tests

#### 1. Overflow unloads oldest tile

Scenario:

- create manager with small test limit, e.g. `3`
- load tiles `A`, `B`, `C`
- load tile `D`

Assert:

- count remains `3`
- `A` is no longer loaded
- `B`, `C`, `D` remain loaded

#### 2. Accessing an already loaded tile makes it newest

Scenario:

- load `A`, `B`, `C`
- access `A` again through `ensure_tile_loaded(...)` via a normal tile-backed call path
- then load `D`

Assert:

- `B` is unloaded
- `A`, `C`, `D` remain loaded

This proves a hit refreshes recency.

#### 3. Hot tile survives while colder tiles are unloaded

Scenario:

- with small limit, keep revisiting one tile while loading new neighboring tiles
- then cause overflow

Assert:

- repeatedly touched tile remains loaded
- colder tiles are the ones unloaded

#### 4. Existing behavior still works

Keep current non-eviction behavior covered:

- missing-neighbor handling still succeeds
- cross-tile traversal still works

If the old Montenegro-based tests remain, they can stay, but they should not be the main evidence for this TODO.

## Implementation notes / invariants

These invariants should hold after the change:

1. `loaded_tiles.len() == loaded_tile_order_len`
2. The active prefix of `loaded_tile_order` contains each loaded tile exactly once.
3. Active order is oldest -> newest.
4. Every successful `ensure_tile_loaded(tile_id)` leaves `tile_id` as newest.
5. Overflow after a load unloads exactly one oldest tile.

Add debug assertions where practical in helper methods and tests.

## Non-goals

Not part of this TODO:

- making `MAX_LOADED_TILES` runtime-configurable in production
- redesigning `MappedTile`
- broader prefetching or manifest-driven residency strategies
- changing external routing APIs

## Suggested implementation order

1. Extend `TileManager` state with order tracking.
2. Add helper methods for promotion and unloading.
3. Move unload enforcement into `ensure_tile_loaded(...)`.
4. Remove the old unload call from `get_adjacent_by_id(...)`.
5. Add test-only hooks for limit + observability.
6. Add focused synthetic eviction tests.
7. Run targeted routing/tile-manager tests.

## Verification

Minimum verification target:

```bash
cargo test -p ridi-router-routing tile_manager -- --nocapture
```

Also run any nearby graph/routing tests affected by tile access behavior.
