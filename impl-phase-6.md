# Implementation Phase 6: Switch `weight_heading` to the new helper

## Goal
Replace `weight_heading`'s full walker-based look-ahead with the specialized helper from Phase 5, while keeping scoring results stable.

## Why this phase exists
Adding the helper is not enough. The actual scoring hotspot only improves once `crates/ridi-router-routing/src/router/weights.rs` stops constructing a fresh `Walker` for heading probes.

## In scope
- Refactor `weight_heading` to call the specialized helper
- Preserve current non-roundabout scoring behavior
- Keep conservative fallback behavior wherever the new helper intentionally stays narrow
- Remove only the now-unnecessary walker usage from this path

## Required preservation rule
From the caller's point of view, heading scoring inputs and outcomes must remain the same for the covered non-roundabout cases.

That includes:
- finish-equivalent look-ahead meaning
- dead-end-equivalent look-ahead meaning
- immediate-fork behavior
- corridor approach-segment behavior used for heading estimation

## Tasks
1. Swap `weight_heading` to the new helper
2. Keep the mapping from helper result to scoring behavior explicit and readable
3. Preserve current fallback behavior for out-of-scope roundabout cases
4. Re-run the heading regression tests from Phase 1
5. Remove or simplify any now-unused local walker setup in this scoring path

## Out of scope
- No scoring philosophy changes
- No route selection behavior changes
- No roundabout redesign

## Deliverables
- Updated `weights.rs` using the specialized helper
- Stable heading behavior under regression tests
- Simpler and cheaper heading look-ahead path

## Validation
Recommended checks:
- `cargo test -p ridi-router-routing weights`
- `cargo test -p ridi-router-routing`

## Exit criteria
- `weight_heading` no longer depends on full walker traversal for the targeted cases
- Heading regression tests pass unchanged
- The code clearly shows the new narrow dependency on the helper

## Handoff to next phase
Phase 7 should perform full-project verification and refresh the performance measurements so the change can be evaluated on both runtime and allocations.
