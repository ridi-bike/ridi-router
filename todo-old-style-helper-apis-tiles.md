# TODO: Old-style helper APIs in `crates/ridi-router-tiles`

## Goal

Remove the remaining ambient/placeholder helper APIs in `crates/ridi-router-tiles` so the tile crate does not preserve the old singleton-era access pattern in disguised form.

This is mainly a **consistency and cleanup** task. The routing refactor is already done in `crates/ridi-router-routing`, but the tiles crate still exposes helper shapes that look like real dereference APIs even though they are placeholder or panic-based.

## Current issues

### Files

- `crates/ridi-router-tiles/src/map_data/graph.rs`
- `crates/ridi-router-tiles/src/map_data/point.rs`
- `crates/ridi-router-tiles/src/map_data/line.rs`

### Problematic APIs

In `crates/ridi-router-tiles/src/map_data/graph.rs`:

- `ElementTagValueRef::get()` returns `None`
- `ElementTagSetRef::get()` returns an all-`none` tag set
- `MapDataElementRef<T>::get()` panics with `tile-generation refs are not dereferenceable`
- `ElementTagSet::{name, hw_ref, highway, surface, smoothness}()` are built on top of the placeholder `.get()` methods

In `crates/ridi-router-tiles/src/map_data/point.rs`:

- `MapDataPoint::distance_between(&MapDataPointRef)` uses `point.get()`
- `MapDataPoint::bearing(&MapDataPointRef)` uses `point.get()`
- `Debug` formatting resolves line refs via `l.get().line_id()`

In `crates/ridi-router-tiles/src/map_data/line.rs`:

- `MapDataLine::line_id()` uses `self.points.0.get()` / `self.points.1.get()`
- `MapDataLine::get_len_m()` uses `self.points.0.get().distance_between(...)`
- `Debug` formatting dereferences both endpoint refs

## Why this should be fixed

These APIs are misleading in two ways:

1. they look like real graph-backed dereference helpers
2. they either panic or return placeholder data

That makes the tiles crate easy to misuse and makes the repo inconsistent with the routing crate's explicit-access direction.

## Recommended direction

### 1. Keep refs as IDs only

`MapDataElementRef<T>`, `ElementTagValueRef`, and `ElementTagSetRef` should remain lightweight identifiers.

They should **not** offer ambient `.get()` methods.

Preferred end state:

- delete `ElementTagValueRef::get()`
- delete `ElementTagSetRef::get()`
- delete `MapDataElementRef<T>::get()`

### 2. Make pure helpers take resolved values

In `point.rs` and `line.rs`, pure helpers should operate on already-resolved values.

Preferred replacements:

- `MapDataPoint::distance_between(&MapDataPoint)`
- `MapDataPoint::bearing(&MapDataPoint)`
- `MapDataLine::len_m(&MapDataPoint, &MapDataPoint)`

That matches the shape already used in `ridi-router-routing`.

### 3. Remove fake convenience tag accessors

`ElementTagSet::{name, hw_ref, highway, surface, smoothness}()` should not remain if they only call placeholder `.get()` behavior.

Two acceptable outcomes:

- delete them entirely
- replace them with helpers that work on concrete tag storage owned by the caller

But do not keep APIs that silently return fake values.

### 4. Make formatting shallow

`Debug` / `Display` should not try to resolve refs.

Preferred behavior:

- keep `Display`/`Debug` structural and shallow
- show IDs / counts / direction
- do not derive line IDs or tag values through ref resolution

## Suggested implementation steps

1. Remove ambient `.get()` methods from ref/tag ref types
2. Update `MapDataPoint` geometry helpers to take resolved values
3. Update `MapDataLine` helpers to take explicit resolved endpoints
4. Rewrite `Debug` implementations to avoid dereferencing
5. Remove dead helper methods that only existed to support `.get()`-style usage
6. Run repo-wide searches for lingering `point.get()`, `line.get()`, `tag.get()` usage in `crates/ridi-router-tiles`

## Acceptance criteria

- no `MapDataElementRef<T>::get()` remains in `crates/ridi-router-tiles`
- no `ElementTagValueRef::get()` remains in `crates/ridi-router-tiles`
- no `ElementTagSetRef::get()` remains in `crates/ridi-router-tiles`
- no point/line helper in the tiles crate performs hidden ref dereference
- `Debug` / `Display` in the tiles crate are shallow and non-resolving
- the crate still compiles and tests pass

## Suggested checks

- `rg "\.get\(\)" crates/ridi-router-tiles/src/map_data`
- `cargo test -p ridi-router-tiles`
- optional: `cargo test --workspace`

## Non-goals

- changing routing behavior
- adding runtime graph/context types to the tiles crate
- redesigning RMDF format semantics in this task
