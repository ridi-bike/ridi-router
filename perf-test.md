# Quick and dirty perf test for option 1: `RoutingContext` adjacency cache

## Goal
Test the core hypothesis behind option 1:

> repeated same-point adjacency lookups inside one `RoutingContext` are a meaningful part of the current hotspot

This document is intentionally about a **temporary perf probe**, not the final implementation.

If the result is good, revert the dirty changes and do the proper implementation cleanly.

---

## What to change

### Target file
- `crates/ridi-router-routing/src/routing_context.rs`

### Keep this experiment small
For the test, only change `RoutingContext`.

Do **not** change:

- `MapDataGraph`
- `TileManager`
- walker logic
- file formats
- lock behavior

This keeps the perf result easy to interpret.

---

## Dirty implementation plan

### 1. Add an adjacency cache to `RoutingCaches`
Add a new field:

```rust
adjacent: HashMap<MapDataPointRef, Vec<(MapDataLineRef, MapDataPointRef)>>,
```

This is fine for the probe.

Notes:

- use `Vec`, not `SmallVec`, for the test
- no need to optimize the cache shape yet
- we just want to know if caching adjacency at all is worthwhile

### 2. Update `RoutingContext::adjacent(...)`
Current code just forwards:

```rust
pub(crate) fn adjacent(
    &self,
    point_ref: &MapDataPointRef,
) -> Vec<(MapDataLineRef, MapDataPointRef)> {
    self.graph.get_adjacent(point_ref.clone())
}
```

Change it to:

1. look for `point_ref` in `self.caches.borrow().adjacent`
2. if found, return the cached vec clone
3. otherwise call `self.graph.get_adjacent(point_ref.clone())`
4. insert the result into the cache
5. return a clone of the stored value, or insert then return the computed vec

A simple version is enough. Example shape:

```rust
pub(crate) fn adjacent(
    &self,
    point_ref: &MapDataPointRef,
) -> Vec<(MapDataLineRef, MapDataPointRef)> {
    if let Some(adjacent) = self.caches.borrow().adjacent.get(point_ref).cloned() {
        return adjacent;
    }

    let adjacent = self.graph.get_adjacent(point_ref.clone());
    self.caches
        .borrow_mut()
        .adjacent
        .insert(point_ref.clone(), adjacent.clone());
    adjacent
}
```

That is good enough for the probe.

### 3. Keep everything else unchanged
No API redesign.
No borrowed slices.
No graph-level cache.
No tile-manager work.

The point of the test is isolation.

---

## Why this is a valid probe
If this dirty change produces a meaningful improvement, then we learn:

- repeated same-point adjacency lookups are real
- task-local caching is a valid direction
- we should invest in a cleaner version

If this dirty change does **not** move the needle much, then:

- option 1 is probably not the first lever
- option 2 or 3 should move higher in priority

---

## Perf test procedure

## Baseline run
From repo root, run the same profiled workflow used in `perf.md`.

```bash
RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia
```

Save the resulting report as the baseline, for example by copying the output report or keeping the regenerated `perf.md` in a separate branch/commit.

Record at least:

- router wall time
- `graph::get_adjacent` calls / total / alloc
- `tile_manager::get_adjacent_by_id` calls / total / alloc
- `walker::get_fork_segments_for_segment_with_context`
- `walker::move_forward_to_next_fork_with_context`
- `walker::get_roundabout_exits_with_context`

## Dirty cache run
Apply the temporary `RoutingContext` cache change and rerun:

```bash
RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia
```

Capture the same metrics.

---

## What to compare

### Primary metrics
1. `graph::get_adjacent`
   - total time
   - call count
   - allocation total

2. `tile_manager::get_adjacent_by_id`
   - total time
   - call count
   - allocation total

3. overall route-generation wall time

### Secondary metrics
4. walker hotspots
   - `get_fork_segments_for_segment_with_context`
   - `move_forward_to_next_fork_with_context`
   - `get_roundabout_exits_with_context`

5. memory picture
   - RSS if available from live sampling
   - cumulative allocation totals

---

## How to interpret the result

### Strong signal that option 1 is valid
I would consider the experiment successful if you see most of these:

- `graph::get_adjacent` time clearly down
- `tile_manager::get_adjacent_by_id` call count clearly down
- adjacency alloc totals clearly down
- some visible improvement in walker hotspot totals
- some visible improvement in route wall time

Rough rule of thumb:

- **10%+ reduction** in `graph::get_adjacent` total time is already interesting
- **20%+ reduction** is a strong signal
- any clear wall-time improvement on top of that makes the case better

### Weak signal
If only alloc drops a little but time barely moves, the cache may not be worth doing first.

### Negative signal
If the change does not materially improve adjacency hotspots, then repeated same-point lookup is probably not the main problem.

In that case:

- prioritize option 2 next
- keep option 3 as cleanup

---

## Guardrails for the dirty test

### Keep the diff tiny
The temporary experiment should touch as little code as possible.

### Do not polish the code
Avoid spending time on:

- API cleanup
- naming cleanup
- perfect cache abstraction
- borrowed-return redesign
- `SmallVec`
- extra tests unless needed to compile

### Do not mix in option 2 or 3
If you mix changes, you lose the signal.

---

## After the test

### If the result is good
1. note the before/after numbers
2. revert the dirty commit
3. do the proper implementation cleanly

The proper implementation can then decide:

- whether to keep `Vec` or use `SmallVec`
- whether to return cloned data or redesign the API
- whether to add dedicated tests
- whether to pair it immediately with option 2

### If the result is bad
1. revert the dirty change
2. move to option 2 as the next experiment

---

## Suggested commit flow

### Baseline
```bash
git checkout -b perf-adjacent-cache-probe
```

### Apply dirty change
Edit only `crates/ridi-router-routing/src/routing_context.rs`.

### Run perf
```bash
RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia
```

### If useful, keep the numbers and revert
```bash
git checkout -- crates/ridi-router-routing/src/routing_context.rs
```

or simply reset the branch after recording results.

---

## Expected outcome
My guess is that this probe has a decent chance of showing value, because the walker logic revisits the same junctions and roundabout points.

But this dirty test is exactly the right way to find out before doing a proper implementation.