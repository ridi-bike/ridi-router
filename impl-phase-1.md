> Delivery rule: phases 1-5 ship in a single delivery, but stay logically separated so validation can be rerun after each phase.

# Implementation Phase 1: Lock behaviour with focused tests

## Goal
Freeze current behaviour before changing navigator flow or route-history queries.

## Why this is its own phase
Everything after this phase changes execution structure or data access. If behaviour drifts, phase 1 must make it obvious where it happened.

## Main targets
- `crates/ridi-router-routing/src/router/weights.rs`
- `crates/ridi-router-routing/src/router/route/mod.rs`
- `crates/ridi-router-routing/src/router/navigator.rs`

## Deliverables
1. Add focused tests for `weight_check_distance_to_next` in a separate test file or module file, not inside `weights.rs`.
2. Add route-query equivalence tests in a separate test file or module file, not inside `route/mod.rs`.
3. Add navigator execution-count tests in a separate test file or module file, not inside `navigator.rs`.
4. Add a small command list for the focused test suite so later phases can rerun the same checks.

## Required test cases
### A. `weight_check_distance_to_next`
- disabled rule returns `ForkChoiceUseWithWeight(0)`
- empty route returns `ForkChoiceUseWithWeight(0)`
- not enough junctions back returns `ForkChoiceUseWithWeight(0)`
- returns `LastSegmentDoNotUse` when current endpoint is farther from `itinerary.next`
- returns zero when current endpoint is not farther
- respects `switched_wps_on.last().on_point` as the lower boundary
- pins repeated-point boundary behaviour

### B. Route query equivalence
Compare current behaviour against the future helper:
- `split_at_point(boundary).get_junctions_from_end(ctx, n)`
- planned helper using a route-history lower bound

Suggested cases:
- no boundary match
- boundary at start
- boundary in the middle
- not enough junctions
- repeated boundary point

### C. Navigator call-count tests
Prove the current baseline and prepare for the staging refactor:
- route-level weight returning `LastSegmentDoNotUse`
- multiple route-level weights with short-circuiting
- route-level weights plus per-fork weights

## Implementation notes
- Put the new tests in separate files, not in the existing inline test modules.
- Keep the test files close to the router code so they can still exercise internal behaviour cleanly.
- Use counters or shared state inside navigator tests to verify exact call counts.
- Keep one or two tests explicitly documenting the current repeated-point caveat.

## Validation
Run the focused tests and record the command/output used as the behavioural baseline for later phases.

## Exit criteria
- All new tests pass on current code.
- At least one test documents the repeated-point boundary caveat.
- The team has a stable focused test subset to rerun after each later phase.

## Risks
- Building precise route fixtures for repeated-point cases may take some extra test setup work.
