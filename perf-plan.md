# `graph::get_point_from_tiles` performance review

## Scope

From `perf.md`:

- `graph::get_point_from_tiles`: **2,891,151 calls**, **463.03 s** cumulative time, **57.8 GB** cumulative allocation
- `tile_manager::get_rules_for_point`: **57.2 GB** cumulative allocation


## Confirmed constraints and design choices

- keep routing behavior unchanged; this is a perf-only plan
- if a large win seems to require subtle behavior changes, stop and discuss before implementing
- internal API changes are fine; external routing API changes are undesirable
- trade more resident memory for lower CPU time / allocation churn
- use **approximate** recency, not exact per-hit LRU, as long as eviction stays roughly reasonable
- apply the same read-mostly ideas not only to points, but also to nearby hot readers like line and tag access where it stays clean
- use **route-generation-task-local caches**, not graph-level shared caches, to avoid shared-cache write contention on mostly-new-point exploration
- start those task-local caches **unbounded** and only add eviction if profiling later shows memory pressure

## Non-goals / rejected starting points

- do **not** keep exact per-hit LRU ordering if it forces a write on every successful read
- do **not** start with a graph-level shared point cache; for the current workload, shared-cache write contention is expected to outweigh most cross-route reuse
- do **not** change external routing APIs as part of this work
- do **not** accept functional routing changes as an implicit side effect of a perf refactor
## What the function does today

`crates/ridi-router-routing/src/map_data/graph.rs:274-329`

For every lookup it:

1. takes the `tile_manager` **write lock**
2. finds the point with `get_point_by_id(...)`
3. fetches adjacent lines with `get_adjacent_by_id(...)`
4. fetches and hydrates turn rules with `get_rules_for_point(...)`
5. allocates fresh `Vec`s for `lines` and `rules`
6. returns a fully-owned `MapDataPoint`

That is expensive by itself. The bigger problem is that several of those steps do more work than this function actually needs.

## Main problems

### 1. `get_point_from_tiles` asks adjacency code for more than it needs

`graph::get_point_from_tiles` only needs the current point's `MapDataLineRef`s.

But it calls `tile_manager::get_adjacent_by_id(...)` (`crates/ridi-router-routing/src/rmdf/tile_manager.rs:712-799`), which:

- linearly searches the point again
- loads the tile's line table
- walks every connected line
- resolves the **other endpoint** of each line
- computes the **other tile id**
- may `ensure_tile_loaded(...)` for border crossings
- allocates a `Vec<(TileId, usize, TileId, u64)>`

Then `get_point_from_tiles` throws away half of that data and keeps only the line refs.

This is a clear over-fetch path.

### 2. Rule hydration copies whole rule sections on every point lookup

This is likely the biggest allocator.

`tile_manager::get_rules_for_point(...)` (`crates/ridi-router-routing/src/rmdf/tile_manager.rs:296-347`) calls:

- `MappedTile::get_rules()` (`crates/ridi-router-routing/src/rmdf/io.rs:169-183`)
- `MappedTile::get_rule_line_refs_payload()` (`crates/ridi-router-routing/src/rmdf/io.rs:185-207`)

Both return freshly allocated `Vec`s for the **entire tile section**, not borrowed slices.

So one point lookup can clone:

- all rule records in the tile
- all rule line-ref payload in the tile
- then the selected rule's `from`/`to` slices again
- then `get_point_from_tiles` maps those into new `Vec<MapDataLineRef>` again

That matches the profile: the alloc hotspot under `get_point_from_tiles` is almost mirrored by `tile_manager::get_rules_for_point`.

### 3. No fast path for points with no rules

`PointRecord` already contains `rules_count` and `lines_count` (`crates/ridi-router-common/src/format.rs:77-88`).

But `get_point_from_tiles` always calls `get_rules_for_point(...)`, even when `rules_count == 0`.

That means even rule-free points still pay the full rule-loading path.

### 4. Point lookup is linear, and it happens repeatedly

`tile_manager::get_point_by_id(...)` (`crates/ridi-router-routing/src/rmdf/tile_manager.rs:262-276`) does a linear scan through the tile's points.

`get_adjacent_by_id(...)` then linearly finds the same point again (`tile_manager.rs:721-729`).

With ~2.9M calls, even “small” linear scans add up.

### 5. Repeated lookups rehydrate the same immutable point over and over

`RoutingContext::point(...)` is just a thin wrapper over `get_point_from_tiles(...)` (`crates/ridi-router-routing/src/routing_context.rs:22-25`).

There are many hot-path call sites that only need cheap point facts like:

- point id
- lat/lon
- junction check (`lines.len() > 2`)
- flags like `residential_in_proximity` / `nogo_area`

So the code often rebuilds a full `MapDataPoint` when it only needs a tiny part of it.

### 6. Parallel route generation still funnels through a write lock

`MapDataGraph` stores `tile_manager` behind `RwLock<TileManager>`, and `get_point_from_tiles(...)` currently takes the **write** lock (`graph.rs:281`).

That is not only because of tile loads. Even a loaded-tile hit still goes through mutable `ensure_tile_loaded(...)`, which updates recency via `mark_tile_recent(...)` (`crates/ridi-router-routing/src/rmdf/tile_manager.rs:237-258`, `189-206`).

At the same time, route generation uses Rayon (`crates/ridi-router-routing/src/router/generator.rs:360-438`).

So many concurrent point lookups still serialize through one mutable tile-manager path. That does not directly explain the allocation spike, but it can cap throughput and amplify the cost of repeated hydration.

## Recommended changes

## Priority 1: stop using `get_adjacent_by_id` inside `get_point_from_tiles`

This is the cleanest low-risk fix.

Use the already-loaded `PointRecord` fields directly:

- `point_record.lines_offset`
- `point_record.lines_count`
- `tile.get_line_refs()`

Build `Vec<MapDataLineRef>` from that slice only.

Why this should help:

- removes the second point lookup
- removes border-crossing logic from a path that does not need it
- avoids building then remapping an adjacency tuple vector
- avoids possible neighbor-tile loads for a simple point hydration

This keeps the external behavior of `MapDataPoint.lines` the same, because the current adjacency function already returns the current tile id for line refs.

## Priority 2: add a zero-rules fast path

Before calling `get_rules_for_point(...)`, check:

```rust
if point_record.rules_count == 0 {
    rules = Vec::new();
}
```

This is tiny, safe, and likely helps a lot if most visited points have no turn restrictions.

Do the same for line refs if `lines_count == 0`, although that is probably less important.

## Priority 3: make tile rule access zero-copy

This is probably the biggest allocation win.

Change `MappedTile` so these return borrowed slices instead of owned `Vec`s:

- `get_rules() -> Result<&[RuleRecord]>`
- `get_rule_line_refs_payload() -> Result<&[u64]>`

Then update `TileManager::get_rules_for_point(...)` to slice directly from those borrowed sections.

Better versions, in increasing ambition:

1. **Good:** keep returning owned `TilePointRule`, but only allocate for the selected point's rules
2. **Better:** return borrowed slices / lightweight views and let `graph` materialize only once
3. **Best:** hydrate directly into the final output with exact capacities, skipping the temporary `TilePointRule` layer entirely

This change directly targets the `57.2 GB` allocation hotspot.

## Priority 4: make loaded-tile access read-mostly

This is the lock strategy change I would pick.

Use **Option B**:

- keep a read-only fast path for already-loaded tiles
- take the write lock only when a tile miss requires load / eviction
- replace exact per-hit LRU rotation with cheap per-tile recency metadata, e.g. `last_used: AtomicU64` or similar
- let eviction consult those timestamps under the write lock

Why this is a good fit here:

- loaded tiles stay cheap to read
- boundary / tile-miss handling still works
- Rayon workers stop serializing on every successful hit
- eviction remains good enough without exact LRU writes on every access

Important note: exact LRU and read-only hits fight each other. If we want the common case to avoid the write lock, recency updates cannot keep mutating the global order array on every access.

A practical shape is:

1. take read lock
2. if tile is already loaded, hydrate directly from the mapped tile and atomically bump recency
3. on miss, drop read lock, take write lock, load tile, update eviction state, retry

This should be paired with the `get_point_from_tiles` rewrite above, so the read-only path does not accidentally trigger boundary loads via `get_adjacent_by_id(...)`.

After the point path is working, extend the same pattern to nearby hot readers where possible:

- `get_line_from_tiles`
- `get_tag_set`
- `get_tag_value`

Those paths look like good candidates for the same split between read-only loaded-tile access and write-only tile miss handling.

They are also good candidates for task-local caching, especially tags and lines:

- many points and segments on the same road share the same street name / surface / smoothness data
- repeated line hydration is already visible in the profile
- a route-generation-task-local cache for lines and tag sets should capture that reuse without adding cross-thread contention

## Priority 5: add route-generation-task-local caches

The graph is immutable during routing, and each route-generation task revisits nearby data often enough that local caching should help without shared synchronization.

Use a **route-generation-task-local cache** design:

- each route-generation task owns its own caches
- caches are not shared across tasks or threads
- caches can be mutated freely without locks
- start unbounded for now

Recommended cache set:

- point cache keyed by `MapDataPointRef` or `(TileId, osm_id)`
- line cache keyed by `MapDataLineRef` or `(TileId, line_index)`
- tag-set cache keyed by `ElementTagSetRef`
- optionally tag-value cache keyed by `ElementTagValueRef` if profiling shows string lookup churn remains significant

Recommended value types:

- start with plain owned values (`MapDataPoint`, `MapDataLine`, `ElementTagSet`, maybe `String`)
- switch individual caches to `Arc<_>` only if profiling shows clone cost becomes noticeable

Why this matches the routing shape better:

- route generation runs many routes in parallel
- those routes often explore different areas and discover mostly new points
- a graph-level shared cache would see many misses and many inserts
- the synchronization cost of shared-cache writes could erase the hydration win
- task-local caches keep the hot path mutation-only and lock-free for that worker

Why line and tag caching are worth planning up front:

- every road contributes many points and segments
- those points and segments often reuse the same line or tag information repeatedly
- road metadata such as street name, pavement, highway class, and smoothness naturally has high local reuse

Lookup flow:

1. check the task-local cache
2. if miss, hydrate from tile data
3. insert into the task-local cache
4. return the cached value

Tradeoffs:

- less reuse across unrelated routes
- more duplicate cached values across workers
- higher total memory use than a single shared cache

Given the stated workload, those tradeoffs are acceptable if they remove synchronization from the common case. If later profiling shows heavy cross-route overlap, a shared cache can be revisited as an optional second phase, but it should not be the starting plan.

## Priority 6: add a point index per loaded tile

Build a lookup index once when a tile is loaded:

- `HashMap<u64, usize>`
- or a sorted `(osm_id, index)` vector with binary search

Use it in both:

- `get_point_by_id(...)`
- `get_adjacent_by_id(...)`

This removes the repeated linear scans.

If tile point counts are modest, even a compact sorted vector may be enough and cheaper than a hash map.

## Priority 7: stop hydrating full points when the caller only needs a field

This is a broader cleanup, but it will pay off.

Examples from hot code:

- replace `ctx.point(point_ref).id` with `point_ref.get_element_id()` where semantics allow it
- add cheap accessors for coordinates and flags
- add a cheap `point_degree(...)` / `is_junction_ref(...)` path based on `lines_count`

That avoids constructing `MapDataPoint` for read-only metadata checks.

## Suggested implementation order

1. **Rewrite `get_point_from_tiles` to read line refs directly from `PointRecord`**
2. **Skip rule hydration when `rules_count == 0`**
3. **Make `MappedTile` rule getters return borrowed slices**
4. **Add a read-only loaded-tile fast path for point reads**
5. **Switch recency tracking from exact per-hit LRU mutation to atomic last-used metadata (Option B)**
6. **Extend the same read-mostly pattern to nearby hot readers (`get_line_from_tiles`, `get_tag_set`, `get_tag_value`)**
7. **Simplify `get_rules_for_point` to avoid intermediate allocations**
8. **Add route-generation-task-local caches for points, lines, and tags**
9. **Add a per-tile point index**
10. **Audit hot call sites that only need id/coords/flags**

## Expected impact

### Biggest likely allocation win

**Borrowed rule slices + zero-rules fast path**

Reason: the current rule-loading stack copies whole tile rule data on each lookup.

### Biggest likely time win with low risk

**Stop using `get_adjacent_by_id` for point hydration**

Reason: it removes duplicated lookup and cross-tile adjacency work from a path that only needs line refs.

### Biggest longer-term combined win

**Task-local caching for points, lines, and tags**

Reason: the same roads, lines, and metadata are likely to be revisited repeatedly within one route-generation task, and local caches avoid both re-hydration and shared-cache contention.

## Validation plan

After each step, re-run the same perf workflow and compare at least:

- `graph::get_point_from_tiles`
- `tile_manager::get_rules_for_point`
- `tile_manager::get_adjacent_by_id`
- `graph::get_line_from_tiles`
- wall-clock route generation time

I would also add a focused micro-benchmark for repeated hydration of the same point and another for repeated hydration of many points from the same tile.

## Short version

The hotspot is not just “too many point lookups”.

`get_point_from_tiles` currently:

- does unnecessary adjacency work
- forces whole-tile rule copying
- repeats linear point scans
- rebuilds the same immutable point data again and again
- takes the write lock even on loaded-tile hits because recency bookkeeping mutates on every access

If I had to pick the first two fixes, I would do these:

1. **Read line refs directly from `PointRecord` instead of calling `get_adjacent_by_id`**
2. **Make rule access borrow from the mmap instead of allocating whole-tile `Vec`s per lookup**

Those two changes are the clearest match for the current profile.