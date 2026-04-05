# TODO: Add missing-tile behavior tests

## Goal

Add explicit tests for the case where an adjacent tile is missing.

## Problem

`TileManager` already has a TODO to test the missing-tile scenario. The code tries to treat missing neighbor tiles as dead-ends, but that behavior should be locked in with tests.

## Plan

- create a fixture with a cross-tile edge whose neighbor tile is absent
- verify adjacency lookup degrades safely
- verify route generation does not panic and behaves intentionally

## Acceptance criteria

- missing-tile behavior is covered by tests
- adjacency lookup treats unavailable neighbor tiles safely
- routing tests pass

## Suggested checks

- `cargo test -p ridi-router-routing`
- targeted cross-tile missing-neighbor tests
