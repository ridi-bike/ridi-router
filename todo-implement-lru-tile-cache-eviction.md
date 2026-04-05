# TODO: Implement real LRU tile-cache eviction

## Goal

Replace arbitrary tile eviction with true least-recently-used eviction in `TileManager`.

## Problem

`evict_if_needed()` currently removes an arbitrary tile, not the least-recently-used tile.

That makes cache behavior less predictable on larger datasets.

## Plan

- add access tracking to loaded tile state
- update access metadata on load and use
- evict the true LRU tile once over the limit
- add overflow/eviction tests

## Acceptance criteria

- eviction order is deterministic
- hot tiles are kept under repeated access
- tile-manager tests cover an eviction case
- routing tests still pass

## Suggested checks

- `cargo test -p ridi-router-routing`
- targeted tile-cache tests
