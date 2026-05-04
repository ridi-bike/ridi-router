# Clean up clippy warnings

## Status

Resolved.

## Resolution

- Elided the needless lifetime in `TileManager::slice_rule_line_refs`.
- Refactored the overlap manifest test helper to group per-tile metadata, avoiding the `too_many_arguments` warning.

## Verification

- `cargo clippy --workspace --all-targets`
