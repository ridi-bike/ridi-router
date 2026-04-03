# Library Refactor Phase 2

## Goal

Convert the repo into a virtual-root Cargo workspace and extract `ridi-router-routing` as the first reusable library.

This phase should make routing usable from Rust without pulling in CLI parsing, JSON/GPX output policy, or stdout/stderr concerns.

## In scope

- Convert the root to a virtual workspace.
- Create:
  - `crates/ridi-router-cli`
  - `crates/ridi-router-routing`
- Move the binary entrypoint into `ridi-router-cli`.
- Move routing-owned code into `ridi-router-routing`.
- Expose the routing public API from the new crate.
- Make the CLI depend on `ridi-router-routing`.
- Keep routing tile opening/reading inside the routing library.
- Preserve the interim singleton-backed routing internals if still needed.
- Move routing-focused tests into the routing crate.
- Keep rule-file parsing, JSON, GPX, output-dir policy, and stderr rendering in the CLI crate.

## Out of scope

- Extracting `ridi-router-tiles` yet.
- Creating `ridi-router-common` unless phase 2 reveals an unavoidable shared-schema need.
- Removing the hidden process-global graph internals.
- Concurrent request handling.
- Streaming/event APIs.
- Broad tile-generation cleanup.

## Files to change

### Workspace
- `Cargo.toml`
  - replace package root with virtual workspace config
- `Cargo.lock`
  - refresh after workspace split

### New crates
- `crates/ridi-router-cli/Cargo.toml`
- `crates/ridi-router-cli/src/main.rs`
- `crates/ridi-router-routing/Cargo.toml`
- `crates/ridi-router-routing/src/lib.rs`

### Code likely moving into `ridi-router-cli`
- current `src/main.rs`
- current CLI-facing parts of `src/router_runner.rs`
- `src/json_writer.rs`
- `src/gpx_writer.rs`
- `src/result_writer.rs`
- `src/file_naming.rs`

### Code likely moving into `ridi-router-routing`
- `src/router/*`
- routing-owned parts of `src/route_output.rs`
- routing-side `src/map_data/*`
- routing-side `src/rmdf/tile_manager.rs`
- routing-owned rules and validation types if they remain routing-owned

## Implementation details

1. Replace the root package with a virtual workspace root.
2. Create `ridi-router-cli` as the only binary crate.
3. Create `ridi-router-routing` as a library crate.
4. Move routing modules into the routing crate and expose only the intended public API.
5. Update CLI code so route generation goes only through `ridi-router-routing` public types and functions.
6. Keep any singleton-backed implementation details private inside the routing crate.
7. Keep tile generation in the CLI crate temporarily, but isolate it so phase 3 can move it cleanly.
8. Move routing tests so they live with the routing code they validate.

## Unit tests to write

### `ridi-router-routing`
- `routing_executor_open_reads_tiles_dir_manifest`
- `routing_executor_generate_start_finish_returns_route_computation`
- `routing_executor_generate_round_trip_returns_route_computation`
- `routing_executor_reuse_is_sequential_and_supported`
- `routing_executor_conflicting_tiles_dir_returns_open_error`
- `routing_public_api_does_not_expose_cli_types`
- `routing_library_does_not_require_rule_file_path_input`

### `ridi-router-cli`
- `generate_route_cli_maps_args_into_route_request`
- `generate_route_cli_renders_routing_open_error`
- `generate_route_cli_renders_routing_generation_error`
- `json_output_adapter_uses_routing_result_type`
- `gpx_output_adapter_uses_routing_result_type`
- `cli_rule_file_parser_stays_outside_routing_crate`

## Testing

- Run `cargo check --workspace`.
- Run `cargo test -p ridi-router-routing`.
- Run `cargo test -p ridi-router-cli`.
- Run route CLI integration tests against fixture tiles.
- Verify that the produced binary name is `ridi-router-cli`.
- Verify that package naming also matches `ridi-router-cli`.

## Validation criteria

- The root is a virtual Cargo workspace.
- `ridi-router-routing` builds as a reusable library.
- `ridi-router-cli` builds as the only binary crate.
- CLI route generation goes through the public API of `ridi-router-routing`.
- `ridi-router-routing` does not depend on `clap`, JSON output policy, GPX output policy, stdout/stderr policy, or rule-file parsing from disk.
- Routing tests live with the routing crate.
- Workspace build and tests pass.

## Progress checklist

- [ ] Convert root `Cargo.toml` to a virtual workspace.
- [ ] Create `crates/ridi-router-cli`.
- [ ] Create `crates/ridi-router-routing`.
- [ ] Move binary entrypoint into `ridi-router-cli`.
- [ ] Move routing modules into `ridi-router-routing`.
- [ ] Expose routing public API from `ridi-router-routing`.
- [ ] Update CLI imports to use routing crate public API only.
- [ ] Keep singleton-backed routing internals private.
- [ ] Move routing tests into the routing crate.
- [ ] Run `cargo check --workspace`.
- [ ] Run `cargo test -p ridi-router-routing`.
- [ ] Run `cargo test -p ridi-router-cli`.

## Done when

The workspace exists, the CLI is a thin consumer of a real `ridi-router-routing` library, and route generation can be used from Rust without pulling in CLI/output concerns.
