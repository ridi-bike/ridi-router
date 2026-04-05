# Plan: nearest-point filtering and search

This file replaces `todo-review-nearest-point-filtering-and-search.md` as the source of truth.

## Status
Relevant. Implement.

## Decisions confirmed

1. **Implement both highway-tag filtering and rules filtering now.**
2. **Rules filtering must match stable `main` behavior exactly.**
3. **Highway allowlist semantics:** a point is eligible if at least one connected line has a `highway` tag in the allowed set.
4. **Points whose adjacent lines have no `highway` tag are excluded when the allowlist is active.**
5. **Search scope stays same-tile only.** No cross-tile expansion in this todo.
6. **Search strategy:** same-tile spatial-index ring scan first, then full-tile fallback.
7. **Distance metric:** use Haversine distance for final ranking.
8. **Testing approach:** extend the synthetic RMDF test helper, add focused unit tests, and add one routing-level test proving `WP_LOOKUP_ALLOWED_HWS` is honored.

## Target behavior

`TileManager::get_closest_to_coords(...)` in `crates/ridi-router-routing/src/rmdf/tile_manager.rs` should:

- preserve the existing API shape for now
- honor `avoid_proximity_to_residential`
- honor `limit_to_hw_tags`
- honor `RouterRules` avoid rules for:
  - `highway`
  - `surface`
  - `smoothness`
- use the RMDF spatial index for bounded local candidate collection
- fall back to a full same-tile scan if the bounded search finds no eligible candidate
- rank candidates by Haversine distance

## Stable v1 behavior to preserve

Filtering semantics must match stable `main` (`main:src/map_data/graph.rs:get_closest_to_coords`) for the filtering part.

### Rules filtering
Build the avoid set from `RouterRules` using only `RulesTagValueAction::Avoid`:

- `rules.highway`
- `rules.surface`
- `rules.smoothness`

Then reject a point if **any adjacent line** has an avoided tag in any of those categories.

Important: this is point-level rejection, not line-survival logic.

Example:
- point touches one `secondary` line and one `track` line
- rider avoids `track`
- result: the point is rejected entirely

That is intentional because the agreed goal is to match `main` exactly for rules filtering.

### Highway allowlist filtering
If `limit_to_hw_tags` is active, reject the point if **all adjacent highway tags** are outside the allowed set.

Equivalent rule:
- keep the point if at least one adjacent line has an allowed `highway`
- reject the point if no adjacent line has an allowed `highway`
- reject the point if adjacent lines have no `highway` tag values at all

### v1 `check_limit_tags` nuance to preserve
Stable `main` only applies the allowlist check when the allowlist contains at least one highway value that is **not** already avoided by rules.

That means:
- if `limit_to_hw_tags = ["primary", "secondary"]`
- and rules avoid both `primary` and `secondary`
- then `main` disables the allowlist check instead of rejecting everything immediately

Preserve this behavior for now so v2 matches v1 filtering semantics.

## Chosen search strategy

### Why not leave the current full-tile scan?
It is simple, but it ignores RMDF's spatial index and does not move v2 toward the intended query path.

### Why not use bounded ring search only?
That would match `main` more closely, but it can return `None` even when the same tile contains a valid candidate outside the ring window.

### Agreed implementation
Use a two-stage same-tile search:

1. **Spatial-index ring scan** over the containing tile
2. **Full-tile fallback** if stage 1 finds no eligible candidate

This keeps search local and fast in the common case while avoiding false negatives inside the tile.

## Search algorithm details

### Stage 1: bounded ring scan
Use the tile RMDF spatial index:

- `tile.get_spatial_index()?`
- `tile.get_points()?`
- `GridCellEntry::encode_cell_id(lat, lon, 100)`

Use the same cell precision and ring width as stable `main` semantics:

- precision: `100`
- ring radius: `20`

Implementation shape:

1. Compute the query cell id from `(lat, lon)`.
2. Generate cell ids for rings `0..=20` around the query cell using the same global cell math as v1 `PointGrid`.
3. For each cell id, locate the matching `GridCellEntry` in the tile spatial index.
   - Preferred approach: binary search because entries are serialized in sorted `cell_id` order.
4. For each matching entry, evaluate the referenced point slice from `points_offset` and `points_count`.
5. Apply filters to each candidate point.
6. Keep the best eligible candidate by Haversine distance.

### Stage 2: full same-tile fallback
If stage 1 yields no eligible point:

- scan all points in the same tile
- apply the exact same filters
- choose the nearest eligible point by Haversine distance

This fallback is intentional and is the only planned deviation from stable `main` search behavior.

## Filter evaluation algorithm

For each candidate point:

1. Skip disconnected points (`lines_count == 0`).
2. If `avoid_proximity_to_residential` and `point.residential_in_proximity()`, reject.
3. Load adjacent line refs from the point's line-ref slice.
4. For each adjacent line:
   - load the `LineRecord`
   - load its `TagSetRecord`
   - resolve `highway`, `surface`, and `smoothness` tag values if present
5. Reject the point if **any** adjacent line matches an avoided rule tag.
6. If allowlist checking is active, reject the point unless at least one adjacent line has an allowed `highway` value.
7. Rank surviving points by Haversine distance.

## Suggested code structure

### `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

Add small internal helpers instead of putting all logic inside the loop.

Suggested helpers:

- `collect_avoid_tags(rules: &RouterRules) -> AvoidTagSet`
- `should_check_limit_tags(limit_to_hw_tags, avoid_tags) -> bool`
- `point_matches_filters(...) -> Result<bool>`
- `adjacent_line_tags(...) -> Result<...>`
- `iter_points_for_grid_cell(...)` or equivalent slice helper
- `haversine_distance_m(...) -> f32`

A simple internal representation for avoid tags is enough, for example:

- avoided highways: `HashSet<String>`
- avoided surfaces: `HashSet<String>`
- avoided smoothness values: `HashSet<String>`

No public API changes are required for this todo.

### `crates/ridi-router-routing/src/map_data/graph.rs`

No semantic changes expected here beyond forwarding the now-functional filtering behavior already passed through.

### `crates/ridi-router-routing/src/routing_context.rs`

No API change required.

## Test support work

Current synthetic RMDF helper support is too limited for low-level tag-filter tests. `crates/ridi-router-test-support/src/lib.rs` currently writes:

- points
- lines
- line refs
- rules
- rule-line refs

but not tag values / tag sets.

### Extend the helper
Extend `ridi_router_test_support::rmdf` so tests can write tagged lines.

Recommended changes:

1. Extend `TileSpec` with:
   - `tag_values: Vec<String>` or a compact equivalent
   - `tag_sets: Vec<TagSetRecord>`
2. Update `write_tile(...)` to serialize:
   - `StringEntry` records
   - tag string pool
   - `TagSetRecord` array
3. Set the RMDF header counts and section offsets correctly.
4. Keep defaults so existing tests do not need to change unless they use tags.

This test helper extension should be done first, because the new nearest-point tests depend on it.

## Tests to add

### Unit tests in `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

#### 1. Highway allowlist honors mixed adjacent tags
Create a point with two adjacent lines, for example:

- one line tagged `track`
- one line tagged `secondary`

Query with:
- `limit_to_hw_tags = Some(&WP_LOOKUP_ALLOWED_HWS)`
- default rules

Expected:
- the point is eligible because at least one adjacent line has an allowed highway

#### 2. Highway allowlist rejects points with no allowed highway
Create the nearest point with only disallowed highway tags, for example `track`, and a slightly farther point with `secondary`.

Expected:
- lookup returns the farther allowed point

#### 3. Rules filtering matches `main`
Create a point with mixed adjacency where one adjacent line has an avoided tag.

Variants to cover:
- avoided `highway`
- avoided `surface`
- avoided `smoothness`

Expected:
- the whole point is rejected if any adjacent line matches an avoided tag

This is the main semantic test for `_rules`.

#### 4. Residential avoidance plus highway filtering
Create candidates where:
- one point passes highway allowlist but is residential-proximate
- another point passes highway allowlist and is not residential-proximate

Expected:
- residential-proximate point is rejected when the flag is enabled

#### 5. Full-tile fallback works
Create a tile where:
- no eligible point exists inside the 20-ring search area
- an eligible point exists elsewhere in the same tile

Expected:
- lookup still returns the eligible farther point via fallback

This should explicitly document the intentional v2 search improvement over stable `main`.

### Routing-level test in `crates/ridi-router-routing/src/routing_api.rs`

Add one focused test proving `WP_LOOKUP_ALLOWED_HWS` is honored for start/finish lookup.

Recommended fixture shape:
- nearest candidate to start or finish is attached only to a disallowed highway such as `track`
- next candidate is attached to an allowed highway such as `secondary`

Expected:
- routing start/finish snapping uses the allowed candidate, not the nearest disallowed one

Use the upgraded synthetic helper if practical. If the routing-level fixture is easier to express with a generated tiny tile fixture, that is acceptable, but the default plan is to reuse the upgraded helper.

## Implementation order

1. Extend RMDF synthetic test helper to support tag values and tag sets.
2. Add tile-manager unit tests that currently fail.
3. Implement avoid-tag extraction from `RouterRules` in `TileManager`.
4. Implement point-level filter evaluation using adjacent line tags.
5. Implement same-tile spatial-index ring scan.
6. Add full-tile fallback.
7. Switch final distance ranking to Haversine.
8. Add the routing-level `WP_LOOKUP_ALLOWED_HWS` test.
9. Run targeted tests, then full crate tests.

## Validation checklist

- `limit_to_hw_tags` is no longer ignored
- `_rules` is no longer ignored
- filtering semantics match stable `main`
- same-tile search uses the RMDF spatial index first
- same-tile fallback prevents false negatives within the tile
- final candidate choice uses Haversine distance
- routing API start/finish snapping honors `WP_LOOKUP_ALLOWED_HWS`

## Non-goals for this todo

Do not include these here:

- cross-tile nearest-point expansion
- border-correct nearest lookup across neighbor tiles
- removing `_rules` from the API
- changing rule semantics away from stable `main`
- priority-based snapping behavior

## Follow-up todos after this lands

1. Decide whether nearest-point lookup should keep matching `main` rule semantics long-term.
   - A future design may prefer line-survival semantics instead of point-level rejection.
2. Decide whether same-tile fallback should remain or whether v2 should move to strictly bounded local search.
3. Define border behavior explicitly and, if needed, implement cross-tile expansion.
4. Revisit whether `_rules` belongs in this layer at all once v2 snapping behavior stabilizes.
