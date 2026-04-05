# TODO: Define turn restriction and rule model for RMDF generation

## Goal

Give turn restrictions a concrete path from OSM relations to runtime-usable rule data.

## Problem

`insert_relation(...)` in generation code still skips relation processing.

Without a concrete model, turn restrictions cannot be generated, serialized, and loaded back consistently.

## Plan

- decide the in-memory shape for relation-derived routing restrictions
- store that data in `GenerationGraph`
- connect RMDF writing to the same structure
- add at least one end-to-end restriction fixture

## Acceptance criteria

- relation-derived restrictions survive generation and load
- runtime code can consume the generated rule data
- tiles and routing tests pass

## Suggested checks

- `cargo test -p ridi-router-tiles`
- `cargo test -p ridi-router-routing`
