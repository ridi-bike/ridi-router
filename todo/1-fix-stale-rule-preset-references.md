# Fix stale rule preset references

## Problem

The repo still references `rule-examples/rules-fast.json`, but that file does not exist.

## Evidence

- `README.md` uses `rule-examples/rules-fast.json`
- Tests also hardcode the missing file:
  - `crates/ridi-router-cli/tests/generate_route_cli.rs`
  - `crates/ridi-router-cli/tests/generate_tiles_cli.rs`
- Existing presets are:
  - `rule-examples/rules-default.json`
  - `rule-examples/rules-prefer-unpaved.json`
  - `rule-examples/rules-avoid-unpaved.json`

## Why it matters

This breaks examples, causes confusion, and cascades into failing tests.

## Suggested fix

Replace `rules-fast.json` references with a real preset, or restore that preset if it is still intended to exist.
