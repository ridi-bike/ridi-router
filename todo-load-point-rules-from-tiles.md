# TODO: Load point rules from tiles at runtime

## Goal

Populate `MapDataPoint.rules` from tile-backed data instead of always using an empty vector.

## Problem

`crates/ridi-router-routing/src/map_data/graph.rs` currently constructs tile-backed points with:

- `rules: Vec::new()`

That means real runtime point loading does not yet expose tile-backed rule data.

## Plan

- define or inspect the rule data available in RMDF
- add tile-manager accessors for point rule lookup
- materialize `Vec<MapDataRule>` in `get_point_from_tiles(...)`
- add tests that prove tile-backed points contain rules

## Acceptance criteria

- tile-backed points expose real rules
- runtime rule logic works on tile-backed routing, not just test graphs
- routing tests pass

## Suggested checks

- `cargo test -p ridi-router-routing`
- targeted tests for point-rule loading
