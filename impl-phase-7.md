# Implementation Phase 7: Full verification and performance refresh

## Goal
Confirm the refactor preserved behavior and improved both runtime and allocation profile on the targeted hotspots.

## Why this phase exists
This work is only successful if it is both correct and measurably faster. The plan explicitly targets runtime and cumulative allocations, not just cleaner internals.

## Rollout note
These phases are execution checkpoints inside a single PR, not separate patch submissions.

## In scope
- Run the full relevant test suite
- Re-run the performance measurement flow used for `perf.md`
- Compare before/after hotspot numbers
- Summarize any remaining regressions, tradeoffs, or follow-up work

## Primary metrics to compare
### 1. `walker::move_forward_to_next_fork_with_context`
- total time
- cumulative alloc
- average alloc per call

### 2. `walker::get_fork_segments_for_segment_with_context`
- total time
- cumulative alloc
- average alloc per call

### 3. `weight_heading`
- total time
- evidence that specialized look-ahead is cheaper than full walker traversal

### 4. route-generation wall time
- same scenario/harness referenced by `./perf.md`

## Expected profile shape
Ideally this phase shows:
- lower allocation total in the walker path first
- lower per-call cost in fork classification helpers
- lower `weight_heading` cost
- reduced time in `move_forward_to_next_fork_with_context`

## Tasks
1. Run the full relevant test suite and record results
2. Run the perf/profiling workflow used for the original measurements: `RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia`
3. Capture before/after numbers for the three main hotspots
4. Write a short result summary with wins, neutral changes, and any regressions
5. Identify clearly scoped follow-up work if the profile still shows obvious waste

## Out of scope
- No new optimization branch in this phase unless a blocker prevents measurement
- No roundabout optimization expansion

## Deliverables
- Test results summary
- Updated performance comparison against the baseline from `perf.md`
- Brief recommendation: ship, iterate once more, or split out follow-up work

## Validation
Recommended checks:
- `cargo test -p ridi-router-routing`
- `RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia`

## Exit criteria
- Behavior remains correct under the test suite
- Performance evidence is collected, not assumed
- The team has a clear go/no-go view based on runtime and allocation results

## Final note
If the numbers improve for allocations but not enough for wall time, record that explicitly. The point of this phase is measurement discipline, not optimistic interpretation.
