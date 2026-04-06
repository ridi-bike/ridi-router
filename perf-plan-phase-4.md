# Perf plan phase 4: rewrite `Route::has_looped` to use indexed candidates

## Goal

Replace the old rescan-based `Route::has_looped` implementation with the new detector-backed query.

This is the core performance change.

## Work items

### 1. Resolve `since_point` to `since_idx` from cached route state

Keep `since_point` as the public input.

Resolve it internally using `point_hits`:

- `None` => `since_idx = 0`
- point found => use the **first** recorded index
- point missing => `since_idx = 0`

Add the compatibility comment documenting that this preserves current behavior even though it may not perfectly match original waypoint intent.

### 2. Implement exact-point revisit check

For the newest segment:

- look up `point_hits[current.end_point]`
- search for a prior index that is:
  - `< current_idx`
  - `>= since_idx`

If found, return `true`.

### 3. Implement same-road nearby-candidate lookup

For each present road identity on the current segment:

- `hw_ref`
- `name`

look up nearby cells and inspect candidate indices only from those buckets.

### 4. Preserve current road matching semantics exactly

The rewrite must preserve:

- same `hw_ref` counts as a match if both sides are `Some`
- same `name` counts as a match if both sides are `Some`
- overall logic is `hw_ref match OR name match`

Do not collapse this into one simplified road identity rule.

### 5. Replace haversine with cheap local squared-distance calculation

Use cached coordinates from `LoopMeta`.

Suggested flow:

- compute local x/y deltas in meters
- compare squared distance to squared threshold
- do not call haversine
- do not call `sqrt`

This is an intentional small approximation change that we already accepted.

### 6. Preserve the segment-gap rule

Keep the equivalent of:

```rust
current_idx - candidate_idx > LOOP_SEGMENT_THRESHOLD
```

This must behave the same as the current scan logic.

### 7. Make the hot path free of repeated heavy hydration

The final query path should avoid repeated calls to:

- `ctx.point(...)`
- `ctx.line(...)`
- `ctx.tag_set(...)`
- `ctx.tag_value(...)`

Any remaining context reads in `has_looped` should be carefully justified.

## Suggested file targets

- `crates/ridi-router-routing/src/router/route/mod.rs`

## Deliverable

`Route::has_looped` becomes an indexed lookup over incremental detector state.

## Acceptance criteria

- old behavior tests pass, except for the accepted distance-calculation simplification where tests define the expected practical result
- `has_looped` no longer rescans route history broadly
- query path uses cached metadata and indexes
- public API still accepts `since_point`
