# Library Refactor Phase 3

## Goal

Extract `ridi-router-tiles` as a reusable library and make the shared-type decision deliberately instead of by accident.

This phase should move all PBF-to-RMDF generation ownership into the tile library while keeping the CLI thin.

## In scope

- Create `crates/ridi-router-tiles`.
- Move PBF reading and RMDF tile generation into the tiles crate.
- Expose:
  - `TileInputSource`
  - `TileGenerationRequest`
  - `TileGenerationSummary`
  - `TileGenerationError`
  - `generate_tiles(request)`
- Make `ridi-router-cli` depend on `ridi-router-tiles` for tile generation.
- Keep tile-generation disk writing inside the tiles crate.
- Move tile-generation tests into the tiles crate.
- Remove `anyhow` from tile library public APIs.
- Decide whether manifest/schema types stay in one crate or need extraction into `ridi-router-common`.
- If needed, create a small data-only `ridi-router-common` crate.

## Out of scope

- Creating a large common utilities crate.
- Moving CLI helpers into common.
- Removing routing singleton internals.
- Redesigning tile format.
- Making tile generation executor-shaped.
- Broad concurrency redesign for tile generation.
- Streaming/event APIs.

## Files to change

### New crates
- `crates/ridi-router-tiles/Cargo.toml`
- `crates/ridi-router-tiles/src/lib.rs`
- optional `crates/ridi-router-common/Cargo.toml`
- optional `crates/ridi-router-common/src/lib.rs`

### Code likely moving into `ridi-router-tiles`
- current `src/rmdf/generator/*`
- current `src/osm_data/*`
- tile-writing logic
- manifest-writing logic
- tile-generation request/config logic

### Code that may move to `ridi-router-common` if justified
- current `src/rmdf/format.rs`
- manifest schema/types from current generator modules
- small pure RMDF schema/version constants needed by both libraries

### CLI updates
- generate-tiles command implementation in `ridi-router-cli`
- any CLI adapter code that builds `TileGenerationRequest`
- human-readable CLI rendering for `TileGenerationError`

## Implementation details

1. Create `ridi-router-tiles` and move tile-generation code there.
2. Expose a request-shaped public API centered on `generate_tiles(request)`.
3. Keep tile/manifest writing inside the tile library.
4. Update the CLI generate-tiles command so it only parses args, builds a request, calls the library, and renders errors.
5. Make the manifest ownership decision explicitly:
   - keep it in one crate if that keeps dependencies clean
   - extract only pure schema/data types into `ridi-router-common` if both libraries need them directly
6. Ensure any `ridi-router-common` crate stays small and data-only.
7. Move tile-generation tests so they live with the tiles crate.

## Unit tests to write

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
- `generate_tiles_cli_does_not_write_tiles_directly`

## Testing

- Run `cargo check --workspace`.
- Run `cargo test -p ridi-router-tiles`.
- Run `cargo test -p ridi-router-cli`.
- If `ridi-router-common` is introduced, run `cargo test -p ridi-router-common`.
- Run end-to-end tile generation for:
  - single-file input
  - directory input
- Verify the generated output remains readable by routing tests.

## Validation criteria

- `ridi-router-tiles` builds as a reusable library.
- CLI tile generation goes through the public API of `ridi-router-tiles`.
- Tile-generation disk writing lives in `ridi-router-tiles`, not in the CLI.
- `anyhow` is removed from tile library public surfaces.
- Shared manifest/schema types either have a clear home or are extracted into a small data-only `ridi-router-common` crate.
- Workspace build and tests pass after the move.

## Progress checklist

- [ ] Create `crates/ridi-router-tiles`.
- [ ] Move tile-generation modules into `ridi-router-tiles`.
- [ ] Expose request/result/error types from the tiles crate.
- [ ] Expose `generate_tiles(request)`.
- [ ] Update CLI generate-tiles path to use the tiles crate public API.
- [ ] Remove `anyhow` from tile library public surfaces.
- [ ] Decide manifest/schema ownership.
- [ ] Create `ridi-router-common` only if clearly justified.
- [ ] Move tile-generation tests into the tiles crate.
- [ ] Run `cargo check --workspace`.
- [ ] Run crate tests.
- [ ] Run end-to-end tile-generation checks.

## Done when

Tile generation is owned by a real `ridi-router-tiles` library, the CLI only orchestrates it, and any shared manifest/schema types have a deliberate home instead of leaking across crates.
