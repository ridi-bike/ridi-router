# Revisit repeated-point route-history compatibility behavior

## Status

Resolved.

## Resolution

- Removed first-occurrence compatibility behavior for repeated route-history points.
- `since_point` route-history lookups now use the latest matching occurrence.
- Removed test-only split/query helpers that encoded the old first-match behavior.
- Added/updated regression tests for repeated boundary points in loop detection, junction lookups, and progression weighting.

## Verification

- `cargo test -p ridi-router-routing route::`
- `cargo test -p ridi-router-routing router::weights::phase1_tests::repeated_boundary_point_uses_latest_match_in_route_history`
- `cargo test -p ridi-router-routing`
