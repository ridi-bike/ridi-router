# Perf plan phase 5: integration validation and regression checks

## Goal

Validate that the new detector works correctly in the full router flow and does not break route mutation or navigation behavior.

## Work items

### 1. Run the existing test suite

At minimum, run tests covering:

- route behavior
- walker backtracking
- navigation with forks
- generator-level route creation if present

### 2. Add focused integration checks where useful

Especially validate that:

- forward walking plus backtracking still produces stable route behavior
- loop detection does not get stuck in stale state after pops
- cloned routes still behave correctly if they are used later

### 3. Sanity-check route clone cost

We already accepted detector state living in `Route`, but verify this did not introduce an obviously bad correctness or memory issue.

### 4. Confirm no accidental semantic drift beyond the distance formula change

Specifically watch for regressions around:

- `since_point`
- `hw_ref`-only matches
- `name`-only matches
- exact endpoint loops

## Suggested commands

Use the normal Rust test commands for the crate or workspace.

## Deliverable

Confidence that the big-bang rewrite is correct beyond isolated unit tests.

## Acceptance criteria

- relevant tests pass
- no stale-index bug appears during backtracking flows
- no obvious clone-related correctness issue appears
- integration behavior matches pre-rewrite expectations
