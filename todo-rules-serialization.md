# TODO: Implement RMDF rules serialization

## Goal

Replace the stubbed `serialize_rules(...)` path with real rule serialization.

## Problem

`serialize_rules(...)` currently returns an empty byte vector.

That blocks full tile-backed rule loading and keeps runtime rule support incomplete.

## Plan

- define the serialized RMDF shape for `MapDataRule`
- serialize `from_lines`, `to_lines`, and `rule_type`
- implement or connect the corresponding read path
- add round-trip tests

## Acceptance criteria

- rules are serialized into RMDF output
- runtime code can read them back
- runtime no longer depends on placeholder empty rule vectors
- tiles and routing tests pass

## Suggested checks

- `cargo test -p ridi-router-tiles`
- `cargo test -p ridi-router-routing`
- rule round-trip fixture tests
