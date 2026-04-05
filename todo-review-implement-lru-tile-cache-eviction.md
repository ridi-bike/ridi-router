# Review: real LRU tile-cache eviction in `TileManager`

## Verdict: relevant

## Summary

This TODO is still relevant.

The current `TileManager` does have a cache size limit, but it does **not** implement LRU eviction. Instead, it removes the first key returned by a `HashMap` iterator, which is not a real recency policy and is not a good basis for deterministic behavior. The code also does **not** track tile access at all, so the acceptance criteria about keeping hot tiles loaded is not currently satisfied.

The todo text is also a bit incomplete: the main gap is not only that eviction is not LRU, but also that eviction is only triggered from one code path (`get_adjacent_by_id`), not from tile loading generally.

## Current evidence from the codebase

1. `TileManager` stores loaded tiles in a plain `HashMap<TileId, MappedTile>` with no access metadata.
   - `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
   - `TileManager` fields at the top of the file show:
     - `loaded_tiles: HashMap<TileId, MappedTile>`
     - no timestamp/counter/list for recency

2. The cache limit exists, but eviction is explicitly a stub.
   - `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
   - `const MAX_LOADED_TILES: usize = 100;`
   - `fn evict_if_needed(&mut self) -> Result<()>` contains:
     - comment: `// Simple strategy: remove arbitrary tile`
     - comment: `// TODO: Implement proper LRU tracking`
     - implementation: `self.loaded_tiles.keys().next().cloned()` followed by `remove`

3. Accesses do not update any recency state, because no recency state exists.
   - `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
   - Methods like `get_point_by_id`, `get_line_by_index`, `get_closest_to_coords`, `get_tag_value`, and `get_tag_set_record` call `ensure_tile_loaded(...)` and then read from `loaded_tiles`, but do not record usage.

4. Eviction is only triggered from `get_adjacent_by_id`, not from tile loading itself.
   - `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
   - `ensure_tile_loaded(...)` loads and inserts the tile, but does not call `evict_if_needed()`.
   - `evict_if_needed()` is called near the end of `get_adjacent_by_id(...)`.
   - This means the effective cache bound is not enforced uniformly across all tile-loading entry points.

5. The surrounding routing code uses `TileManager` through multiple accessors, not only adjacency traversal.
   - `crates/ridi-router-routing/src/map_data/graph.rs`
   - Examples:
     - `get_point_from_tiles(...)` calls `get_point_by_id(...)` and `get_adjacent_by_id(...)`
     - `get_line_from_tiles(...)` calls `get_line_by_index(...)`
     - `get_tag_value(...)` calls `get_tag_value(...)`
     - `get_tag_set(...)` calls `get_tag_set_record(...)`
     - `get_closest_to_coords(...)` calls `TileManager::get_closest_to_coords(...)`
   - So if many different tiles are touched through non-adjacency paths, the current eviction hook is not sufficient.

6. The existing tests do not cover eviction behavior.
   - `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
   - Present tests:
     - `test_load_manifest`
     - `test_single_tile_query`
     - `test_cross_tile_traversal`
     - `test_missing_tile_handling`
   - `test_missing_tile_handling` is an empty TODO body, so it does not validate behavior yet.
   - There is no test for overflow, eviction order, or hot-tile retention.

7. The current test setup is weak for this area.
   - Running `cargo test -p ridi-router-routing tile_manager -- --nocapture` currently passes, but the tile-manager tests skip themselves when `test_data/montenegro_tiles` is absent.
   - That means the current green test result does not prove cache correctness.

## The actual problem

The actual problem is larger than the original todo text suggests:

- eviction is not LRU
- cache behavior is not meaningfully deterministic
- hot tiles are not protected from eviction
- cache enforcement is not centralized in tile loading
- there are no real tests locking down overflow behavior

Because `MappedTile` holds a file handle and memory map (`crates/ridi-router-routing/src/rmdf/io.rs`), this is not just a cleanliness issue. The cache exists partly to avoid keeping too many mapped files open at once.

## What behavior is missing or risky

- Repeatedly used tiles can be evicted even if they are the most active tiles.
- Two runs with the same workload should not rely on `HashMap` iteration order for eviction choice.
- The cache can grow past the intended limit on code paths that load tiles without going through `get_adjacent_by_id(...)`.
- There is no regression test that proves overflow handling works.
- There is no regression test that proves a frequently reused tile stays resident while colder tiles are dropped.

## Possible solution options

### Option A: Keep `HashMap` + add explicit recency tracking

Maintain:
- `loaded_tiles: HashMap<TileId, MappedTile>`
- a monotonic access counter on `TileManager`
- per-tile last-access metadata in a second map or wrapper struct

Eviction would scan for the minimum last-access value.

### Option B: Store a wrapper per tile with access metadata

Replace the map value with something like:
- `HashMap<TileId, LoadedTileState>`
- where `LoadedTileState` contains `MappedTile` and `last_access`

This is a straightforward extension of the current design and keeps lookups simple.

### Option C: Use an ordered structure for recency

Use a structure that supports:
- O(1) or near-O(1) lookup by `TileId`
- efficient promotion on access
- efficient eviction of oldest entry

This could be done with a custom linked-list pattern, an insertion-ordered map plus remove/reinsert, or a small cache helper crate if adding a dependency is acceptable.

## Tradeoffs / implications

- **Simplest implementation:** per-tile `last_access` metadata with linear scan on eviction. With `MAX_LOADED_TILES = 100`, a scan is probably acceptable and easy to reason about.
- **More efficient implementation:** ordered recency structure, but more moving parts and more maintenance burden.
- **Centralizing access tracking:** every method that returns data from a tile should update recency consistently; otherwise the policy will be inaccurate.
- **Centralizing eviction:** eviction should happen at load time or immediately after insertion, not only in one higher-level query path.
- **Testing:** deterministic tests likely need a small synthetic tile set or a test-only constructor/setup, rather than relying on external Montenegro fixtures that may not exist locally.

## Recommended direction

Use **Option B**:

- keep the `HashMap` keyed by `TileId`
- wrap each loaded tile in a small state struct containing `MappedTile` plus recency metadata
- add a monotonic access counter on `TileManager`
- update that metadata in a single helper used by all tile-access methods
- call eviction from `ensure_tile_loaded(...)` after inserting a newly loaded tile
- evict the tile with the oldest recorded access value

Why this direction:
- small change to current structure
- easy to test
- clear semantics
- likely fast enough for a max cache size of 100

## Open questions

1. Should a newly loaded tile count as "recently used" immediately, or only after actual read access?
   - Practically, marking it on load is probably fine.

2. Should eviction happen before or after inserting a new tile?
   - After insert is simpler if the rule is "allow temporary size N+1, then evict one".

3. Do all read paths need to update recency, including tag lookups?
   - Probably yes, if those lookups keep the file map useful for ongoing routing work.

4. Should `MAX_LOADED_TILES` remain hardcoded?
   - Not required for this TODO, but worth keeping in mind if datasets or deployment environments vary.

5. How should tests create enough distinct tiles to exercise eviction?
   - If fixture generation is heavy, a small synthetic test fixture may be better than depending on `test_data/montenegro_tiles`.

## Suggested implementation starting point

Start in:
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

Practical first steps:
1. Introduce a `LoadedTileState` wrapper around `MappedTile`.
2. Add a monotonic `access_counter` to `TileManager`.
3. Add a helper that marks a tile as accessed.
4. Make `ensure_tile_loaded(...)` both load and enforce cache size.
5. Update all tile read paths to mark access consistently.
6. Add focused tests that do not depend on optional external tile data.

## Related files to inspect next

- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/rmdf/io.rs`
- `todo-implement-lru-tile-cache-eviction.md`
- `thoughts/plans/rmdf_memory_mapped_tiles/06_tile_manager_multi.md`
- `thoughts/reviews/rmdf_memory_mapped_tiles-review.md`
