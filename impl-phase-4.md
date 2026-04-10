# Phase 4 — Remove roundabout allocation churn

## Goal
Make roundabout exploration cheaper without changing which exits are discovered.

## Why this phase is after the first three
The roundabout code is clearly allocation-heavy, but it is still more behavior-sensitive than the earlier cache and control-flow cleanups. Land the low-risk wins first, then simplify this code in isolation.

## Main files
- `crates/ridi-router-routing/src/router/walker.rs`

## Concrete changes
1. Rewrite `get_roundabout_exits_with_context(...)` to accumulate exits into one flat buffer instead of `Vec<Vec<Segment>>` plus a final flatten.
2. Avoid `SegmentList -> Vec -> Vec<_>` churn inside the loop.
3. Use a small stack-friendly buffer for the common case if it keeps the code readable, for example `SmallVec<[Segment; 4]>`.
4. Apply the same cleanup pattern to `move_to_roundabout_exit_with_context(...)`.
5. Reuse Phase 1 borrowed accessors consistently so roundabout checks do not clone line data while filtering exits.
6. Add regression tests for:
   - simple roundabouts
   - loops that revisit points
   - exits mixed with continuing roundabout segments

## Validation
- `cargo test -p ridi-router-routing`
- Re-run the profiled route:
  - `RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia`
- Compare these hotspots first:
  - `walker::get_roundabout_exits_with_context`
  - `walker::move_forward_to_next_fork_with_context`
  - cumulative allocation totals

## Exit criteria
- Roundabout helper output stays identical on tests.
- Allocation totals for roundabout exploration drop clearly.
- The implementation is simpler than the current nested-collection flow.

## Notes
Do not mix memoization into this phase. Keep it as a local data-shape cleanup.
