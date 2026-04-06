# Perf plan phase 6: profiling, tuning, and cleanup

## Goal

Measure the effect of the rewrite, tune the remaining hotspots, and clean up any rough edges.

## Work items

### 1. Re-run the same hotpath profile

Check whether:

- `route::has_looped` drops sharply
- `weight_no_loops` drops sharply
- `ctx.point(...)` is no longer a major cost on this path
- `ctx.line(...)`, `ctx.tag_set(...)`, and `ctx.tag_value(...)` are mostly gone from this path

### 2. Inspect candidate-set behavior

If possible, add temporary counters or measurements for:

- exact-point hit vector sizes
- number of nearby cells inspected per query
- number of candidate indices checked per query

This helps tune the spatial bucket size.

### 3. Tune `CellId` granularity if needed

If candidate lists are too large, reduce bucket size.
If too many neighboring buckets need to be inspected, increase it slightly.

### 4. Clean up internal API names and comments

After the implementation settles:

- refine type names
- remove temporary debug code
- keep the compatibility comment for `since_point`
- document the accepted planar-distance approximation

### 5. Decide whether follow-up work is needed

Possible follow-ups after profiling:

- reduce `Route::clone()` cost if it becomes visible
- reuse detector ideas for `is_back_on_road_within_distance`
- revisit `weight_check_distance_to_next`

## Deliverable

A measured result and a cleaned-up implementation.

## Acceptance criteria

- new profile confirms a major reduction in `has_looped` cost
- no new dominant hotspot was introduced by the detector itself
- cell sizing is acceptable or at least documented for follow-up
- code comments reflect the final behavior
