# IPC Refactor Phase 3

## Goal
Delete the dead IPC transport implementation and remove the remaining IPC-specific runtime/dependency/docs surface.

## In scope
- Delete `src/ipc_handler.rs`.
- Remove `interprocess` from `Cargo.toml`.
- Remove IPC-specific module imports and error variants.
- Remove stale server/client documentation and helper commands.

## Out of scope
- Global `MapDataGraph` removal.
- NDJSON implementation.
- Library extraction.

## Files to change
### Primary
- `src/ipc_handler.rs`
  - delete file
- `src/main.rs`
  - remove `mod ipc_handler;`
- `src/router_runner.rs`
  - remove `IpcHandlerError` and `RouterRunnerError::Ipc`
  - remove any remaining IPC imports/comments
- `Cargo.toml`
  - remove `interprocess`

### Repo cleanup
- `README.md`
  - remove stale server/client references
  - remove or rewrite outdated route examples
- `justfile`
  - remove `start-server` / `start-client` recipes
  - remove other obviously stale IPC-era commands

## Implementation details
1. Confirm phases 1 and 2 left no normal route code depending on `ipc_handler`.
2. Delete `src/ipc_handler.rs`.
3. Remove module declaration from `src/main.rs`.
4. Remove `interprocess` dependency.
5. Remove IPC error variants from application errors.
6. Clean repo docs/scripts that still describe server/client behavior.

## Unit tests to write
### `src/router_runner.rs`
- `router_runner_error_no_longer_contains_ipc_variant`
- `generate_route_path_compiles_without_ipc_types`

### `src/main.rs` or compile-oriented test coverage
- keep or add a smoke test proving crate modules compile without `ipc_handler`

### Doc/script validation helpers
These are not traditional unit tests, but add lightweight repo checks where practical:
- grep-based CI check or test script that fails on `start-server`
- grep-based CI check or test script that fails on `start-client`
- grep-based CI check or test script that fails on `interprocess` in `Cargo.toml`

## Validation criteria
- `src/ipc_handler.rs` is gone.
- `src/main.rs` no longer declares `mod ipc_handler;`.
- `Cargo.toml` no longer depends on `interprocess`.
- `src/router_runner.rs` has no IPC-specific error type/variant left.
- `README.md` and `justfile` no longer describe server/client usage.
- `cargo check` passes.

## Progress checklist
- [ ] Verify phases 1 and 2 removed runtime IPC usage.
- [ ] Delete `src/ipc_handler.rs`.
- [ ] Remove `mod ipc_handler;` from `src/main.rs`.
- [ ] Remove `interprocess` from `Cargo.toml`.
- [ ] Remove IPC-specific error variants/imports from `src/router_runner.rs`.
- [ ] Clean stale IPC docs in `README.md`.
- [ ] Clean stale IPC recipes in `justfile`.
- [ ] Add/update unit/compile checks.
- [ ] Run `cargo check`.
- [ ] Run repo grep checks for stale IPC terms.

## Done when
The repo no longer contains active IPC transport code or presents IPC/server-client behavior as a supported architecture.