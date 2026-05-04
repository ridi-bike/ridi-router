# Replace skipped fixture-based tests with reliable coverage

## Status

Resolved.

## Resolution

- `generate_route_cli` now uses deterministic synthetic RMDF tiles and rules instead of optional `map-data/output`.
- `generate_tiles_cli` now fails clearly if the checked-in tiny PBF fixture is missing instead of silently skipping.
- `tile_manager` tests now use synthetic RMDF fixtures instead of optional external Montenegro tiles.
- `public_api_success` now runs by default against the checked-in tiny PBF fixture instead of an ignored large Latvia fixture.

## Verification

- `cargo test -p ridi-router-cli --test generate_route_cli`
- `cargo test -p ridi-router-cli --test generate_tiles_cli`
- `cargo test -p ridi-router-routing rmdf::tile_manager`
- `cargo test -p ridi-router-tiles --test public_api_success`