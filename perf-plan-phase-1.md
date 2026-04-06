# Perf plan phase 1: lock down current behavior with tests

## Goal

Add test coverage around `Route::has_looped` before changing the implementation.

This phase exists to make the later big-bang rewrite safe.

## Why this phase exists

The new loop detector will change the implementation completely. Before doing that, we need tests that describe the current behavior we want to preserve.

The one accepted behavior change is the distance formula:

- current code uses haversine
- new code may use a cheap local planar squared-distance check

So tests should focus on practical threshold behavior, not on reproducing haversine exactly.

## Work items

### 1. Add focused unit tests for `Route::has_looped`

Cover at least these cases:

- exact endpoint revisit returns `true`
- exact endpoint revisit respects `since_point`
- same `hw_ref` and close points returns `true`
- same `name` and close points returns `true`
- same close points with different road identity returns `false`
- if only one side has `hw_ref`, it does not match
- if only one side has `name`, it does not match
- route segments within `LOOP_SEGMENT_THRESHOLD` do not count as a loop
- `since_point = None` behaves like scanning from the start
- `since_point` present once behaves as expected
- `since_point` present multiple times uses the **first** occurrence
- repeated same point before `since_point` is ignored when current behavior says it should be
- close-but-outside-threshold case returns `false`
- comfortably-inside-threshold case returns `true`

### 2. Add tests for route mutation behavior that the new detector must preserve

Cover at least:

- adding segments updates later loop outcomes as expected
- removing the last segment restores prior outcomes
- add / remove / add again keeps behavior stable

### 3. Add a code comment documenting the preserved `since_point` quirk

Near the loop-check resolution logic, document:

- current behavior resolves `since_point` using the first occurrence in route history
- this may not match the original waypoint-transition intent if the same point appears multiple times
- we are preserving this behavior intentionally for compatibility

## Suggested file targets

- `crates/ridi-router-routing/src/router/route/mod.rs`
- existing route tests, or a nearby route test module if there is one

## Deliverable

A test suite that clearly defines the behavior to preserve during the rewrite.

## Acceptance criteria

- all new tests pass on the current implementation
- at least one test explicitly proves first-occurrence `since_point` behavior
- at least one test covers exact-point loops
- at least one test covers fuzzy same-road loops by `hw_ref`
- at least one test covers fuzzy same-road loops by `name`
- the code comment for `since_point` semantics is added or at least planned at the exact target location
