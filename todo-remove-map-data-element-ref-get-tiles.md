# TODO: Remove `MapDataElementRef<T>::get()` in `crates/ridi-router-tiles`

## Goal

Delete `MapDataElementRef<T>::get()` from `crates/ridi-router-tiles/src/map_data/graph.rs`.

## Problem

The method currently panics with `tile-generation refs are not dereferenceable`.

That means the type still advertises an old dereference-style API even though dereferencing is not actually supported.

## Plan

- delete `MapDataElementRef<T>::get()`
- keep only the explicit ID accessors such as `get_tile_id()` and `get_element_id()`
- update any remaining callers to use explicit resolved values instead of ref dereference

## Acceptance criteria

- `MapDataElementRef<T>::get()` no longer exists
- no code in the tiles crate depends on ambient ref dereference
- the crate still compiles and tests pass

## Suggested checks

- `rg "MapDataElementRef<.*>::get|\.get\(\)" crates/ridi-router-tiles/src`
- `cargo test -p ridi-router-tiles`
