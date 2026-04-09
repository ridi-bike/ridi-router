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


## Execution results

> Run date: 2026-04-09

> Baseline for comparison: `./perf.md` saved report dated 2026-04-08

> Commands run:

> - `cargo test -p ridi-router-routing`

> - `RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia`

> Notes: test output included existing compiler warnings in unrelated crates. No new code changes were made in this phase; this phase only collected verification data.

### Test results

- `cargo test -p ridi-router-routing`: **pass**
- Result: **142 passed, 0 failed, 2 ignored**
- Relevant new walker, routing-context, and heading-look-ahead regression tests stayed green.

### Perf run summary

- `Routes from itineraries`: **62 s**
- `Route generation finished`: **68 s**
- Route files written successfully to `map-data/routes`
- Hotpath cumulative allocation total: **2.9 GB**

### Target hotspot comparison vs baseline from `perf.md`

| Metric | Baseline (`perf.md`) | Current run | Change |
|---|---:|---:|---:|
| Route generation wall time | 46.77 s | 68 s | +45% |
| Total cumulative allocation | 4.1 GB | 2.9 GB | -29% |
| `walker::move_forward_to_next_fork_with_context` total time | 13.70 s | 7.95 s | -42% |
| `walker::move_forward_to_next_fork_with_context` cumulative alloc | 940.0 MB | 339.3 MB | -64% |
| `walker::move_forward_to_next_fork_with_context` avg time/call | 38.3 µs | 67.27 µs | +76% |
| `walker::move_forward_to_next_fork_with_context` avg alloc/call | 2.7 KB | 2.9 KB | +7% |
| `walker::get_fork_segments_for_segment_with_context` total time | 6.23 s | 5.21 s | -16% |
| `walker::get_fork_segments_for_segment_with_context` cumulative alloc | 598.4 MB | 190.0 MB | -68% |
| `walker::get_fork_segments_for_segment_with_context` avg time/call | 1.62 µs | 3.22 µs | +99% |
| `walker::get_fork_segments_for_segment_with_context` avg alloc/call | 156 B | 123 B | -21% |
| `weights::weight_heading` total time | 2.86 s | 7.94 s | +178% |
| `weights::weight_heading` cumulative alloc | 179.9 MB | 251.9 MB | +40% |
| `weights::weight_heading` avg time/call | 20.27 µs | 82.55 µs | +307% |
| `weights::weight_heading` avg alloc/call | 1.3 KB | 2.7 KB | +112% |

### Additional observations

- The specialized helper is visible in the profile as `walker::heading_look_ahead_from_segment_with_context` at **7.58 s** and **244.7 MB**.
- That helper is currently not cheaper than the previous full-walker-based heading path at the route level. The `weight_heading` hotspot materially regressed.
- New classifier functions now dominate a meaningful part of traversal time:
  - `walker::classify_continuation_segments_with_context`: **9.70 s**, **387.0 MB**
  - `walker::classify_fork_segments_for_segment_with_context`: **4.82 s**, **142.3 MB**
- Roundabout traversal remains expensive and still sits near the top of the allocation table, but roundabout work was explicitly out of scope for this pass.

### Result summary

**Wins**

- Behavior checks passed in the routing test suite.
- Cumulative allocation improved in the measured route run.
- Total time in `walker::move_forward_to_next_fork_with_context` improved substantially.
- Total time and allocation in `walker::get_fork_segments_for_segment_with_context` also improved.

**Regressions**

- End-to-end route generation wall time got worse.
- `weights::weight_heading` regressed heavily in both total time and cumulative allocation.
- Per-call cost for the two walker hotspots increased even though total cost dropped because the run executed fewer calls.

**Neutral / needs interpretation**

- Call counts changed a lot relative to the saved baseline, so per-call comparisons and route-level timing should be interpreted together, not in isolation.
- This run produced **80** routes, while the saved baseline report recorded **79** routes. The test suite stayed green, but the route-count change should be treated as something to double-check when later optimization phases are reviewed.

### Recommendation

**Iterate once more before shipping.**

This phase met the verification goal for correctness under tests, but it did **not** meet the perf goal cleanly. Allocation totals improved, yet wall time regressed and `weight_heading` became much more expensive. The most clearly scoped follow-up work is to revisit the heading look-ahead path and the new classification-layer overhead in the later optimization phases, not in this phase.