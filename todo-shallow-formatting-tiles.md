# TODO: Make formatting shallow in `crates/ridi-router-tiles`

## Goal

Keep `Debug` / `Display` implementations in the tiles crate shallow and non-resolving.

## Problem

Current formatting in `point.rs` and `line.rs` dereferences refs to print derived information.

That makes formatting depend on hidden lookup behavior and preserves old ambient assumptions.

## Plan

- rewrite `Debug` implementations to print structural data only
- avoid resolving endpoint refs, line IDs, or tag values during formatting
- keep `Display` / `Debug` cheap and predictable

## Acceptance criteria

- formatting code does not call ref `.get()` helpers
- `Debug` / `Display` are shallow and structural
- the crate still compiles and tests pass

## Suggested checks

- `rg "impl (Debug|Display)" crates/ridi-router-tiles/src/map_data`
- `rg "\.get\(\)" crates/ridi-router-tiles/src/map_data`
- `cargo test -p ridi-router-tiles`
