# Phase 3 — Short-circuit per-fork weight evaluation

## Goal
Stop spending time on expensive per-fork weights after a fork is already dead.

## Why this phase stands alone
This is a logic-preserving control-flow cleanup in `navigator.rs`. It does not need cache changes, and it should be easy to validate against existing behavior.

## Main files
- `crates/ridi-router-routing/src/router/navigator.rs`
- `crates/ridi-router-routing/src/router/generator.rs`

## Concrete changes
1. Replace the `map(...).collect::<Vec<_>>()` pattern in per-fork weight evaluation with an explicit loop.
2. Stop immediately when a calc returns:
   - `LastSegmentDoNotUse`
   - `ForkChoiceDoNotUse`
3. Accumulate fork weight totals directly instead of building temporary `Vec<WeightCalcResult>` values.
4. Preserve current weight ordering from `generator.rs`; treat that order as semantic.
5. If it keeps the code simple, separate route-once and per-fork calcs once up front instead of filtering the same list repeatedly.
6. Add tests that prove fork choice and discard behavior stay the same for representative inputs.

## Validation
- `cargo test -p ridi-router-routing`
- Re-run the profiled route:
  - `RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia`
- Compare these hotspots first:
  - `weights::weight_heading`
  - `weights::weight_no_short_detours`
  - total route wall time

## Exit criteria
- Dead forks stop paying for later expensive weight calcs.
- No behavior drift in fork rejection or fork choice order.
- The diff stays contained to navigator control flow.

## Notes
Keep expensive look-ahead weights late in the configured order. That ordering is part of the performance win.
