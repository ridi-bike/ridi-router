# Implementation Phase 3: Replace route slicing with a no-allocation route query

## Goal
Remove the `split_at_point(...).get_junctions_from_end(...)` path from `weight_check_distance_to_next`.

## Why this is phase 3
After route-once staging, the next clear waste is route slicing, vector cloning, and duplicate reverse scans.

## Main targets
- `crates/ridi-router-routing/src/router/route/mod.rs`
- `crates/ridi-router-routing/src/router/weights.rs`

## Deliverables
1. Add a route-history helper that scans the existing route without cloning.
2. Make the helper work from a lower-bound route index, not from a cloned suffix.
3. Update `weight_check_distance_to_next` to use the new helper.
4. Preserve current repeated-point boundary semantics.

## Recommended API direction
Prefer an index-based helper, for example:
- `Route::nth_junction_from_end_since_idx(...) -> Option<&Segment>`

Possible support helpers if needed:
- `route_index_first_for_point(point_ref)`
- `route_index_last_for_point(point_ref)`
- `segment_meta(idx)`

## Behaviour requirements
Given `since_idx` and `num_of_junctions`, the helper should:
1. scan `route_segments` from the end toward `since_idx`
2. count junction endpoints
3. return the segment at the Nth junction from the end
4. allocate nothing
5. preserve the current effective boundary rule used by `split_at_point(...).position(...)`

## Implementation notes
- Do not introduce a separate route cache in this phase.
- Keep the old helpers around until the new tests pass, then stop using them from `weight_check_distance_to_next`.
- Add a code comment where repeated-point semantics are preserved intentionally.

## Validation
- Run the route-query equivalence tests from phase 1.
- Run the focused `weight_check_distance_to_next` tests.
- Confirm that no new allocation path was added in the helper implementation.

## Exit criteria
- `weight_check_distance_to_next` no longer calls `split_at_point` for this check.
- The new helper matches current behaviour on all equivalence cases.
- The repeated-point caveat is documented in code.

## Risks
- Index semantics are easy to shift by one. The equivalence tests must cover start, middle, and repeated-point boundaries.
