# TODO: Remove `ElementTagValueRef::get()` in `crates/ridi-router-tiles`

## Goal

Remove the misleading ambient helper `ElementTagValueRef::get()` from `crates/ridi-router-tiles/src/map_data/graph.rs`.

## Problem

The method currently looks like a real dereference API but returns placeholder data instead of doing real lookup work.

That makes the API easy to misuse and keeps old `.get()`-style access alive in the tiles crate.

## Plan

- delete `ElementTagValueRef::get()`
- update any callers to stop expecting ambient value lookup from the ref itself
- keep `ElementTagValueRef` as a plain identifier only

## Acceptance criteria

- `ElementTagValueRef::get()` no longer exists
- callers do not depend on tag refs for ambient lookup
- the crate still compiles and tests pass

## Suggested checks

- `rg "ElementTagValueRef::get|\.get\(\)" crates/ridi-router-tiles/src`
- `cargo test -p ridi-router-tiles`
