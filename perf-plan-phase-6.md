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


## Measured result

> Profile command used:

> ```bash
> RIDI_FEATURES='hotpath,hotpath-alloc,hotpath-mcp' ./dev.sh route riga,latvia sigulda,latvia
> HOTPATH_ALLOC_SELF=true RIDI_FEATURES='hotpath,hotpath-alloc,hotpath-mcp' ./dev.sh route riga,latvia sigulda,latvia
> ```

> Compared with the earlier baseline in `perf.md`.

- `route::has_looped`: from about `41.76s` inclusive to about `0.54ms` total (`0.49ms` in the exclusive-allocation run's timing table).
- `weights::weight_no_loops`: from about `41.76s` inclusive to about `0.62ms` total (`0.57ms` in the exclusive-allocation run's timing table).
- `graph::get_point_from_tiles`, `graph::get_line_from_tiles`, `graph::get_tag_set`, and `graph::get_tag_value` no longer show up as meaningful costs on the loop-detection path.
- `route::has_looped` shows `0 B` exclusive allocation in the allocation-focused pass.

## Tuning decision

The current `CellId` granularity is acceptable for now.

Reason:

- the detector is no longer a visible hotspot
- no extra detector-specific hotspot replaced it
- the remaining dominant work is elsewhere (`tile_manager::get_adjacent_by_id`, `tile_manager::get_point_by_id`, `tile_manager::get_rules_for_point`, and `weights::weight_check_distance_to_next` allocations)

So this phase keeps the current bucket size and only documents the choice in code comments.

## Follow-up left for later phases

- investigate `weight_check_distance_to_next`, now the clearest remaining algorithmic/allocation issue
- consider reusing detector-style incremental state for `is_back_on_road_within_distance` if it becomes worth the complexity
- revisit `Route::clone()` only if later profiles show detector-state cloning as measurable
