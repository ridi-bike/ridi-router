# Todos

This file is the source of truth for open follow-up work in this repo.

Last audited: 2026-04-06
Validation snapshot:
- `cargo test --workspace -q`
  - `ridi-router-common`: pass
  - `ridi-router-routing`: pass
  - `ridi-router-tiles`: pass
  - `ridi-router-cli`: `generate_route_cli` still has 4 failing tests in this checkout

## P0 — Stabilize CLI route fixture tests

- Replace the repo-scoped `map-data/output` dependency in `crates/ridi-router-cli/tests/generate_route_cli.rs`.
  - Current state: these tests only check whether `map-data/output/manifest.json` exists, but that is not enough to make them deterministic.
  - In this checkout, `cargo test --workspace -q` fails in 4 `generate_route_cli` tests with `Routing error: Could not find start point on map` for the hard-coded Latvia coordinates used by the tests.
  - The problem is that `map-data/output` is treated as an implicit shared fixture, but its contents are not pinned to the test assumptions.
  - Prefer either:
    - checked-in deterministic RMDF fixture data that matches the test coordinates, or
    - synthetic per-test fixture generation like the existing synthetic JSON/GPX route tests.
  - Avoid tests depending on whatever local dataset last populated `map-data/output`.

## P1 — Routing robustness

- Harden tile-backed lookup failures for library use.
  - `crates/ridi-router-routing/src/map_data/graph.rs` still uses `expect(...)` / `unwrap(...)` for tile-backed point, line, adjacency, and rule lookups.
  - `crates/ridi-router-routing/src/rmdf/tile_manager.rs` still has internal `unwrap()` assumptions after `ensure_tile_loaded(...)`.
  - Corrupt tiles, invalid refs, or wrong-context misuse can still panic instead of returning structured errors.

- Add guardrails against cross-graph ref misuse.
  - `MapDataPointRef`, `MapDataLineRef`, and tag refs only carry `tile_id` + `element_id`.
  - They are not tied to a specific `MapDataGraph` / `RoutingContext`, so internal callers can still resolve refs through the wrong graph instance.
  - Decide whether to use graph identity checks, debug assertions, or fallible resolution APIs.

## P1 — Restriction coverage

- Extend generation support beyond the currently supported node-based restriction subset.
  - `crates/ridi-router-tiles/src/generation/graph.rs` currently skips:
    - via-way restrictions
    - `*:conditional` restrictions
    - `except=*` restrictions
    - `restriction:*` variants
    - malformed / unresolved restriction relations
  - Decide what should be supported, what should stay unsupported, and what should be surfaced more clearly to users.

- Add end-to-end tile-backed restriction tests.
  - Current coverage already exists for:
    - restriction materialization in generation
    - RMDF rule serialization
    - RMDF rule hydration in routing
    - nearest-point filtering behavior in `TileManager`
  - Still missing: an integration test proving that tile-generated restriction data changes actual routed output end to end.

## P2 — Cleanup

- Trim warning / dead-code noise reported by `cargo test --workspace -q`.
  - Current warnings include unused exports/imports in `ridi-router-tiles`, dead code in `ridi-router-routing`, and a small `unused_mut` test warning.
  - Good cleanup targets from the latest test run:
    - `crates/ridi-router-tiles/src/proximity/mod.rs`
    - `crates/ridi-router-tiles/src/rmdf/generator/manifest.rs`
    - `crates/ridi-router-tiles/src/rmdf/mod.rs`
    - `crates/ridi-router-routing/src/rmdf/io.rs`
    - `crates/ridi-router-routing/src/rmdf/validation.rs`
    - `crates/ridi-router-routing/src/router/itinerary.rs`
    - `crates/ridi-router-tiles/tests/public_api_success.rs`

## Closed / no longer open

These were previously tracked in stale review docs, but they are already implemented and should not remain open todos:
- basic RMDF rule serialization
- RMDF rule reading on the routing side
- runtime point rule hydration from tiles
- nearest-point rules filtering
- nearest-point highway filtering
- grid-ring nearest search
