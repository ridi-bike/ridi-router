# TODO: Write real point-record offsets in RMDF generation

## Goal

Replace placeholder point-record offsets with real offsets in RMDF writer output.

## Problem

`crates/ridi-router-tiles/src/rmdf/generator/writer.rs` still writes placeholder values for:

- `lines_offset`
- `rules_offset`

That should become real serialized offsets.

## Plan

- keep point ordering consistent with side-table ordering
- calculate line-ref offsets from the flattened line-ref table
- calculate rules offsets from the flattened rules table
- validate offset/count round-tripping

## Acceptance criteria

- point records no longer use placeholder offsets where real data exists
- generated RMDF points reference valid serialized sections
- tiles tests pass

## Suggested checks

- `cargo test -p ridi-router-tiles`
- fixture-based RMDF round-trip tests
