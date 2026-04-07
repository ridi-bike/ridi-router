# Implementation Phase 5: secondary lookup reductions and hot-call-site cleanup

## Goal
Use post-cache profiling to remove the next layer of avoidable lookup work.

## Why this is phase 5
These changes are useful, but they should be guided by the profile after phases 1-4 land. Some may shrink a lot once the earlier work is done.

## Scope

### In scope
1. Add a per-loaded-tile point index built at tile load time.
2. Use that index in:
   - `get_point_by_id(...)`
   - `get_adjacent_by_id(...)`
3. Add cheap field-level access paths for callers that do not need a full `MapDataPoint`, such as:
   - id-only access
   - coords-only access
   - flag checks
   - cheap junction / degree checks based on `lines_count`
4. Audit hot call sites and replace full point hydration where semantics allow.
5. Add focused micro-benchmarks for:
   - repeated same-point hydration
   - repeated many-point hydration within one tile

### Explicitly out of scope
- external routing API changes unless separately approved
- behavior changes hidden inside perf work

## Expected code touch points
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/routing_context.rs`
- hot callers in generator / navigator / walker / weights as identified by profiling

## Deliverables
- repeated point lookup no longer scans linearly through tile points
- hot call sites that only need cheap metadata stop materializing full points
- micro-benchmarks exist for the core hydration scenarios

## Validation
1. Run workspace checks and relevant tests.
2. Re-run perf and inspect the next hotspot set.
3. Compare call counts and time for:
   - `tile_manager::get_point_by_id`
   - `tile_manager::get_adjacent_by_id`
   - `graph::get_point_from_tiles`
   - hot walker / weight callers affected by call-site cleanup

## Exit criteria
- point lookup is indexed
- obvious full-point-overuse call sites are removed
- benchmark coverage exists for the core hydration paths

## Main risk
This phase can sprawl if the call-site audit is not kept profile-driven. Only change the hottest confirmed sites.

## Rollback plan
Keep the per-tile index independent from the caller audit so each can be landed and evaluated separately.
