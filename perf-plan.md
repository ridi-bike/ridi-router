# Perf plan: incremental loop detector for `Route::has_looped`

## Goal

Replace the current repeated route rescan in `Route::has_looped` with an incremental loop detector that is updated as the route grows and shrinks.

The target is to change loop detection from:

- scan many prior segments
- repeatedly hydrate points and lines
- repeatedly decode tags and allocate strings

into something much closer to:

- update a few small indexes on push/pop
- check a very small candidate set when evaluating the newest segment

## Short answer to the metadata question

Yes: **option 4 includes the metadata idea from option 2**.

It more or less depends on it.

A fast incremental loop detector needs a compact per-segment cache so it can answer loop questions without calling:

- `ctx.point(...)`
- `ctx.line(...)`
- `ctx.tag_set(...)`
- `ctx.tag_value(...)`

inside the hot loop.

So the design should include a per-segment metadata record, built once when the segment is added.

## Current behavior to replace

Today `Route::has_looped` does all of this every time it is called:

1. Find the starting position for the loop check by scanning for `since_point`
2. Read the newest segment's point and line data from `ctx`
3. Scan earlier segments from that checkpoint to the end
4. For each candidate segment:
   - hydrate the endpoint point
   - compute point-to-point distance
   - hydrate the line
   - hydrate tag set
   - decode `hw_ref` and `name`
   - compare road identity

This is expensive even before considering the large call count.

## Proposed design

Maintain an incremental `LoopDetector` alongside the route.

It should support three operations efficiently:

- `push(segment, meta)`
- `pop()`
- `has_looped(since_point)` with fast internal resolution to `since_idx`

> Note: the public checkpoint concept should stay as `since_point` to match the current itinerary model.
  The route-specific detector will resolve that point to an index using cached route state.
The newest segment is the only one that needs to be checked.

## Core idea

Split loop detection into two separate checks.

### 1. Exact point revisit

This is the cheap and precise case.

If the current segment ends on a point that already appeared earlier in the relevant route suffix, that is a loop.

### 2. Near revisit on the same road identity

This matches the current fuzzy logic:

- prior point is within `LOOP_DISTANCE_THRESHOLD`
- there are more than `LOOP_SEGMENT_THRESHOLD` segments between the two positions
- road identity matches by `hw_ref` or `name`

Instead of scanning the whole suffix, only check candidates that:

- share the same road identity
- are in nearby spatial buckets

## Suggested data structures

```rust
struct LoopMeta {
    end_point: MapDataPointRef,
    point_id: u64,
    lat: f32,
    lon: f32,
    hw_ref: Option<RoadKey>,
    name: Option<RoadKey>,
}

struct LoopDetector {
    // One meta entry per route segment, same index as route_segments.
    metas: Vec<LoopMeta>,

    // Exact point revisit lookup.
    point_hits: HashMap<MapDataPointRef, Vec<usize>>,

    // Spatial indexes scoped by road identity.
    hw_ref_cells: HashMap<RoadKey, HashMap<CellId, Vec<usize>>>,
    name_cells: HashMap<RoadKey, HashMap<CellId, Vec<usize>>>,
}
```

### `LoopMeta`

This is the option-2 metadata cache.

It should contain only the fields needed by loop detection.

That means:

- endpoint identity
- endpoint coordinates
- compact road identity keys for `hw_ref` and `name`

No adjacency, no decoded rules, no owned vectors.

### `RoadKey`

`RoadKey` should be a cheap comparable identifier for a road tag value.

Important detail: current tag value refs are tile-local. That means a raw tag index is probably not enough if the same road continues across tiles.

Good options:

- a global string interner that maps road strings to `u32`
- a `SmartString`-based cache stored once per route/detector
- a hybrid fast-path:
  - same tile => compare tag refs directly
  - different tile => compare interned string key

For this plan, assume `RoadKey` is a stable interned id.

### `CellId`

`CellId` is a coarse spatial bucket derived from `(lat, lon)`.

The detector should bucket points into small fixed-size cells so nearby checks only inspect a few buckets.

Example approach:

- choose a cell size around 25m to 50m
- quantize lat/lon to integer cell coordinates
- pack `(x, y)` into a `u64` or use `(i32, i32)` as the key

Then a near-point query only searches the current cell plus neighboring cells.

## Query algorithm

When `has_looped_since(since_idx)` is called, inspect only the newest segment.

Let:

- `current_idx = metas.len() - 1`
- `current = metas[current_idx]`

### Step 1: exact point revisit

Check `point_hits[current.end_point]`.

If any recorded prior index is:

- `< current_idx`
- `>= since_idx`

then return `true`.

This should be very cheap because the vector is usually tiny.

### Step 2: fuzzy same-road near revisit

For each present road identity:

- `current.hw_ref`
- `current.name`

look up nearby spatial cells for that key.

For each candidate index found in those cells:

1. skip if `candidate_idx < since_idx`
2. skip if `current_idx - candidate_idx <= LOOP_SEGMENT_THRESHOLD`
3. fetch `candidate = metas[candidate_idx]`
4. do a cheap distance precheck using cached coordinates
5. if it passes, do the exact threshold check
6. if under `LOOP_DISTANCE_THRESHOLD`, return `true`

If neither exact-point nor same-road-near-point matches, return `false`.

## Why this should be much faster

### The current version

Per call, cost is roughly:

- O(route suffix length)
- with expensive hydration inside the loop

### The proposed version

Per call, cost is roughly:

- O(number of exact point hits)
- plus O(number of same-road candidates in nearby cells)

That candidate set should usually be very small.

The big win is not just asymptotic behavior. It also removes repeated heavy work:

- no repeated point hydration
- no repeated line hydration
- no repeated tag set hydration
- no repeated string allocation for `hw_ref` / `name`

## Update operations

The detector must stay correct when the route changes.

### On push

When a segment is appended to the route:

1. build `LoopMeta` once
2. append to `metas`
3. push the segment index into `point_hits[end_point]`
4. compute `CellId`
5. if `hw_ref` exists, push index into `hw_ref_cells[hw_ref][cell]`
6. if `name` exists, push index into `name_cells[name][cell]`

### On pop

When the walker backtracks and removes the last segment:

1. pop the last `LoopMeta`
2. remove the last index from the relevant `point_hits` entry
3. remove the last index from the relevant road-cell entries
4. optionally delete empty vectors/maps

Because pushes and pops happen at the end, vectors-of-indices work well.

## Handling `since_point`

The current API passes `since_point: Option<&MapDataPointRef>`.

We should keep that public concept for now.

Reason: `since_point` belongs to itinerary semantics, not route semantics.
It means "ignore loop crossings before this waypoint chunk", which is still the right external concept even if the implementation uses indices internally.

The route should resolve `since_point` to `since_idx` through cached route state instead of rescanning `route_segments`.

The existing `point_hits: HashMap<MapDataPointRef, Vec<usize>>` already gives us what we need.

Implementation rule for exact behavior parity:

- if `since_point` is `None`, use `since_idx = 0`
- if `since_point` is present and appears in `point_hits`, use the **first** recorded index for that point
- if `since_point` is present and does not appear, fall back to `since_idx = 0`

That preserves the current behavior exactly, because the current implementation uses `.position(...)`, which also picks the first occurrence.

Important note to document in code:

- this may not match the original waypoint intent if the same point is visited multiple times
- the intended waypoint semantics are "since the last relevant waypoint transition"
- the current implementation does not model that precisely
- itinerary waypoint lists should also not normally contain the same point multiple times

We should add a code comment near the resolution logic documenting both:

- the current preserved behavior
- the possible semantic mismatch / future cleanup opportunity

Internally, after resolving `since_idx`, the detector should only work with indices.

## Geometry details

We do not need haversine here.

For this path, replacing haversine with a cheaper local-distance approximation is acceptable and should be part of the rewrite.

Reason:

- the threshold is only 50m
- the candidate distances are very small
- the error from a local planar approximation at this scale should not matter meaningfully for routing behavior

Suggested approach:

- store endpoint lat/lon in `LoopMeta`
- compute local x/y deltas in meters using a simple latitude/longitude scale factor
- compare squared distance against `LOOP_DISTANCE_THRESHOLD * LOOP_DISTANCE_THRESHOLD`
- avoid `sqrt` as well

So the final fuzzy distance check can be a cheap approximate metric, not just a prefilter.

Because this is the one intentional semantic relaxation in the rewrite, we should cement it with tests around the threshold behavior we actually want to preserve in practice.
## Ownership: `Route` vs `Walker`

Settled decision: keep the detector inside `Route`.

Why:

- `weight_no_loops` already reads from `Route`
- push/pop naturally mirror route mutation
- it avoids reshaping the current weight calculation API

Tradeoff:

- `Route::clone()` gets heavier
- all route clones now carry detector state too

That is acceptable for the first implementation. We can revisit it later if route cloning becomes visible in profiles.
## Expected complexity and memory tradeoff

### CPU

Expected large improvement because the detector turns route-wide rescans into tiny indexed lookups.

### Memory

Memory usage will increase because each segment now stores:

- cached coordinates
- road keys
- index entries in a few maps

That is likely a very good trade, given the current CPU and allocation profile.

## Risks and correctness concerns

### 1. Road identity semantics

Need to define exactly what counts as the same road:

- `hw_ref` only?
- `name` only?
- `hw_ref OR name` to match current logic?

The current code uses `hw_ref` or `name`, so the new detector should preserve that unless we intentionally change behavior.

### 2. Cross-tile identity

If the same named road spans multiple tiles, road identity comparison must still work.

That is why raw tile-local tag indices alone are risky.

### 3. Backtracking correctness

The detector must remain fully reversible on pop.

That makes append-only vectors of indices a good fit.

### 4. Bucket size tuning

If cells are too large, candidate sets grow.
If cells are too small, each query touches more neighbor cells.

This likely needs a small amount of tuning, but even a decent first guess should be much better than rescanning the route.

## Suggested implementation phases

### Phase 1: lock down current behavior with tests

Before changing the implementation, add unit tests for `Route::has_looped` that capture the current behavior, including edge cases.

Important test cases:

- exact endpoint revisit
- same `hw_ref` match with close points
- same `name` match with close points
- no match when only one side has `hw_ref` / `name`
- segment-gap threshold behavior
- `since_point = None`
- `since_point` present once
- `since_point` present multiple times, confirming that the **first** occurrence is used
- route suffix cases around the fuzzy distance threshold

Also add a comment in code documenting the preserved but potentially imperfect `since_point` semantics when the same point appears multiple times.

### Phase 2: big-bang implementation rewrite

Land the full detector rewrite in one patch:

- add `LoopMeta`
- add `RoadKey` interning
- add `point_hits`
- add `hw_ref_cells` and `name_cells`
- update route push/pop to maintain the detector incrementally
- resolve `since_point` to `since_idx` from cached route state
- replace haversine with cheap local squared-distance comparison
- keep `hw_ref OR name` semantics exactly
- keep first-occurrence `since_point` semantics exactly

The goal of this phase is that the new implementation passes the old-behavior tests unchanged, except for the explicitly accepted distance-calculation simplification where tests should define the desired practical behavior.

### Phase 3: validate and tune

- rerun the hotpath profile
- inspect candidate counts per query
- tune cell size if needed
- watch whether `Route::clone()` becomes measurable
- confirm `route::has_looped` and `weight_no_loops` drop sharply
## What success should look like

After implementation:

- `route::has_looped` should no longer dominate CPU
- `weight_no_loops` should drop sharply
- `ctx.point(...)`, `ctx.line(...)`, `ctx.tag_set(...)`, and `ctx.tag_value(...)` should mostly disappear from this path
- allocator pressure from tag decoding in this path should collapse

## Settled implementation decisions

1. Keep the detector in `Route`.
2. Preserve current loop semantics as closely as possible.
3. Preserve `hw_ref OR name` matching exactly.
4. Keep `since_point` as the public concept, but resolve it inside `Route` via cached point-hit indices.
5. When `since_point` appears multiple times, use the **first** occurrence to match current behavior exactly.
6. Use an interned `RoadKey` for `hw_ref` and `name`.
7. Replace haversine with a cheaper local distance approximation for the fuzzy near-point check.
8. Do the implementation as a big-bang rewrite after first adding tests that lock down current behavior.