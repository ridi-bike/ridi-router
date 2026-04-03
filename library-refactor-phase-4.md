# Library Refactor Phase 4

## Goal

Finish the refactor by making `ridi-router-cli` obviously just a thin adapter over `ridi-router-routing` and `ridi-router-tiles`, then harden the workspace with tests and docs.

## In scope

- Remove remaining boundary leaks from CLI into library internals.
- Ensure the CLI owns:
  - `clap` parsing
  - rule-file parsing from disk
  - JSON output shaping
  - GPX output shaping
  - file naming
  - output directory validation
  - stdout/stderr rendering
  - human-readable error rendering
- Ensure the libraries own:
  - routing domain APIs and typed errors
  - tile-generation workflow and typed errors
- Remove dead transitional adapters and private shims left from phases 1-3.
- Update docs and examples for workspace usage.
- Verify dependency direction stays clean.
- Move tests to final ownership locations.
- Perform final workspace hardening and cleanup.

## Out of scope

- Full removal of the routing singleton internals.
- Public streaming/event API design.
- Richer route debug/exploration models.
- Backward-compatibility shims for the old single-crate layout.
- Broad algorithmic changes unrelated to crate ownership.

## Files to change

### Primary
- `crates/ridi-router-cli/src/*`
  - final CLI-only orchestration and output adapters
- `crates/ridi-router-routing/src/*`
  - remove any lingering CLI/output leakage
- `crates/ridi-router-tiles/src/*`
  - remove any lingering CLI/output leakage
- optional `crates/ridi-router-common/src/*`
  - confirm it remains small and data-only
- `README.md`
  - document workspace layout and new binary name
- any helper scripts or dev docs
  - update commands and examples

### Tests and validation
- final unit tests in crate-local test modules
- integration tests under `tests/` if still useful at workspace root or CLI crate level
- dependency checks via `cargo tree`

## Implementation details

1. Remove remaining library dependence on CLI-facing modules.
2. Remove temporary compatibility helpers that are no longer needed.
3. Audit dependency direction:
   - CLI may depend on routing and tiles
   - routing and tiles may depend on common only if needed
   - routing must not depend on tiles
   - tiles must not depend on routing
4. Make sure rule-file parsing is still CLI-owned.
5. Make sure JSON and GPX output models are still CLI-owned adapters.
6. Update docs and examples to reflect the new workspace and binary name.
7. Move any remaining tests to the responsibility they validate.

## Unit tests to write

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

### End-to-end tests
- `generate_route_end_to_end_json_output`
- `generate_route_end_to_end_gpx_output`
- `generate_tiles_then_route_end_to_end`
- `cli_reports_typed_library_errors_human_readably`

## Testing

- Run `cargo check --workspace`.
- Run `cargo test --workspace`.
- Run end-to-end CLI route generation with JSON output.
- Run end-to-end CLI route generation with GPX output.
- Run end-to-end CLI tile generation, then route generation against the produced tiles.
- Run `cargo tree --workspace` to confirm dependency direction.
- Do a final docs and examples sanity pass.

## Validation criteria

- `ridi-router-cli` is visibly thin and owns only CLI/file/output concerns.
- `ridi-router-routing` can be used from Rust without pulling in CLI/output concerns.
- `ridi-router-tiles` can be used from Rust to generate tiles with typed errors.
- Dependency direction matches the architecture locked in `library-refactor.md`.
- Any `ridi-router-common` crate is small and data-only.
- Workspace tests pass end to end.
- Docs and examples describe the real architecture.
- The only major deferred item is the later follow-up to remove the process-global routing internals completely.

## Progress checklist

- [ ] Remove remaining CLI leakage from routing crate.
- [ ] Remove remaining CLI leakage from tiles crate.
- [ ] Remove dead transitional shims.
- [ ] Audit dependency direction with `cargo tree`.
- [ ] Confirm rule-file parsing stays in CLI.
- [ ] Confirm JSON/GPX shaping stays in CLI.
- [ ] Move tests to final ownership locations.
- [ ] Update README and developer docs.
- [ ] Add final end-to-end coverage.
- [ ] Run `cargo check --workspace`.
- [ ] Run `cargo test --workspace`.
- [ ] Perform final doc/example sanity pass.

## Done when

The workspace tells a clean, consistent story: reusable routing and tile-generation libraries underneath, one thin CLI on top, clean dependency direction, typed library errors, and no accidental CLI ownership leaking into reusable crates.
