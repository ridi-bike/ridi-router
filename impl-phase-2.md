# Implementation Phase 2: Apply route-once weight staging

## Goal
Stop evaluating route-only `LastSegmentDoNotUse` weights once per fork choice. Evaluate them once per fork event instead.

## Why this is phase 2
This is the biggest direct execution-structure win, and it should reduce repeated work even before deeper route-history optimizations land.

## Main targets
- `crates/ridi-router-routing/src/router/weights.rs`
- `crates/ridi-router-routing/src/router/navigator.rs`
- `crates/ridi-router-routing/src/router/generator.rs`

## Route-once weights
These should move into the route-once stage:
- `weight_no_loops`
- `weight_progress_speed`
- `weight_check_distance_to_next`
- `weight_check_avoid_rules`

## Per-choice weights
These should stay per fork choice unless later evidence says otherwise:
- `weight_avoid_nogo_areas`
- `weight_no_sharp_turns`
- `weight_no_short_detours`
- `weight_prefer_same_road`
- `weight_heading`
- `weight_rules_highway`
- `weight_rules_surface`
- `weight_rules_smoothness`

## Deliverables
1. Extend `WeightCalc` with an explicit stage or scope enum.
2. Split navigator weight evaluation into:
   - route-once stage at fork entry
   - per-choice stage for candidate scoring
3. Short-circuit immediately when any route-once weight returns `LastSegmentDoNotUse`.
4. Update generator weight registration to assign the right stage to each weight.
5. Keep existing scoring behaviour for per-choice weights unchanged.

## Recommended shape
Example direction:
- `enum WeightCalcStage { RouteOnce, PerForkChoice }`
- `struct WeightCalc { name, stage, calc }`

Then in `Navigator::generate_routes_with_context`:
1. gather surviving fork choices
2. run all `RouteOnce` weights once against the current route state
3. if any returns `LastSegmentDoNotUse`, prune before per-choice scoring
4. otherwise score each candidate with only `PerForkChoice` weights

## Validation
- Rerun phase 1 navigator execution-count tests.
- Confirm route-once weights are called once per fork event.
- Confirm per-choice weights are still called once per surviving choice.
- Run the focused `weight_check_distance_to_next` tests.

## Exit criteria
- No route-only weight is evaluated once per candidate anymore.
- `LastSegmentDoNotUse` from route-once weights prunes before per-choice work starts.
- Existing route choice behaviour remains unchanged for non-pruned forks.

## Risks
- `WeightCalcInput` currently includes `current_fork_segment` and `walker_from_fork`; route-once weights do not need them, but the input type may stay shared for simplicity.
- This phase changes control flow, so phase 1 tests are mandatory before merging.
