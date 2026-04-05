# TODO: Convert `MapDataPoint` helpers in `crates/ridi-router-tiles`

## Goal

Make `MapDataPoint` geometry helpers operate on resolved values instead of refs.

## Problem

In `crates/ridi-router-tiles/src/map_data/point.rs`:

- `distance_between(&MapDataPointRef)` dereferences the ref via `.get()`
- `bearing(&MapDataPointRef)` dereferences the ref via `.get()`

These helpers look pure but hide lookup behavior.

## Plan

- change helpers to accept `&MapDataPoint`
- remove hidden ref dereference from point geometry logic
- update any callers accordingly

## Acceptance criteria

- `distance_between` and `bearing` no longer take `MapDataPointRef`
- no point helper does hidden lookup work
- the crate still compiles and tests pass

## Suggested checks

- `rg "distance_between\(|bearing\(" crates/ridi-router-tiles/src/map_data`
- `cargo test -p ridi-router-tiles`
