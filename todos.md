# Todos

This file is the source of truth for open follow-up work in this repo.

Last audited: 2026-04-06
Validation snapshot:
- `cargo test --workspace -q`
  - `ridi-router-common`: pass
  - `ridi-router-routing`: pass
  - `ridi-router-tiles`: pass
  - `ridi-router-cli`: `generate_route_cli` still has 4 failing tests in this checkout

## P0 — Audit follow-up for route-generation fixes

- Make the CLI rule source explicit instead of implicitly reading stdin when `--rule-file` is absent.
  - Current fix review: treating empty stdin as `RouterRules::default()` is a reasonable guard, but it still leaves "defaults" vs "stdin" environment-dependent in `crates/ridi-router-cli/src/cli/rules.rs`.
  - Prefer: no `--rule-file` means defaults; stdin should require `--rule-file -` or a dedicated flag, with tests for empty and non-empty stdin.

- Lock down the RMDF packed-format contract and centralize low-level readers.
  - Current fix review: the unaligned `TagSetRecord` read is the correct fix, not a workaround.
  - Follow-up: document that RMDF sections are packed/on-disk records, audit other typed mmap reads for alignment assumptions, and reduce reader duplication between `crates/ridi-router-routing/src/rmdf/io.rs` and `crates/ridi-router-cli/src/rmdf/io.rs`.

- Split endpoint snapping policy from waypoint-generation highway preferences.
  - Current fix review: retrying start/finish snapping without `WP_LOOKUP_ALLOWED_HWS` is a good robustness fix, but it is still coupling endpoint validity to a waypoint-generation heuristic.
  - Follow-up: introduce an explicit endpoint snapping policy and keep a regression test for "preferred snap misses, fallback snap succeeds" behavior.

- Propagate closest-point lookup failures instead of logging and returning `None`.
  - Current fix review: warning logs improved debuggability, but this is still masking data/lookup failures as ordinary "point not found" results.
  - Follow-up: preserve `Result<Option<_>>` through `MapDataGraph` / `RoutingContext` / `routing_api` so real lookup failures do not collapse into `RoutingGenerationError::PointNotFound`.

- Define first-class routing profiles instead of relying on `dev.sh` to swap in `rules-fast.json`.
  - Current fix review: using the fast preset by default in `dev.sh` is a practical dev-only workaround, not the real product-level default story.
  - Follow-up: add named profiles such as `fast` / `default` / `thorough`, document when each should be used, and keep explicit test coverage for both the fast profile and the true `RouterRules::default()` path.

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


## P1 — Nearest-point query hot path

- Reduce per-candidate allocations in `TileManager::get_closest_to_coords(...)`.
  - `crates/ridi-router-routing/src/rmdf/tile_manager.rs` builds `Vec<AdjacentLineTags>` for each candidate point and `optional_tag_value(...)` clones tag strings with `to_string()` inside the inner search loop.
  - This path runs during nearest-point search before every route, so avoid/limit-tag checks currently pay repeated allocation and UTF-8 copy costs.
  - Prefer comparing tag indices, borrowing `&str` from mapped tiles, or caching decoded adjacent-tag summaries per point.

- Extract nearest-point filter/query context instead of threading 10-11 arguments through helper functions.
  - `find_closest_in_grid_rings(...)`, `find_closest_in_points(...)`, and `update_closest_for_points(...)` all carry the same large parameter set.
  - Clippy already flags these signatures as `too_many_arguments`, which is a good sign the search/filter boundary is leaking too much state.
  - A small query context struct would simplify testing, make future perf work safer, and reduce call-site duplication.

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

- Centralize repeated CLI test helpers / fixtures.
  - `crates/ridi-router-cli/src/cli/output_dir.rs` and `crates/ridi-router-cli/src/router_runner.rs` both test `prepare_empty_output_dir(...)` directly.
  - `crates/ridi-router-cli/src/json_writer.rs`, `crates/ridi-router-cli/src/gpx_writer.rs`, and `crates/ridi-router-cli/src/result_writer.rs` each duplicate `unique_test_dir`, route-stat builders, and route fixture helpers.
  - `crates/ridi-router-cli/tests/generate_route_cli.rs` and `crates/ridi-router-cli/tests/generate_tiles_cli.rs` also hand-roll overlapping command / temp-dir setup logic.
  - Move the shared pieces into `ridi-router-test-support` or a local test helper module so CLI behavior changes only need one fixture update.

- Reduce peak-memory churn and duplicated pass scaffolding in `InMemoryPbf`.
  - `crates/ridi-router-tiles/src/osm_data/in_memory_pbf.rs` collects full `Vec<_>`s in `load_nodes(...)`, `load_ways(...)`, and `load_relations(...)` before building the final `HashMap` / `RTree` structures.
  - The way / relation passes also clone tags and member lists up front, which inflates memory use on large PBF imports.
  - Consider streaming reducers, pass-specific builders, or shared collection helpers to cut duplicate logic and lower peak allocation pressure.

## Closed / no longer open

