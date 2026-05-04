# Replace skipped fixture-based tests with reliable coverage

## Problem

Several tests silently skip when external fixtures are unavailable.

## Evidence

Skipped CLI fixture tests:
- `crates/ridi-router-cli/tests/generate_route_cli.rs`
- `crates/ridi-router-cli/tests/generate_tiles_cli.rs`

Skipped routing tests depend on missing `test_data/montenegro_tiles`:
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

Heavy public API test is ignored:
- `crates/ridi-router-tiles/tests/public_api_success.rs`

## Why it matters

Real integration coverage is weaker than it appears, especially for data-dependent behavior.

## Suggested fix

Prefer synthetic fixtures checked into the repo, or make fixture generation deterministic and part of test setup.
