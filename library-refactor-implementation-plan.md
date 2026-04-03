# Library Refactor Implementation Plan

## Phase split decision

This refactor should be delivered in **4 phases**.

That split is deliberate:
- **Phase 1** cleans ownership boundaries while the repo is still one crate.
- **Phase 2** extracts the routing library and creates the workspace shell.
- **Phase 3** extracts the tile-generation library and makes the shared-type decision.
- **Phase 4** finishes the thin CLI, removes remaining boundary leaks, and hardens the workspace.

This is the smallest phase split that keeps risk manageable without turning the refactor into a long series of tiny cleanup-only steps.

## Why this split

A 2-phase plan would mix too many concerns at once:
- API cleanup
- workspace conversion
- routing extraction
- tile extraction
- CLI cleanup
- shared type decisions

A 4-phase plan keeps each phase reviewable and testable:
1. **clean boundaries first**
2. **extract routing**
3. **extract tiles**
4. **finalize CLI and workspace**

This also matches the locked decision in `library-refactor.md` to do the work **incrementally with cleanup**, not as a big-bang rewrite.

---

## Phase 1 - Boundary cleanup inside the current crate

## Goal

Make ownership boundaries explicit before moving code into workspace crates.

## In scope

- Keep the repo as a single package for now.
- Introduce library-oriented request/result/config types in the current crate.
- Introduce the intended routing API shape in current-crate form:
  - `RoutingExecutor`
  - `RoutingExecutorConfig`
  - `RouteRequest`
  - `RouteMode`
  - `RoutingError`
- Introduce the intended tile-generation API shape in current-crate form:
  - `TileInputSource`
  - `TileGenerationRequest`
  - `TileGenerationSummary`
  - `TileGenerationError`
- Move CLI-only concerns behind clear adapter functions/modules:
  - `clap` parsing
  - rule-file parsing from disk
  - JSON shaping
  - GPX shaping
  - stdout/stderr policy
  - output directory policy
  - file naming
- Make routing initialization happen behind an executor-style entrypoint rather than directly from CLI flow.
- Add typed library-facing errors at the boundary, even if some internals still use existing error types temporarily.
- Keep the current process-global graph as an internal implementation detail behind the executor.
- Add explicit conflicting-`tiles_dir` rejection behavior for repeated executor opens in the same process.

## Out of scope

- Converting the repo to a Cargo workspace.
- Moving code into `crates/` yet.
- Full removal of the process-global `MapDataGraph` implementation.
- Multi-dataset routing in one process.
- Streaming routing/event APIs.
- Any public JSON schema redesign.
- GPX format redesign.
- Introducing `ridi-router-common`.

## Concrete work

1. Add current-crate modules that mirror the future library boundaries.
2. Refactor `router_runner.rs` so it becomes CLI orchestration only.
3. Move route execution behind `RoutingExecutor::open(...).generate(...)`.
4. Move tile generation behind a request-shaped `generate_tiles(...)` function.
5. Ensure JSON and GPX writers consume library result types or CLI adapter models, not routing internals.
6. Replace `anyhow` at the intended library boundary with typed enums.
7. Keep transitional adapters private to the current crate.

## Likely files/modules touched

- `src/router_runner.rs`
- `src/main.rs`
- `src/route_output.rs`
- `src/result_writer.rs`
- `src/json_writer.rs`
- `src/gpx_writer.rs`
- `src/file_naming.rs`
- `src/router/*`
- `src/map_data/*`
- `src/rmdf/generator/*`
- new boundary modules for routing and tile APIs

## Unit tests for this phase

### Routing boundary tests
- `routing_executor_open_rejects_conflicting_tiles_dir`
- `routing_executor_open_allows_reopening_same_tiles_dir`
- `routing_executor_generate_returns_transport_neutral_route_result`
- `route_mode_start_finish_maps_from_cli_inputs`
- `route_mode_round_trip_maps_from_cli_inputs`
- `routing_error_exposes_typed_open_failure`
- `routing_error_exposes_typed_generation_failure`

### Tile API boundary tests
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

- Run focused unit tests for new boundary modules.
- Run existing route-generation tests.
- Run existing tile-generation tests.
- Run `cargo check` and `cargo test` in the single-crate layout.
- Add one regression test that proves the CLI still produces the same route output files for an existing fixture.

## Acceptance criteria

- The CLI route path calls an executor-style routing API instead of initializing graph state directly.
- The tile-generation path calls a request-style tile API.
- JSON/GPX/output-dir/file naming stay owned by CLI-facing modules.
- Typed library-facing error enums exist for routing and tile generation.
- Opening a routing executor twice with different `tiles_dir` values in one process fails with a typed error.
- The repo still builds and tests successfully as a single crate.
- No workspace move has happened yet.

---

## Phase 2 - Create the workspace and extract `ridi-router-routing`

## Goal

Turn the repo into a virtual-root workspace and extract a reusable routing library first.

## In scope

- Convert the root to a virtual workspace.
- Create:
  - `crates/ridi-router-cli`
  - `crates/ridi-router-routing`
- Move the existing binary entrypoint into `ridi-router-cli`.
- Move routing-owned code into `ridi-router-routing`.
- Make the CLI depend on `ridi-router-routing`.
- Expose the routing public API from the new crate.
- Keep routing tile opening/reading inside the routing library.
- Preserve the interim singleton-backed internals if still needed.
- Move routing-focused tests into the routing crate.

## Out of scope

- Extracting `ridi-router-tiles` yet.
- Creating `ridi-router-common` unless phase 2 reveals an unavoidable manifest/schema dependency.
- Removing the hidden process-global graph internals.
- Concurrent multi-request executor support.
- Streaming/event APIs.
- Broad tile-generation cleanup.

## Concrete work

1. Replace root `Cargo.toml` package configuration with workspace configuration.
2. Add member crates for CLI and routing.
3. Move routing modules into `crates/ridi-router-routing/src`.
4. Move CLI orchestration and writers into `crates/ridi-router-cli/src`.
5. Update imports so CLI uses only the routing crate public API for route generation.
6. Make `ridi-router-cli` produce the `ridi-router-cli` binary.
7. Keep temporary internal implementation shims private inside `ridi-router-routing`.
8. Keep tile-generation code in the CLI crate temporarily, but isolate it so phase 3 can move it cleanly.

## Likely files/crates touched

- `/Cargo.toml`
- `/crates/ridi-router-cli/Cargo.toml`
- `/crates/ridi-router-routing/Cargo.toml`
- moved code from current `src/main.rs`
- moved code from current `src/router_runner.rs`
- moved code from current `src/router/*`
- moved code from current routing-side `src/map_data/*`
- moved code from current routing-side `src/rmdf/tile_manager.rs`
- integration tests currently under `/tests`

## Unit tests for this phase

### `ridi-router-routing`
- `routing_executor_open_reads_tiles_dir_manifest`
- `routing_executor_generate_start_finish_returns_route_computation`
- `routing_executor_generate_round_trip_returns_route_computation`
- `routing_executor_reuse_is_sequential_and_supported`
- `routing_executor_conflicting_tiles_dir_returns_open_error`
- `routing_public_api_does_not_expose_cli_types`

### `ridi-router-cli`
- `generate_route_cli_maps_args_into_route_request`
- `generate_route_cli_renders_routing_open_error`
- `generate_route_cli_renders_routing_generation_error`
- `json_output_adapter_uses_routing_result_type`
- `gpx_output_adapter_uses_routing_result_type`

## Testing

- Run `cargo check --workspace`.
- Run `cargo test -p ridi-router-routing`.
- Run `cargo test -p ridi-router-cli`.
- Run route CLI integration tests against fixture tiles.
- Verify binary naming and package naming are both `ridi-router-cli`.

## Acceptance criteria

- The root is a virtual Cargo workspace.
- `ridi-router-routing` builds as a reusable library crate.
- `ridi-router-cli` builds as the only binary crate.
- CLI route generation goes through the public API of `ridi-router-routing`.
- `ridi-router-routing` does not depend on `clap`, JSON output policy, GPX output policy, stdout/stderr policy, or rule-file parsing from disk.
- Routing tests live with the routing crate.
- Workspace builds and tests pass.

---

## Phase 3 - Extract `ridi-router-tiles` and make the shared-type decision

## Goal

Move tile generation into its own library and decide whether a small `ridi-router-common` crate is actually needed.

## In scope

- Create `crates/ridi-router-tiles`.
- Move PBF-to-RMDF generation code into the tiles crate.
- Expose request/result/error types from the tiles crate.
- Make `ridi-router-cli` depend on `ridi-router-tiles` for tile generation.
- Decide whether manifest/schema types should remain in one crate or move into `ridi-router-common`.
- If needed, create a **small data-only** `ridi-router-common` crate for manifest/schema/shared pure types.
- Move tile-generation tests into the tiles crate.
- Remove `anyhow` from tile library public APIs.

## Out of scope

- Creating a utilities dumping-ground crate.
- Moving CLI helpers into common.
- Removing routing singleton internals.
- Redesigning tile format.
- Changing tile-generation operational model to executor-style.
- Parallel or distributed generation redesign.

## Concrete work

1. Create `ridi-router-tiles` crate and move generation modules there.
2. Expose `generate_tiles(request) -> Result<..., TileGenerationError>`.
3. Move manifest-writing and tile-writing responsibilities fully into the tiles crate.
4. Update CLI generate-tiles command to build a request and call the library.
5. Evaluate cross-crate manifest ownership:
   - if only tiles owns it cleanly, keep it in tiles
   - if both routing and tiles need the schema directly, extract only the pure schema/types into `ridi-router-common`
6. Update tests to follow ownership.

## Likely files/crates touched

- `/crates/ridi-router-tiles/Cargo.toml`
- current `src/rmdf/generator/*`
- current `src/osm_data/*`
- tile-writing and manifest-writing modules
- optional `/crates/ridi-router-common/*`
- CLI generate-tiles command implementation

## Unit tests for this phase

### `ridi-router-tiles`
- `generate_tiles_from_file_input_writes_manifest`
- `generate_tiles_from_directory_input_writes_manifest`
- `generate_tiles_writes_expected_tile_files`
- `generate_tiles_returns_summary_counts`
- `tile_generation_error_reports_missing_input`
- `tile_generation_error_reports_manifest_write_failure`
- `tile_generation_error_reports_tile_write_failure`
- `tile_generation_public_api_does_not_expose_clap_or_cli_types`

### `ridi-router-common` if created
- `manifest_types_round_trip_serialize_deserialize`
- `manifest_version_constant_matches_expected_value`
- `shared_manifest_schema_is_data_only`

### `ridi-router-cli`
- `generate_tiles_cli_maps_args_into_tile_generation_request`
- `generate_tiles_cli_renders_typed_tile_generation_error`

## Testing

- Run `cargo check --workspace`.
- Run `cargo test -p ridi-router-tiles`.
- Run `cargo test -p ridi-router-cli`.
- If common is introduced, run `cargo test -p ridi-router-common`.
- Run end-to-end tile generation on:
  - single-file input
  - directory input
- Verify generated output remains readable by routing tests.

## Acceptance criteria

- `ridi-router-tiles` is a reusable library crate.
- CLI tile generation goes through the tiles crate public API.
- Tile-generation disk writing lives in `ridi-router-tiles`, not in CLI.
- `anyhow` is gone from tile library public surfaces.
- Shared manifest/schema types either have a clear home or are extracted into a small data-only common crate.
- Workspace builds and tests pass after the move.

---

## Phase 4 - Finalize the thin CLI and harden the workspace

## Goal

Finish the refactor so the CLI is clearly only an adapter over the two libraries.

## In scope

- Remove remaining boundary leaks from CLI into library internals.
- Ensure CLI owns:
  - rule-file parsing from disk
  - JSON and GPX output models/adapters
  - file naming
  - output directory validation
  - stdout/stderr rendering
  - human-readable error rendering
- Ensure libraries own:
  - routing domain APIs and typed errors
  - tile-generation request/workflow and typed errors
- Clean up package metadata, docs, and test locations.
- Update README and developer docs for workspace usage.
- Verify crate dependency direction stays clean.
- Remove dead transitional modules and private shims that are no longer needed.

## Out of scope

- Full instance-scoped removal of the routing singleton internals.
- Public streaming/event API design.
- Richer route debug/exploration models.
- Backward compatibility shims for the old single-crate layout.
- Broad algorithmic changes unrelated to crate ownership.

## Concrete work

1. Remove any remaining library dependence on CLI-specific modules.
2. Remove stale compatibility helpers from phases 1-3.
3. Audit dependency graphs so `routing` and `tiles` stay independent unless a small common crate is justified.
4. Move tests to final ownership locations.
5. Update docs and examples to reflect the workspace layout and new binary name.
6. Verify release/package metadata for the breaking version.

## Unit tests for this phase

### CLI-only tests
- `cli_rule_file_parser_returns_routing_rule_values`
- `cli_json_output_can_diverge_from_library_struct_shape`
- `cli_gpx_output_can_diverge_from_library_struct_shape`
- `cli_output_directory_policy_rejects_non_empty_dir`
- `cli_file_naming_uses_expected_route_names`
- `cli_human_error_rendering_wraps_typed_library_errors`

### Dependency boundary tests
- `routing_crate_source_contains_no_clap_dependency_usage`
- `routing_crate_source_contains_no_json_writer_usage`
- `routing_crate_source_contains_no_gpx_writer_usage`
- `tiles_crate_source_contains_no_clap_dependency_usage`
- `cli_depends_on_routing_and_tiles_but_reverse_is_not_true`

## Testing

- Run `cargo check --workspace`.
- Run `cargo test --workspace`.
- Run end-to-end CLI route generation with JSON output.
- Run end-to-end CLI route generation with GPX output.
- Run end-to-end CLI tile generation, then route generation against the produced tiles.
- Run a dependency audit with `cargo tree` to confirm directionality.
- Perform a final docs/readme validation pass.

## Acceptance criteria

- `ridi-router-cli` is visibly thin and only orchestrates library calls plus CLI/file/output concerns.
- `ridi-router-routing` can be used from Rust without pulling in CLI/output concerns.
- `ridi-router-tiles` can be used from Rust to generate tiles with typed errors.
- Dependency direction matches the architecture in `library-refactor.md`.
- Any `ridi-router-common` crate, if present, is small and data-only.
- Workspace tests pass end-to-end.
- The repo matches the desired end state described in `library-refactor.md`, except for the explicitly deferred follow-up to remove the process-global routing internals.

---

## Cross-phase guardrails

These rules apply in every phase:
- Do not introduce streaming/event APIs yet.
- Do not redesign route result models beyond what the library boundary needs now.
- Do not let JSON or GPX shape define the routing library structs.
- Do not move rule-file parsing into reusable libraries.
- Do not create `ridi-router-common` early without a clear concrete need.
- Do not promise concurrent multi-request routing support.
- Keep the hidden singleton-backed routing internals explicitly temporary.

## Final confirmation

**Confirmed phase split: 4 phases**

1. **Boundary cleanup in current crate**
2. **Workspace + routing library extraction**
3. **Tile library extraction + shared-type decision**
4. **Thin CLI finalization + workspace hardening**

This is the recommended implementation order.