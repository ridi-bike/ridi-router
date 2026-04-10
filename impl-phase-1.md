# Phase 1 — Borrow walker hot-path data instead of cloning it

## Goal
Cut the easiest allocation churn in the walker without changing routing behavior.

## Why this phase comes first
`RoutingContext` already has borrowed point and adjacency accessors, but the hottest walker classification paths still clone point rules, adjacency, and lines on every call. This is the lowest-risk change in the whole plan.

## Main files
- `crates/ridi-router-routing/src/routing_context.rs`
- `crates/ridi-router-routing/src/router/walker.rs`

## Concrete changes
1. Add `RoutingContext::with_line(...)` plus any small internal helper needed to hydrate line cache once and then borrow it.
2. Refactor `classify_segments_for_point_with_context(...)` to:
   - borrow `center_point.rules` with `with_point(...)`
   - borrow adjacency with `with_adjacent(...)`
   - inspect lines through `with_line(...)`
   - build `Segment` only after a branch survives all filters
3. Refactor `classify_fork_segments_for_segment_with_context(...)` the same way.
4. Keep the current previous-point logic unchanged in this phase. Do not mix in memoization yet.
5. Add or extend focused tests around `RoutingContext` cache behavior and walker classification parity.

## Validation
- `cargo test -p ridi-router-routing`
- Re-run the profiled route:
  - `RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia`
- Compare these hotspots first:
  - `walker::classify_fork_segments_for_segment_with_context`
  - `walker::get_fork_segments_for_segment_with_context`
  - `walker::move_forward_to_next_fork_with_context`

## Exit criteria
- No route-behavior change in existing tests.
- Classification and forward-walk allocation totals move down.
- The diff stays local to context access and walker loops.

## Notes
This phase should stay boring. If a change needs route-history semantics or new memoization keys, defer it to Phase 5.
