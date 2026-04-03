# IPC Refactor Phase 2

## Goal
Cut over `generate-route` to the new strict output contract:
- `--output-dir DIR`
- `--format gpx|json`
- one file per route
- no final route payloads on stdout
- zero routes is a valid successful end state

## In scope
- Replace the current `--output FILE` contract for `generate-route`.
- Add explicit output format selection.
- Make GPX output multi-file.
- Make JSON output multi-file.
- Allow existing empty output directories.
- Fail on non-empty output directories.
- Keep route file ordering arbitrary for now.

## Out of scope
- Stable route ordering.
- NDJSON/event streaming implementation.
- Global state removal.
- IPC transport deletion itself.

## Files to change
### Primary
- `src/router_runner.rs`
  - change CLI args for `GenerateRoute`
  - remove `DataDestination` parsing from file extension logic
  - validate `--output-dir`
  - validate `--format`
  - treat zero routes as success
  - log when no routes are found
- `src/result_writer.rs`
  - redesign destination API around output directory + format
  - stop supporting final route payload writes to stdout for GPX/JSON route commands
- `src/gpx_writer.rs`
  - write one GPX file per route into a directory
  - centralize filename generation if possible

### New files/modules likely needed
- `src/json_writer.rs` or `src/output/json_writer.rs`
- `src/output/file_naming.rs` or similar helper for route filenames
- `src/output/mod.rs` if phase 1 did not already create it

## Implementation details
1. Replace current generate-route output argument shape.
   - remove `--output FILE` from route generation
   - add `--output-dir DIR`
   - add `--format gpx|json`
2. Validate output directory rules.
   - if missing: create it
   - if exists and empty: allow
   - if exists and contains files: fail
3. Redesign writer entrypoint around a directory-oriented output request.
4. GPX writer should emit one file per route.
5. JSON writer should emit one file per route.
6. Filename format for now:
   - ordinal prefix
   - rounded total distance in kilometers
   - extension based on format
   - example: `001-354km.gpx`
7. If route generation succeeds with zero routes:
   - exit code remains 0
   - output directory remains empty
   - logs indicate no routes were found

## Unit tests to write
### `src/router_runner.rs`
- `generate_route_cli_requires_output_dir`
- `generate_route_cli_requires_format`
- `generate_route_cli_rejects_old_output_flag`
- `generate_route_allows_existing_empty_output_dir`
- `generate_route_rejects_non_empty_output_dir`
- `generate_route_zero_routes_is_successful_end_state`

### `src/result_writer.rs`
- `writer_dispatches_to_gpx_writer_for_gpx_format`
- `writer_dispatches_to_json_writer_for_json_format`
- `writer_does_not_use_stdout_for_final_route_payloads`
- `writer_returns_ok_for_zero_routes_without_creating_files`

### `src/gpx_writer.rs`
- `gpx_writer_writes_one_file_per_route`
- `gpx_writer_uses_expected_filename_pattern`
- `gpx_writer_accepts_arbitrary_route_order_without_sorting`

### New JSON writer module
- `json_writer_writes_one_file_per_route`
- `json_writer_uses_expected_filename_pattern`
- `json_writer_serializes_each_route_as_standalone_document`

## Validation criteria
- `generate-route` uses `--output-dir` and `--format`.
- There is no compatibility bridge from `--output FILE`.
- Final GPX/JSON route outputs are not written to stdout.
- Existing empty output dirs work.
- Non-empty output dirs fail clearly.
- Zero-route success leaves an empty output dir and logs that no routes were found.
- `cargo check` passes.

## Progress checklist
- [ ] Replace `GenerateRoute` CLI args with `--output-dir` + `--format`.
- [ ] Remove old `--output FILE` route-generation path.
- [ ] Add output-directory validation logic.
- [ ] Refactor result writing around directory + format.
- [ ] Implement multi-file GPX writing.
- [ ] Implement multi-file JSON writing.
- [ ] Implement filename helper logic.
- [ ] Handle zero-route success case explicitly.
- [ ] Add/update unit tests for CLI parsing and validation.
- [ ] Add/update unit tests for GPX multi-file output.
- [ ] Add/update unit tests for JSON multi-file output.
- [ ] Run `cargo check`.

## Done when
The `generate-route` command fully uses the new output contract and no longer behaves like a single-file/stdout result command.