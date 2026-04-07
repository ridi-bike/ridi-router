# Implementation Phase 4: Reuse loop-detector metadata for cheaper junction lookup

## Goal
Avoid repeated `ctx.point(...).is_junction()` calls by extending the existing route metadata instead of creating a second cache.

## Why this is phase 4
Phase 3 removes allocation waste. Phase 4 removes the remaining repeated metadata lookups on the hot path.

## Main targets
- `crates/ridi-router-routing/src/router/route/mod.rs`

## Deliverables
1. Extend the existing per-segment loop metadata with `is_junction`.
2. Expose minimal helpers needed by the route-history query.
3. Make the new no-allocation junction query use metadata instead of repeated `ctx.point(...)` lookups.
4. Keep metadata push/pop symmetry correct when route segments are added or removed.

## Recommended changes
- Add `is_junction: bool` to `LoopMeta`.
- Populate it in `LoopMeta::from_segment(...)`.
- Reuse `point_hits` to resolve the lower-bound route index.
- Keep helper surface area small and route-focused.

## Important compatibility rule
Preserve current repeated-point boundary behaviour for this pass:
- use the same effective boundary as the existing first-match lookup
- add a code comment explaining why this may differ from the conceptual waypoint-switch moment

## Validation
- Run route-query equivalence tests again.
- Run `has_looped` tests to ensure loop-detector behaviour still matches current expectations.
- Run the focused `weight_check_distance_to_next` tests.

## Exit criteria
- Junction counting no longer depends on repeated `ctx.point(...).is_junction()` calls in the hot path.
- Loop-detector tests still pass.
- No duplicate metadata cache was introduced.

## Risks
- Because `LoopDetector` also supports `has_looped`, metadata changes must not break point-hit or cell-index maintenance.
