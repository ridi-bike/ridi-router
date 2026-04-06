# Perf plan phase 3: metadata hydration and incremental push/pop maintenance

## Goal

Build `LoopMeta` once per appended segment and maintain all detector indexes incrementally as the route grows and shrinks.

This removes the need for repeated `ctx.*` hydration inside future loop queries.

## Work items

### 1. Add a one-time metadata builder

Create a helper that, given:

- `ctx`
- a `Segment`

produces `LoopMeta` by hydrating only once.

It should:

- get the endpoint point once
- get the line once
- get the tag set once
- resolve `hw_ref` once
- resolve `name` once
- convert road strings into `RoadKey`
- compute the spatial cell id

### 2. Update route mutation APIs to maintain detector state

`Route::add_segment(...)` should:

- append the segment
- build `LoopMeta`
- append to `metas`
- insert the new index into `point_hits`
- insert the new index into the relevant road-cell maps

`Route::remove_last_segment(...)` should:

- pop the segment
- pop the matching `LoopMeta`
- remove the last index from `point_hits`
- remove the last index from the relevant road-cell maps
- optionally clean up empty entries

### 3. Keep index maintenance strictly reversible

Backtracking correctness matters.

Make sure the maintenance logic assumes:

- pushes happen at the end
- pops remove the most recent segment only

That means vectors of indices should behave like stacks.

### 4. Add direct tests for detector maintenance

Cover:

- push one segment
- push many segments
- pop one segment
- pop many segments
- push/pop symmetry
- repeated point insertion
- repeated road-key insertion across multiple cells

## Suggested file targets

- `crates/ridi-router-routing/src/router/route/mod.rs`
- any helper module created for loop detection internals

## Deliverable

The detector indexes stay in sync with route mutation.

## Acceptance criteria

- detector metadata is built once on append
- detector metadata is removed cleanly on pop
- push/pop tests pass
- no rescan is needed to answer future exact-point or same-road candidate lookups
