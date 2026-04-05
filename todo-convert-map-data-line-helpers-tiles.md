# TODO: Convert `MapDataLine` helpers in `crates/ridi-router-tiles`

## Goal

Make `MapDataLine` helpers operate on explicit inputs instead of ref dereference.

## Problem

In `crates/ridi-router-tiles/src/map_data/line.rs`:

- `line_id()` dereferences endpoint refs
- `get_len_m()` dereferences endpoint refs

These helpers hide lookup work and preserve the old API style.

## Plan

- redesign helpers to take resolved endpoint values where needed
- rename `get_len_m()` to a non-ambient shape if appropriate
- remove hidden endpoint dereference from line helpers

## Acceptance criteria

- line helpers no longer depend on endpoint `.get()` calls
- no line helper performs hidden lookup work
- the crate still compiles and tests pass

## Suggested checks

- `rg "line_id\(|get_len_m\(|\.get\(\)" crates/ridi-router-tiles/src/map_data`
- `cargo test -p ridi-router-tiles`
