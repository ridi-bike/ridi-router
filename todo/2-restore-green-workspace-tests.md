# Restore green workspace tests

## Problem

`cargo test --workspace` currently fails in CLI tests because they try to load the missing `rules-fast.json` file.

## Evidence

Failing tests:
- `generate_route_missing_manifest_fails`
- `generate_route_invalid_tiles_dir_fails`

Manual verification showed the CLI returns the expected tile-manifest errors when run with a valid existing rule file.

## Why it matters

The workspace test suite is not green, which makes regressions harder to trust and slows down development.

## Suggested fix

Fix the stale rule preset references first, then rerun and stabilize the full workspace test suite.
