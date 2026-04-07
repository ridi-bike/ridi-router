# Perf improvement plan: `weights::weight_check_distance_to_next`

## Goal

Reduce the CPU and allocation cost of `weights::weight_check_distance_to_next` without changing routing behaviour, except for any explicitly approved distance-approximation change.

## Out of scope

- lightweight graph/point hydration APIs
- broader tile manager refactors
- changing rule semantics unless explicitly agreed

## Key findings from the current code

1. `weight_check_distance_to_next` is currently evaluated inside the per-fork-choice loop.
2. The weight does **not** depend on `current_fork_segment` or `walker_from_fork`.
3. Today, `LastSegmentDoNotUse` only helps **after** one fork choice has already evaluated all weights.
   - It prevents later fork choices from being evaluated.
   - It does **not** stop the remaining weights for the first evaluated choice.
4. `weight_check_distance_to_next` currently does unnecessary route work:
   - `split_at_point(check_from)`
   - clone suffix into a new `Vec<Segment>`
   - reverse scan over the cloned suffix
5. That reverse scan checks junction-ness via `ctx.point(...)`, which is relatively expensive.
6. The existing loop-detector metadata already contains useful route-history infrastructure:
   - one meta entry per route segment
   - `point_hits: HashMap<MapDataPointRef, Vec<usize>>`
   - cached lat/lon
   - cached road identity

## Final planning decisions

1. Start with **tests first** to lock down current behaviour.
2. Apply a **route-once gate stage** for these route-level `LastSegmentDoNotUse` weights:
   - `weight_no_loops`
   - `weight_progress_speed`
   - `weight_check_distance_to_next`
   - `weight_check_avoid_rules`
3. Replace `split_at_point(...).get_junctions_from_end(...)` with a **single no-allocation route query**.
4. Reuse and extend the existing **loop-detector route metadata** instead of introducing a separate cache.
5. Preserve the current repeated-point boundary behaviour and add a **code comment** documenting the caveat after the refactor.
6. Use a **pure 3% guarded exact fallback** for the distance comparison in `weight_check_distance_to_next`.
7. No code changes are part of this document; this file is the planning artifact.

---

## Phase 0: Behaviour-locking tests first

### A. Add focused unit tests for `weight_check_distance_to_next`

Add tests that cover the current semantics before any refactor.

Suggested cases:

1. **disabled rule returns zero weight**
   - `progression_direction.enabled = false`
   - expect `ForkChoiceUseWithWeight(0)`

2. **empty route returns zero weight**
   - no last segment
   - expect `ForkChoiceUseWithWeight(0)`

3. **not enough junctions back returns zero weight**
   - fewer than `check_junctions_back + 1` junctions since the boundary
   - expect `ForkChoiceUseWithWeight(0)`

4. **returns `LastSegmentDoNotUse` when current endpoint is farther from `next` than the chosen historical junction**

5. **returns zero when current endpoint is not farther**

6. **respects `switched_wps_on.last().on_point` as the lower boundary**
   - ensure the check only considers the route suffix after the active boundary

7. **document repeated-point boundary behaviour**
   - important because current code finds the boundary by point search
   - if the same point appears multiple times, current semantics may not match the conceptual waypoint-switch boundary
   - add a test to pin the current behaviour before refactoring

### B. Add route-query equivalence tests in `route/mod.rs`

Before replacing the implementation, add tests that compare:

- current behaviour:
  - `route.split_at_point(boundary).get_junctions_from_end(ctx, n)`
- new behaviour:
  - `route.nth_junction_from_end_since_...(ctx, boundary_or_idx, n)`

Suggested cases:

1. no boundary match
2. boundary at start
3. boundary in the middle
4. not enough junctions
5. repeated boundary point

### C. Add navigator execution-count tests

Add tests that prove route-level gates are evaluated once per fork event, while fork-dependent weights are still evaluated per choice.

Suggested cases:

1. route-level weight returning `LastSegmentDoNotUse`
   - called once
   - per-fork weights are not called

2. multiple route-level weights
   - if the first returns `LastSegmentDoNotUse`, later route-level weights are not called

3. passing route-level weights + per-fork weights
   - route-level weights called once
   - per-fork weights called once per choice

These tests should be added before the navigator refactor.

---

## Phase 1: Apply `LastSegmentDoNotUse` as a true route-once gate

### Why

These weights are route-only and can invalidate the whole current route state:

- `weight_no_loops`
- `weight_progress_speed`
- `weight_check_distance_to_next`
- `weight_check_avoid_rules`

They should be executed once per fork event, not once per fork choice.

### Implementation direction

Introduce explicit weight staging.

Possible shape:

- extend `WeightCalc` with a scope/stage flag, for example:
  - `RouteOnce`
  - `PerForkChoice`

Then in `Navigator::generate_routes_with_context`:

1. when a fork is reached, run all `RouteOnce` weights once
2. if any returns `LastSegmentDoNotUse`, prune immediately
3. otherwise evaluate only `PerForkChoice` weights for each candidate choice

### Notes

- This is stronger than the current behaviour.
- Today the code only skips later fork choices after the first evaluated choice reports `LastSegmentDoNotUse`.
- After the refactor, route-level pruning happens before any per-choice scoring work.

### Expected benefit

- removes repeated work for all route-only gate weights
- should directly reduce `weight_check_distance_to_next`
- should also help `weight_no_loops`, `weight_progress_speed`, and `weight_check_avoid_rules`

---

## Phase 2: Replace route slicing with a no-allocation route query

### Current problem

`weight_check_distance_to_next` currently does:

1. find `check_from`
2. `split_at_point(check_from)`
3. clone route suffix
4. reverse scan for the Nth junction

This creates avoidable allocations and duplicate scans.

### New route API

Add a route helper that directly scans the existing route history.

Likely shape:

- `Route::get_junctions_from_end_since_idx(...)`
- or `Route::nth_junction_from_end_since_idx(...)`

Recommended return value:

- `Option<&Segment>`
- or `Option<usize>` if later logic should work directly with metadata indices

### Planned behaviour

Given:

- `since_idx` = inclusive lower bound in route history
- `num_of_junctions`

The method should:

1. scan `route_segments` from the end toward `since_idx`
2. count junction endpoints
3. return the segment at the Nth junction from the end
4. perform no allocation and no cloning

### Why index-based is better here

- avoids `position(...)` + clone + second scan
- lets us use existing route metadata directly
- makes it much easier to reuse loop-detector state

---

## Phase 3: Reuse and extend the existing route metadata

### Reuse from loop detector

The current `LoopDetector` already stores one meta entry per segment.

We should reuse or extend that metadata instead of creating a separate cache just for this weight.

### Candidate extension

Add `is_junction` to the existing meta entry.

That allows the new route query to count junctions without repeated `ctx.point(...).is_junction()` calls.

### Candidate helper accessors

Expose minimal route/history helpers backed by existing metadata, for example:

- `route.route_index_first_for_point(point_ref)`
- `route.route_index_last_for_point(point_ref)`
- `route.segment_meta(idx)`
- `route.nth_junction_from_end_since_idx(...)`

### Important caveat

The existing `point_hits` map can help find route indices for a point, but it does **not** automatically encode the exact waypoint-switch moment.

If the same point appears multiple times, there are two possible semantics:

1. **preserve current behaviour**
   - use the same effective boundary as the existing `split_at_point(...).position(...)` behaviour
   - add a code comment after the refactor documenting that repeated points can make this differ from the conceptual waypoint-switch boundary
2. **preserve conceptual waypoint-switch intent**
   - store the exact route index when the switch happened

For this pass, preserve current behaviour and document the caveat in code.

---

## Distance calculation: haversine vs approximation

## Current state

`weight_check_distance_to_next` currently uses haversine distance through `MapDataPoint::distance_between`.

The loop detector already uses a cheaper planar approximation for a different purpose.

## Important difference from `has_looped`

The approximation used in `has_looped` is safe there because:

- distances are very small
- the threshold is small and local (`50 m`)
- it only needs a near/far check

For `weight_check_distance_to_next`, the distances to `itinerary.next` can be much larger.

That means approximation error matters more, especially when the two compared distances are close.

## Measured approximation results

The planar approximation used by `has_looped` is **not** safe enough to reuse blindly for this weight at continental scale.

The useful result from the analysis was a **relative threshold**, not an absolute-meter threshold.

## Practical thresholds from the comparison work

Treat these as practical guidance based on straight-line separation between:

- the current endpoint
- the historical comparison point used by the check

This is not exactly “route distance since waypoint”, but it is the right proxy for when the approximation starts to drift enough to flip the decision.

### 1% rule

- `0–150 km` since waypoint is OK
- `150–200 km` is borderline
- `>200 km` starts to break down
- `>300 km` is not OK

### 2% rule

- `0–300 km` since waypoint is OK
- `300–500 km` is borderline
- `>500 km` starts to break down
- `>800 km` is not OK

### 3% rule

- `0–500 km` since waypoint is OK
- `500–800 km` is borderline
- `>800 km` starts to break down
- `>1000 km` is not OK

## Decision for this plan

Use a **pure 3% guarded exact fallback** for `weight_check_distance_to_next`.

Reasoning: the project targets phone route generation, older phones are in scope, and waypoint-to-waypoint spans above roughly `800 km` are already considered impractical for the intended use case.

Implementation shape:

1. compute the cheap approximate distances
2. compare the two approximate distances
3. if the difference is within **3%**, switch to exact haversine for the final decision
4. otherwise trust the approximate comparison

Add a code comment explaining why `3%` was chosen.

## Validation sequence

### Before implementation

1. add tests for current behaviour
2. run the new focused test set and save the baseline result

### After Phase 1

1. run the same focused tests
2. confirm navigator call counts changed as intended

### After Phase 2 and 3

1. run the same focused tests again
2. confirm route-query equivalence

### After any approximation change

1. run all previous tests
2. run exact-vs-approx comparison tests
3. document any accepted behaviour delta

### Perf validation

Re-run:

```bash
RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia fast
HOTPATH_ALLOC_SELF=true RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia fast
```

Track at least:

- `weights::weight_check_distance_to_next`
- `weights::weight_no_loops`
- `weights::weight_progress_speed`
- `weights::weight_check_avoid_rules`
- `graph::get_point_from_tiles`
- `tile_manager::get_adjacent_by_id`
- allocation attributed to route-weight logic

---

## Proposed implementation order

1. add behaviour-locking tests
2. add navigator route-once gating for route-only `LastSegmentDoNotUse` weights
3. add the no-allocation route query for Nth junction from end since boundary
4. extend/reuse loop-detector route metadata with `is_junction`
5. preserve the current repeated-point boundary semantics and add a code comment documenting the caveat
6. add the pure `3%` guarded exact fallback for the distance comparison, with a code comment explaining the threshold choice
7. re-run focused tests after each step
8. re-run the perf workflow and compare hotspot numbers