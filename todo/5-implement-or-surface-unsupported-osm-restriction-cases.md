# Implement or surface unsupported OSM restriction cases

## Problem

Tile generation currently skips several OSM restriction forms.

## Evidence

Skipped restriction types:
- conditional restrictions
- `except=*`
- `restriction:*` variants
- via-way restrictions

Relevant code:
- `crates/ridi-router-tiles/src/generation/graph.rs`
- `crates/ridi-router-tiles/src/generation/restriction.rs`

Today these are logged, but not surfaced strongly as user-facing generation limitations.

## Why it matters

Generated tiles can miss real-world legal routing constraints without making that limitation obvious enough.

## Suggested fix

Either implement support for the skipped restriction classes, or expose the skipped counts clearly in generation output and documentation.


## Resolution

- `except=*` is now evaluated for the motorcycle profile. Restrictions are ignored when `except` contains `motorcycle`, `motor_vehicle`, or `vehicle`; otherwise they are applied.
- `restriction:motorcycle`, `restriction:motor_vehicle`, and `restriction:vehicle` variants are now materialized.
- Matching conditional variants are parsed and applied as unconditional motorcycle restrictions.
- Via-way restrictions remain unsupported and are ignored with a warning/count because generated RMDF restrictions are keyed by a single via node.
