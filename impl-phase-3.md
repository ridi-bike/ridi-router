# Implementation Phase 3: Build the shared non-roundabout next-step classifier

## Goal
Create one internal classification layer that computes legal next-step outcomes for non-roundabout traversal and becomes the single source of truth for both the main walker and heading look-ahead.

## Why this phase exists
The current path repeatedly builds temporary collections even for the common 0/1-choice cases. The refactor needs a lower-level primitive that can cheaply represent:
- no legal continuation
- exactly one legal continuation
- multiple legal continuations

## Proposed shape
The exact names can change, but the intended model is:

```rust
enum NextStepClass {
    None,
    One(Segment),
    Many(SmallVec<[Segment; 4]>),
}
```

The important property is not the exact type. It is that 0/1-choice cases avoid full `SegmentList` construction.

## Responsibilities
The classifier must handle the non-roundabout rules now enforced through existing helper logic:
- start-point segment discovery
- continuation discovery from an incoming segment
- reverse-edge exclusion
- one-way filtering
- `OnlyAllowed` rule filtering
- `NotAllowed` rule filtering
- enough information to support explicit fork-choice validation

## Design rules
- This classifier is the single source of truth for legal next-step computation in this refactor
- The main walker and heading helper must both use it
- Roundabout logic remains separate for this pass
- Use Phase 2 borrowed `RoutingContext` accessors where possible

## Suggested internal split
A practical implementation can separate:
1. classification from a start point
2. classification from an incoming segment
3. conversion helpers for the rare paths that still need a collection payload
4. wrong-choice support helpers that can build `available_fork_ids` only when needed

## Tasks
1. Introduce the internal enum/result type
2. Implement shared non-roundabout classification helpers
3. Replace duplicated filtering logic inside existing internal helpers where safe
4. Keep existing external behavior untouched until Phase 4 swaps the main walker over
5. Add focused tests if useful for classifier-only edge cases

## Out of scope
- No main walker fast-path swap yet
- No `weight_heading` integration yet
- No roundabout optimization

## Deliverables
- Shared classifier internals in the router traversal code
- Clear internal naming around 0/1/many outcomes
- Minimal allocation on the common path by design

## Validation
Recommended checks:
- `cargo test -p ridi-router-routing`
- Targeted reruns of walker rule-filtering tests from Phase 1

## Exit criteria
- The classifier can express all required non-roundabout outcomes
- Legal-step semantics still match current behavior under existing tests
- Later phases can replace ad hoc next-step discovery with this classifier directly

## Handoff to next phase
Phase 4 should use this classifier inside `move_forward_to_next_fork_with_context` so the common corridor path becomes cheap.
