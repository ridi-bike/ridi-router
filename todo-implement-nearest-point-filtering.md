# TODO: Implement nearest-point filtering semantics

## Goal

Make `TileManager::get_closest_to_coords(...)` honor the intended filtering behavior.

## Problem

The current implementation still has TODOs for:

- expanding ring search
- rules filtering
- highway tag filtering

So closest-point lookup is structurally present but semantically incomplete.

## Plan

- add a better search strategy than the current simple scan
- apply filtering in a clear order
- honor `RouterRules`
- honor `limit_to_hw_tags`
- add targeted tests for filtering combinations

## Acceptance criteria

- closest-point lookup respects route rules
- closest-point lookup respects optional highway filtering
- lookup behavior is deterministic and tested
- routing tests pass

## Suggested checks

- `cargo test -p ridi-router-routing`
- targeted tests for `get_closest_to_coords(...)`
