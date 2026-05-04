# Add end-to-end CLI coverage for round-trip routing

## Status

Resolved.

## Resolution

- Added deterministic synthetic round-trip RMDF fixture support.
- Added end-to-end `generate-route round-trip` CLI coverage for JSON output.
- Added end-to-end `generate-route round-trip` CLI coverage for GPX output.

## Verification

- `cargo test -p ridi-router-cli --test generate_route_cli`
