# Implementation Phase 4: Refactor the main walker onto the classifier fast path

## Goal
Make `walker::move_forward_to_next_fork_with_context` cheap on the common 0/1-choice path while preserving current semantics.

## Why this phase exists
This is the main hotspot. The biggest direct win should come from stopping the walker from allocating and rescanning temporary collections during corridor traversal.

## In scope
- Refactor `crates/ridi-router-routing/src/router/walker.rs`
- Route non-roundabout next-step decisions through the shared classifier from Phase 3
- Preserve roundabout behavior by keeping roundabout-specific logic intact
- Preserve explicit wrong-choice behavior and payload shape

## Required target behavior
The main loop should:
- return `Finish` immediately on finish
- return `DeadEnd` immediately on no legal next step
- advance immediately on exactly one legal next step
- return `Fork(...)` only when there are truly multiple legal choices and no explicit choice is set
- validate explicit choices without paying unnecessary allocation cost on the success path

## Fast-path rules
The common corridor case should avoid:
- building `SegmentList`
- collecting all choices just to use one
- rescanning temporary collections multiple times

## Error-path policy
For `WalkerError::WrongForkChoice`, it is acceptable to do extra work to build `available_fork_ids`.
That payload matters for correctness, but it is not the hot path.

## Strict preservation requirements
This phase must preserve:
- fork timing from the caller's point of view
- finish behavior
- dead-end behavior
- junction-loop behavior in non-roundabout paths
- wrong-choice error meaning and payload shape
- route mutation/history results for successful traversal

## Tasks
1. Swap the walk loop to use the shared classifier
2. Keep explicit-choice handling behaviorally identical
3. Build multi-choice collections only on the paths that really need them
4. Keep roundabout delegation conservative and unchanged
5. Re-run the full walker regression set after each meaningful step

## Out of scope
- No new heading helper usage yet
- No roundabout redesign
- No broad cleanup unrelated to the hotspot

## Deliverables
- Refactored `move_forward_to_next_fork_with_context`
- Same public outcomes with lower allocation pressure on corridor traversal
- Clean handling of wrong-choice errors on the slow path

## Validation
Recommended checks:
- `cargo test -p ridi-router-routing walker`
- `cargo test -p ridi-router-routing`

## Exit criteria
- All walker regression tests from Phase 1 are green
- The walker now uses the classifier on the non-roundabout fast path
- No observable semantic drift is introduced

## Handoff to next phase
With the main walker moved over, Phase 5 can add a narrow heading-specific look-ahead helper on top of the same classifier.
