# Phase 5 — Memoize fork classification and roundabout exits by oriented segment

## Goal
Cut repeated walker recomputation at revisited junctions and during backtracking.

## Why this is the last phase
This likely has the biggest upside, but it is the highest-risk behavior change. It depends on the earlier cleanup phases making the hot path simpler and easier to reason about.

## Main files
- `crates/ridi-router-routing/src/router/walker.rs`
- `crates/ridi-router-routing/src/routing_context.rs`

## Concrete changes
1. Introduce an oriented-segment cache key, effectively `line_ref + end_point`.
2. Cache these results by oriented segment:
   - fork classification result
   - roundabout exits result
3. Derive the incoming point from the current oriented segment where possible instead of scanning route history for the previous point.
4. Use the cache from both forward and backward walker paths:
   - `move_forward_to_next_fork_with_context(...)`
   - `move_backwards_to_prev_fork_with_context(...)`
   - roundabout traversal helpers
5. Keep any fallback path needed for edge cases where the oriented segment does not fully determine the old behavior.
6. Add targeted tests for:
   - repeated revisits to the same junction
   - backtracking over the same fork
   - rule-restricted forks
   - roundabouts with multiple exits

## Validation
- `cargo test -p ridi-router-routing`
- Re-run the profiled route:
  - `RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia`
- Compare these hotspots first:
  - `walker::classify_fork_segments_for_segment_with_context`
  - `walker::get_fork_segments_for_segment_with_context`
  - `walker::get_roundabout_exits_with_context`
  - `walker::move_backwards_to_prev_fork_with_context`

## Exit criteria
- Revisited oriented segments reuse cached classification work.
- Backtracking no longer pays the full classification cost again.
- All walker and route tests still pass.

## Notes
This phase is where route-behavior regressions are most likely. Land it only after the earlier phases are green and re-profiled.
