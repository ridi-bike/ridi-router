# IPC Refactor Phase 4

## Goal
Finish documentation, test coverage, and repo cleanup so the repository tells the truth about the new one-shot CLI model.

## In scope
- Update examples and usage docs.
- Add focused unit tests for new output behavior.
- Add integration tests for the one-shot CLI flow.
- Make test expectations explicitly avoid stable route ordering assumptions.

## Out of scope
- NDJSON/event streaming implementation.
- Global `MapDataGraph` removal.
- Stable output ordering.

## Files to change
### Primary
- `README.md`
  - update route-generation examples
  - remove old `--input` / `--cache-dir` route examples
  - document `--tiles`, `--output-dir`, `--format`
  - document zero-route success behavior
- `justfile`
  - replace stale route commands with current ones
- new `tests/` directory if needed
  - add end-to-end CLI coverage
- existing modules with unit tests
  - `src/router_runner.rs`
  - `src/result_writer.rs`
  - `src/gpx_writer.rs`
  - new JSON writer module

## Test strategy
### Unit tests
Keep logic-heavy tests close to the code:
- CLI argument validation in `src/router_runner.rs`
- filename generation logic in output helpers
- per-format writing behavior in `src/gpx_writer.rs` and JSON writer module
- zero-route writer behavior in `src/result_writer.rs`

### Integration tests
Add end-to-end CLI coverage in `tests/` if practical:
- command succeeds with valid tiles/output-dir/format
- command fails with invalid tiles dir
- command fails when manifest is missing
- command reuses existing empty output dir
- command rejects non-empty output dir
- command succeeds with zero routes and leaves output dir empty

## Unit tests to write
### `src/router_runner.rs`
- `validate_output_dir_accepts_missing_then_createable_dir`
- `validate_output_dir_accepts_existing_empty_dir`
- `validate_output_dir_rejects_non_empty_dir`
- `validate_zero_route_success_logs_expected_message` (or equivalent helper-level test)

### output helper module
- `filename_rounds_distance_to_nearest_whole_kilometer`
- `filename_uses_expected_extension_for_format`
- `filename_generation_does_not_imply_stable_ordering`

### `src/result_writer.rs`
- `result_writer_leaves_output_dir_empty_when_routes_are_empty`
- `result_writer_writes_only_requested_format`

### integration tests in `tests/`
- `generate_route_end_to_end_gpx_output_dir`
- `generate_route_end_to_end_json_output_dir`
- `generate_route_non_empty_output_dir_fails`
- `generate_route_zero_routes_succeeds`

## Validation criteria
- `README.md` examples match the real CLI.
- `justfile` commands match the real CLI.
- Unit tests cover new writer/CLI validation behavior.
- Integration tests cover the key one-shot CLI contract.
- Tests do not assume stable route ordering.
- `cargo test` passes for the refactor-relevant coverage.

## Progress checklist
- [ ] Update `README.md` route-generation examples.
- [ ] Update `justfile` recipes.
- [ ] Add or expand unit tests in router/output modules.
- [ ] Add integration tests under `tests/`.
- [ ] Cover empty-dir reuse behavior.
- [ ] Cover non-empty-dir failure behavior.
- [ ] Cover zero-route success behavior.
- [ ] Ensure tests do not depend on file order.
- [ ] Run `cargo test`.
- [ ] Do a final doc/example sanity pass.

## Done when
The docs, scripts, and tests all reinforce the same story: one request per process, `--tiles` as bootstrap, and file-based final outputs via `--output-dir` + `--format`.