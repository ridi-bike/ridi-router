# TODO: Remove `ElementTagSetRef::get()` in `crates/ridi-router-tiles`

## Goal

Remove `ElementTagSetRef::get()` from `crates/ridi-router-tiles/src/map_data/graph.rs`.

## Problem

The method currently returns a fake empty tag set. That is worse than no API because it suggests the ref can resolve itself correctly.

## Plan

- delete `ElementTagSetRef::get()`
- move any needed tag-set materialization to explicit owner-side logic
- keep `ElementTagSetRef` as a lightweight identifier

## Acceptance criteria

- `ElementTagSetRef::get()` no longer exists
- no caller expects tag-set refs to resolve themselves
- the crate still compiles and tests pass

## Suggested checks

- `rg "ElementTagSetRef::get|\.get\(\)" crates/ridi-router-tiles/src`
- `cargo test -p ridi-router-tiles`
