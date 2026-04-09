# Implementation Phase 5: Add the specialized heading look-ahead helper

## Goal
Introduce a narrow helper for heading scoring that reuses the same non-roundabout next-step classifier as the main walker, but does not pay full walker costs.

## Why this phase exists
`weight_heading` currently constructs a fresh `Walker` and runs full traversal logic to answer a much smaller question. This phase extracts only the information heading scoring actually needs.

## Required behavior
Given a candidate fork segment, the helper should follow shared non-roundabout next-step logic until it reaches one of:
- `Finish`
- `DeadEnd`
- the next decision point

## Suggested return shape
Exact names may differ, but the intent is:

```rust
enum HeadingLookAheadResult {
    DeadEnd,
    Finish,
    Decision {
        approach_segment: Option<Segment>,
    },
}
```

The helper should return only what heading scoring needs.

## Must-use design rule
This helper must use the same low-level next-step classification as the main walker.
No duplicated rule logic.
No alternate interpretation of legal continuations.

## What this helper must not do
- build a full `Route`
- mutate a full `Walker`
- update full loop-detector route history
- become a general speculative traversal API

## Roundabout scope limit
This helper is specialized for non-roundabout look-ahead in this pass.
If roundabout behavior is encountered, preserve current behavior conservatively rather than inventing new semantics here.

## Tasks
1. Add the new helper and result type
2. Drive traversal via the shared classifier
3. Return the approach segment needed by heading scoring
4. Add the heading regression tests planned in Phase 1
5. Keep the implementation narrow and internal

## Out of scope
- No `weight_heading` swap yet
- No general speculative traversal framework
- No roundabout optimization

## Deliverables
- Specialized heading look-ahead helper
- Regression tests covering finish, dead-end, immediate decision, and corridor approach-segment cases
- Conservative behavior at roundabout boundaries

## Validation
Recommended checks:
- `cargo test -p ridi-router-routing weights`
- `cargo test -p ridi-router-routing`

## Exit criteria
- Helper behavior is covered by regression tests
- The helper shares the classifier rather than duplicating logic
- The helper returns only the data `weight_heading` needs

## Handoff to next phase
Phase 6 should switch `weight_heading` to this helper and confirm scoring behavior remains unchanged.
