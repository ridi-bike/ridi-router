# Implementation Phase 5: Add guarded approximate distance comparison and complete perf validation

## Goal
Reduce distance-comparison cost in `weight_check_distance_to_next` while preserving behaviour, except for the explicitly approved guarded approximation rule.

## Why this is phase 5
This is the only planned behaviour-affecting optimization. It should land after the structural refactors so any change in results can be isolated to the approximation logic.

## Main targets
- `crates/ridi-router-routing/src/router/weights.rs`
- test locations added in earlier phases
- perf workflow commands from `perf-plan.md`

## Deliverables
1. Add a cheap approximate point-distance helper for this weight path.
2. Compare approximate distances first.
3. If the relative difference is within 3%, fall back to exact haversine for the final decision.
4. Otherwise trust the approximate comparison.
5. Add a code comment explaining why the 3% guard was chosen.
6. Add focused exact-vs-approx tests around close-call cases.
7. Re-run hotspot and allocation profiling.

## Comparison rule
Planned decision logic:
1. compute approximate distance from current endpoint to `itinerary.next`
2. compute approximate distance from historical comparison point to `itinerary.next`
3. if the difference is within 3%, recompute both exactly with haversine
4. otherwise use the approximate result

## Validation
### Behaviour
- rerun all tests from phases 1-4
- add exact-vs-approx comparison tests
- document any accepted delta explicitly

### Perf
Re-run at least:
```bash
RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia fast
HOTPATH_ALLOC_SELF=true RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia fast
```
Track at minimum:
- `weights::weight_check_distance_to_next`
- `weights::weight_no_loops`
- `weights::weight_progress_speed`
- `weights::weight_check_avoid_rules`
- `graph::get_point_from_tiles`
- `tile_manager::get_adjacent_by_id`
- allocation attributed to route-weight logic

## Exit criteria
- Guarded approximation is covered by tests.
- Any accepted behaviour delta is documented.
- Perf numbers show a clear win on the targeted hotspot and no regression in the focused test suite.

## Risks
- Relative-difference math must be defined carefully for very small values.
- If approximation wins are small after phases 2-4, this phase may need a perf re-check before merge.
