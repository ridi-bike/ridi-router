# Restore green workspace tests

## Status

Resolved.

## Resolution

- CLI tests now use an existing rule preset (`rule-examples/rules-default.json`).
- Tile-generation CLI success tests now use a tiny checked-in PBF fixture instead of the large Montenegro extract, keeping `cargo test --workspace` within normal test time.

## Verification

- `cargo test -p ridi-router-cli --test generate_tiles_cli -- --nocapture`
- `cargo test --workspace`
