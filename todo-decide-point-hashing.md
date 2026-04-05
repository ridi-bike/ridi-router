# TODO: Decide whether point hashing stays or is removed

## Goal

Make an explicit decision about the placeholder `generate_point_hashes()` path.

## Problem

Generation code still contains a point-hashing placeholder even though comments suggest it may be unnecessary if grid-cell indexing is the real approach.

## Plan

Choose one direction:

- delete the dead placeholder if grid-cell indexing is sufficient
- or implement point hashing with a real consumer and tests

Do not leave it ambiguous.

## Acceptance criteria

- the placeholder is either removed or implemented
- the chosen spatial-index story is clear in code
- tiles tests pass

## Suggested checks

- `cargo test -p ridi-router-tiles`
- search for remaining dead placeholder references
