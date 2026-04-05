# Phase 3: Same-tile spatial-index search + fallback

## Goal
Replace the current full-tile-only nearest lookup with the agreed two-stage same-tile search.

## Files
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- unit tests in the same file

## Planned changes
1. Add search helpers in `tile_manager.rs` for the spatial-index path:
   - query-cell encoding with precision `100`
   - ring cell-id generation for rings `0..=20`
   - binary search for `GridCellEntry.cell_id` in the sorted tile index
   - helper to iterate the point slice referenced by `points_offset` / `points_count`
2. Implement stage 1 search:
   - look only in the containing tile
   - scan grid cells ring-by-ring
   - evaluate candidates with the same filter helper from Phase 2
   - keep the best candidate by Haversine distance
3. Implement stage 2 fallback:
   - if stage 1 finds no eligible candidate, scan the full same tile
   - reuse the exact same filter helper and Haversine ranking
4. Keep behavior explicitly same-tile only.
   - no neighbor-tile expansion
   - no border-correct lookup work in this phase

## Tests to add in this phase
1. full-tile fallback works when no eligible point exists inside the 20-ring window
2. if Phase 1 includes spatial-index fixture support, add one direct ring-scan success test proving stage 1 can return a valid candidate without needing fallback

## Important implementation constraints
- Do not duplicate filter logic between stage 1 and stage 2.
- Do not change point-level rejection semantics.
- Prefer a binary search on `GridCellEntry` because writer output is sorted by `cell_id`.

## Validation
- Same-tile lookup uses `tile.get_spatial_index()?` first.
- Fallback still prevents false negatives inside the tile.
- Existing cross-tile adjacency behavior remains unchanged.

## Exit criteria
- `get_closest_to_coords(...)` performs the agreed two-stage search.
- The search improvement is covered by tests, especially the fallback case.
