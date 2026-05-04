# Clean up clippy warnings

## Problem

The workspace still has clippy warnings.

## Evidence

- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`: needless lifetime
- `crates/ridi-router-routing/src/map_data/graph.rs`: too many arguments in helper

## Why it matters

Warnings add noise and make it easier for new issues to hide in CI output.

## Suggested fix

Apply the simple lifetime cleanup and either refactor the helper signature or explicitly allow it if the current shape is intentional.
