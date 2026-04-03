# Library Refactor Phase 1

## Goal

Clean the ownership boundaries inside the current single-crate repo so the later workspace split is mostly a move, not a redesign.

This phase should introduce the intended library-facing API shapes while keeping the repository as one package.

## In scope

- Keep the repo as a single crate for now.
- Introduce routing boundary types in current-crate form:
  - `RoutingExecutor`
  - `RoutingExecutorConfig`
  - `RouteRequest`
  - `RouteMode`
  - `RoutingError`
- Introduce tile-generation boundary types in current-crate form:
  - `TileInputSource`
  - `TileGenerationRequest`
  - `TileGenerationSummary`
  - `TileGenerationError`
- Refactor CLI flow so it becomes orchestration only.
- Move graph initialization behind `RoutingExecutor::open(...)`.
- Keep the current process-global graph only as an internal implementation detail.
- Explicitly reject conflicting `tiles_dir` openings in one process with a typed routing error.
- Push CLI-only concerns behind clear adapter modules/functions:
  - `clap` parsing
  - rule-file parsing from disk
  - JSON output shaping
  - GPX output shaping
  - file naming
  - output directory policy
  - stdout/stderr rendering
- Introduce typed library-facing errors at the routing and tile-generation boundaries.

## Out of scope

- Converting the repo to a Cargo workspace.
- Moving code into `crates/`.
- Extracting `ridi-router-routing` or `ridi-router-tiles` yet.
- Creating `ridi-router-common`.
- Removing the singleton-backed routing internals.
- Multi-dataset routing in one process.
- Streaming/event APIs.
- JSON schema redesign.
- GPX redesign.

## Files to change

### Primary
- `src/router_runner.rs`
  - reduce it to CLI orchestration
  - build request/config types instead of reaching into routing internals directly
- `src/main.rs`
  - declare new boundary modules if needed
- `src/route_output.rs`
  - ensure it matches the intended library result boundary
- `src/result_writer.rs`
  - keep it CLI-owned and consume route result types cleanly
- `src/json_writer.rs`
  - keep JSON policy at the CLI boundary
- `src/gpx_writer.rs`
  - keep GPX policy at the CLI boundary
- `src/file_naming.rs`
  - keep output naming at the CLI boundary
- `src/router/*`
  - isolate routing-owned code behind executor entrypoints
- `src/map_data/*`
  - keep singleton internals hidden behind executor open/generate flow
- `src/rmdf/generator/*`
  - isolate tile-generation entrypoint behind request-shaped API

### New modules likely needed
- `src/routing_api.rs` or `src/routing/mod.rs`
- `src/tiles_api.rs` or `src/tiles/mod.rs`
- `src/cli/` or equivalent current-crate adapter modules if useful

## Implementation details

1. Add library-shaped request/config/result/error types without creating new crates yet.
2. Introduce `RoutingExecutor::open(config)` as the intended routing entrypoint.
3. Move any CLI-owned graph setup out of `router_runner.rs` and behind the executor API.
4. Ensure `RoutingExecutor::open(...)` rejects a second open with a different `tiles_dir`.
5. Introduce a request-shaped tile-generation entrypoint such as `generate_tiles(request)`.
6. Keep JSON and GPX writers as CLI adapters over route results.
7. Keep rule-file parsing from disk in CLI-facing code.
8. Replace `anyhow` at the intended library boundary with typed enums, even if some internal conversions remain temporarily.
9. Keep any temporary compatibility helpers private to the current crate.

## Unit tests to write

### Routing boundary tests
- `routing_executor_open_rejects_conflicting_tiles_dir`
- `routing_executor_open_allows_reopening_same_tiles_dir`
- `routing_executor_generate_returns_route_computation`
- `route_mode_start_finish_maps_from_cli_inputs`
- `route_mode_round_trip_maps_from_cli_inputs`
- `routing_error_exposes_typed_open_failure`
- `routing_error_exposes_typed_generation_failure`

### Tile-generation boundary tests
- `tile_generation_request_accepts_file_input_source`
- `tile_generation_request_accepts_directory_input_source`
- `generate_tiles_returns_summary_on_success`
- `tile_generation_error_exposes_invalid_output_dir`

### CLI adapter tests
- `router_runner_builds_route_request_from_cli_args`
- `router_runner_parses_rule_file_into_rust_rule_values`
- `result_writer_serializes_cli_json_from_route_computation`
- `gpx_writer_serializes_gpx_from_route_computation`
- `prepare_output_dir_rejects_non_empty_dir`
- `file_naming_generates_expected_route_output_names`

## Testing

- Run targeted unit tests for new routing and tile boundary modules.
- Run existing route-generation unit tests.
- Run existing tile-generation unit tests.
- Run `cargo check`.
- Run `cargo test`.
- Add one regression test showing the CLI still produces expected route output for an existing fixture.

## Validation criteria

- The CLI route path goes through an executor-style routing API.
- The CLI tile-generation path goes through a request-style tile API.
- JSON, GPX, file naming, output directory validation, and stdout/stderr policy remain CLI-owned.
- Typed routing and tile-generation error enums exist at the intended library boundary.
- Opening routing with a different `tiles_dir` in the same process fails with a typed error.
- The repo still builds and tests successfully as a single crate.

## Progress checklist

- [ ] Add routing boundary types in current-crate form.
- [ ] Add tile-generation boundary types in current-crate form.
- [ ] Introduce `RoutingExecutor::open(...)`.
- [ ] Introduce request-shaped tile-generation entrypoint.
- [ ] Refactor `router_runner.rs` into CLI orchestration only.
- [ ] Keep JSON/GPX/file-output policy in CLI modules.
- [ ] Add typed routing errors.
- [ ] Add typed tile-generation errors.
- [ ] Add unit tests for routing boundary.
- [ ] Add unit tests for tile-generation boundary.
- [ ] Add unit tests for CLI adapters.
- [ ] Run `cargo check`.
- [ ] Run `cargo test`.

## Done when

The repo still has one crate, but its internal boundaries now match the future library architecture closely enough that phase 2 is mostly extraction, not redesign.
