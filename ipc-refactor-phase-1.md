# IPC Refactor Phase 1

## Goal
Remove IPC-shaped result types from the normal `generate-route` flow and replace them with transport-neutral route/output models.

This phase should stop the CLI boundary from depending on `ipc_handler::{ResponseMessage, RouteMessage, RouterResult}` while keeping the current command behavior working long enough to support the next phase.

## In scope
- Introduce transport-neutral route result/output types.
- Make `router_runner.rs` produce those types instead of IPC response structs.
- Make writers consume transport-neutral types.
- Keep the global `MapDataGraph` runtime model as-is for now.
- Do **not** remove `src/ipc_handler.rs` yet if it is still needed for compilation during the transition.

## Out of scope
- CLI cutover to `--output-dir` / `--format`.
- Deleting IPC transport files/dependencies.
- Removing `MapDataGraph::get()` / `OnceLock`.
- NDJSON implementation.

## Files to change
### Primary
- `src/router_runner.rs`
  - stop importing/building `ResponseMessage`, `RouteMessage`, `RouterResult`
  - convert `RouteWithStats` into new transport-neutral output models
- `src/result_writer.rs`
  - replace `ResponseMessage` input with new internal output model
  - prepare API so phase 2 can switch destinations cleanly
- `src/gpx_writer.rs`
  - replace `RouteMessage` input with transport-neutral route model
- `src/main.rs`
  - add new module declarations if needed

### New files/modules likely needed
- `src/route_output.rs`, or
- `src/output/mod.rs`, or
- `src/output/model.rs`

Recommended model split:
- `ComputedRoute`
- `RouteStats` reuse existing stats type where possible
- `RouteComputation` / `RouteComputationResult`
- optional formatting-specific helpers kept outside the route model

## Implementation details
1. Define a new route output model in a non-IPC module.
   - It should represent final route data, not transport envelopes.
   - It should be usable by GPX and JSON writers.
2. Make `RouterRunner::run_generate_route()` build the new result model directly.
3. Update `ResultWriter::write(...)` to accept the new model.
4. Update `GpxWriter` to consume the new route type.
5. Keep error propagation local to CLI/application layers; do not recreate an `Envelope { id, result }` abstraction.

## Unit tests to write
### `src/gpx_writer.rs`
- `sort_by_longest_sorts_descending_by_length`
- `write_gpx_accepts_transport_neutral_routes`
- `write_gpx_includes_route_points_for_each_route`

### `src/result_writer.rs`
- `write_json_serializes_transport_neutral_result`
- `write_gpx_path_uses_transport_neutral_routes`
- `write_error_result_returns_routes_generation_failed`

### `src/router_runner.rs`
- `coords_from_str_parses_valid_lat_lon`
- `coords_from_str_rejects_missing_lon`
- `router_runner_maps_generated_routes_into_transport_neutral_model`

## Validation criteria
- `src/router_runner.rs` no longer imports or constructs `ipc_handler::ResponseMessage` / `RouteMessage` / `RouterResult` for normal CLI routing.
- `src/result_writer.rs` no longer accepts `ResponseMessage`.
- `src/gpx_writer.rs` no longer accepts `RouteMessage`.
- `cargo check` passes.
- No functional change is required yet to the CLI contract.

## Progress checklist
- [ ] Decide final module location for transport-neutral output types.
- [ ] Add transport-neutral route/output model.
- [ ] Refactor `router_runner.rs` to build the new model.
- [ ] Refactor `result_writer.rs` to accept the new model.
- [ ] Refactor `gpx_writer.rs` to accept the new model.
- [ ] Add/update unit tests for router runner mapping.
- [ ] Add/update unit tests for result writing.
- [ ] Add/update unit tests for GPX writing.
- [ ] Run `cargo check`.
- [ ] Confirm no normal route flow depends on IPC output types anymore.

## Done when
The CLI route path is transport-neutral internally, even if dead IPC code still exists elsewhere in the repo.