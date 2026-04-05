# Review: nearest-point filtering semantics in `TileManager::get_closest_to_coords(...)`

## Verdict: partially relevant

## Summary

This TODO is **still relevant**, but the wording is now a bit stale.

The core claim is still true: the tile-backed closest-point lookup does **not** yet honor the intended filtering semantics for `RouterRules` or `limit_to_hw_tags`.

What is stale:
- the todo frames everything as one lookup problem, but there are now **two separate concerns**:
  1. **semantic filtering is missing** in closest-point selection
  2. **search strategy is incomplete** and still ignores the spatial index / neighboring search behavior
- routing already applies rule-based filtering later during navigation, so this is **not the only place** where rules matter anymore
- the acceptance item “routing tests pass” is already true, but that does **not** prove the lookup semantics are implemented

So this is **not obsolete**, and it is **not already covered** elsewhere. But it should be re-scoped as a follow-up on the tile-backed lookup path, not as a general routing-rules task.

## Current evidence from the codebase

### 1. `TileManager::get_closest_to_coords(...)` still has the unresolved TODOs in code

File: `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

The current implementation still contains these TODOs directly in the function body:
- `// TODO: Implement expanding ring search like PointGrid`
- `// TODO: Implement rules filtering`
- `// TODO: Implement highway tag filtering`

The function signature also shows the missing behavior clearly:
- `rules` is accepted as `_rules`
- `limit_to_hw_tags` is accepted as `_limit_to_hw_tags`

That means those parameters are intentionally unused right now.

### 2. The current lookup only applies two filters

File: `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

The actual loop only filters by:
- `point.lines_count == 0` → skip disconnected points
- `avoid_proximity_to_residential && point.residential_in_proximity()` → skip residential-proximate points

There is **no** filtering by:
- `RouterRules.highway`
- `RouterRules.surface`
- `RouterRules.smoothness`
- `limit_to_hw_tags`
- point-level rule records

### 3. The lookup still ignores the spatial index for search behavior

File: `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

The function reads:
- `tile.get_spatial_index()?`
- `GridCellEntry::encode_cell_id(...)`

but then does a full linear scan of all points in the tile instead of using the spatial index.

It also derives exactly one tile from the query coordinates and only searches that tile:
- `TileId::from_coords(lat, lon, self.tile_size_degrees)`
- `self.ensure_tile_loaded(tile_id)?`

So the current behavior is:
- deterministic within the loaded tile
- not using the intended indexed/ring-style search
- potentially wrong near tile borders if a closer candidate lives in a neighboring tile

### 4. The higher layers do pass filtering inputs, but the tile lookup ignores them

Files:
- `crates/ridi-router-routing/src/routing_context.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/routing_api.rs`
- `crates/ridi-router-routing/src/router/generator.rs`

`RoutingContext::closest_to_coords(...)` forwards both `rules` and `limit_to_hw_tags`.

`RoutingExecutor::generate(...)` calls closest-point lookup for start and finish with:
- `Some(&WP_LOOKUP_ALLOWED_HWS)`

`Generator` also calls closest-point lookup for waypoint generation with:
- `Some(&WP_LOOKUP_ALLOWED_HWS)`

So the API already expresses the intended behavior, but the tile-backed implementation does not honor it.

### 5. Rule enforcement exists elsewhere, but not in closest-point selection

File: `crates/ridi-router-routing/src/router/weights.rs`

The routing stack already applies rule logic while evaluating route segments:
- `weight_rules_highway(...)`
- `weight_rules_surface(...)`
- `weight_rules_smoothness(...)`
- `weight_check_avoid_rules(...)`

This is the main overlap with the todo.

However, this does **not** replace closest-point filtering. It only affects route expansion after the start/finish/waypoint point has already been chosen. So the current code can still seed routing from a point attached to a road type that the lookup was supposed to avoid.

### 6. Point-rule support is not ready in the tile format path

Files:
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
- `crates/ridi-router-common/src/format.rs`
- `crates/ridi-router-routing/src/rmdf/io.rs`

The tile-backed point load path still builds:
- `rules: Vec::new(), // TODO: Fetch rules from tiles`

The RMDF writer currently writes:
- `rules_offset: 0 // TODO: Calculate from rules`
- `serialize_rules(...)` returns `Vec::new()` with `// TODO: Implement based on MapDataRule structure`

The binary format defines `RuleRecord`, but the runtime reader currently has no rule accessor comparable to `get_points`, `get_lines`, `get_tag_set`, or `get_tag_value`.

So if “rules filtering” means using point-attached rule records during nearest lookup, that part is blocked by a broader unfinished rules-in-tiles pipeline.

### 7. Test coverage does not yet validate the missing semantics

Files:
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- local check: `cargo test -p ridi-router-routing`

Current state:
- `cargo test -p ridi-router-routing` passes
- but `test_missing_tile_handling` in `tile_manager.rs` still has an empty TODO body
- there are no targeted tests proving `get_closest_to_coords(...)` respects `RouterRules` or `limit_to_hw_tags`

So the workspace is green, but the specific behavior from this todo is not locked down.

## The actual problem

The real unresolved issue is narrower than the todo title suggests:

1. **Closest-point selection ignores filtering inputs that the API already carries.**
2. **The tile format/runtime path is not yet capable of point-rule filtering.**
3. **The search algorithm still does a single-tile linear scan instead of the intended indexed/ring-style search.**

That means the nearest-point API can currently return a point that is:
- connected to a disallowed highway class for waypoint/start/finish lookup
- inconsistent with future rule-aware semantics
- not actually the best candidate near tile boundaries

## What behavior is missing or risky

### Missing now
- honoring `limit_to_hw_tags` during candidate selection
- honoring route-rule-driven tag exclusions during candidate selection, if that is the intended contract
- any point-rule-based filtering from RMDF data
- tests for filtering combinations and tie behavior

### Risky now
- start/finish and generated waypoint lookup can pick points on roads outside `WP_LOOKUP_ALLOWED_HWS`
- route generation may start from a poor seed and rely on later routing logic to recover
- future implementation of rules-in-tiles could change lookup behavior substantially unless tests pin down the contract first
- current single-tile search can miss better candidates near borders

## Possible solution options

### Option A: Implement only `limit_to_hw_tags` filtering now

Use the candidate point’s adjacent lines to inspect line tag sets and allow a point only if at least one connected line has a `highway` tag in the allowed set.

Pros:
- matches existing callers immediately
- unblocks the most concrete current mismatch
- uses data already available in RMDF (`LineRecord`, `TagSetRecord`, tag strings)

Cons:
- does not solve point-rule filtering
- still leaves search strategy incomplete

### Option B: Implement tag-rule-aware filtering from `RouterRules` using adjacent line tags

Translate the rule maps already used in `router/weights.rs` into a pre-filter for candidate points, based on connected line tags.

Pros:
- closer to the todo’s original intent
- aligns seed-point selection with later route scoring/avoid logic

Cons:
- needs a clear contract: should candidate selection reject a point if **any** adjacent line is disallowed, or only if **all** usable adjacent lines are disallowed?
- risks duplicating rule interpretation logic in two places unless shared helpers are introduced
- still does not use point-rule records from RMDF

### Option C: Finish the RMDF rules pipeline first, then implement full rule-aware lookup

First make tiles actually serialize, load, and expose rule records; then implement lookup filtering on top of that.

Pros:
- cleanest architecture if point-level rules are truly required
- avoids temporary semantics that will later be replaced

Cons:
- broader scope
- delays fixing the already-visible `limit_to_hw_tags` mismatch

### Option D: Separate semantics from search optimization

Treat this as two tasks:
1. semantic filtering correctness
2. spatial-index / ring-search improvement

Pros:
- keeps the implementation plan focused
- avoids blocking correctness work on search optimization

Cons:
- nearest lookup may still be suboptimal near borders until phase 2

## Tradeoffs / implications

- **Do not treat the spatial-index TODO as the same thing as filtering semantics.** It matters, but it is a different axis.
- **Do not assume route weights make this obsolete.** They only operate after lookup has already chosen a seed point.
- **If point-rule filtering is required, this todo overlaps with unfinished RMDF rule serialization/loading work.** That dependency should be called out explicitly in the implementation plan.
- **If `limit_to_hw_tags` is the real current requirement, it can likely be implemented without waiting for rule-record support.**

## Recommended direction

Treat this todo as **still relevant**, but split it conceptually:

1. **First:** implement and test `limit_to_hw_tags` filtering in `TileManager::get_closest_to_coords(...)` using adjacent line tag sets.
2. **Second:** decide whether `RouterRules` filtering at lookup time should be based on line tags only, or on future point/rule records from RMDF.
3. **Third:** separately improve search behavior to use the spatial index and define how border-crossing nearest lookup should work.

That gives a practical order:
- fix the clear API mismatch already exercised by `WP_LOOKUP_ALLOWED_HWS`
- avoid blocking on unfinished rule serialization
- keep search optimization/search-scope as a dedicated follow-up

## Open questions

1. What is the exact contract for `RouterRules` during closest-point lookup?
   - reject a point if any connected segment is avoided?
   - reject only if all connected usable segments are avoided?
   - consider only `highway`, `surface`, `smoothness`, or also point-level rules later?

2. Should `limit_to_hw_tags` be applied before or after rule filtering?

3. Should nearest lookup be restricted to the containing tile, or should it search neighboring tiles when close to a border?

4. Is expanding-ring search meant only as a performance optimization inside one tile, or as part of cross-tile nearest correctness?

5. Is the intended result “closest eligible point” or “closest point with at least one eligible outgoing line”?

## Suggested implementation starting point

Start in:
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

Practical first slice:
1. add a small helper that checks whether a point has at least one adjacent line whose `highway` tag is in `limit_to_hw_tags`
2. add focused tests for `get_closest_to_coords(...)` with mixed adjacent highway tags
3. decide and document the rule-filter contract
4. only after that, decide whether the point-rule path requires separate RMDF work

If the implementation needs point-level rules from tile data, the next dependency is:
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
- `crates/ridi-router-routing/src/rmdf/io.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`

## Related files to inspect next

- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/routing_context.rs`
- `crates/ridi-router-routing/src/routing_api.rs`
- `crates/ridi-router-routing/src/router/generator.rs`
- `crates/ridi-router-routing/src/router/weights.rs`
- `crates/ridi-router-routing/src/rmdf/io.rs`
- `crates/ridi-router-common/src/format.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
