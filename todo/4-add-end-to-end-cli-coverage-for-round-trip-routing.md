# Add end-to-end CLI coverage for round-trip routing

## Problem

Round-trip mode exists, but CLI integration tests only cover `start-finish` flows.

## Evidence

- Round-trip mode exists and is tested at library level.
- CLI integration tests currently cover only `start-finish` flows.

## Why it matters

The CLI exposes round-trip routing as a user-facing feature, but the end-to-end path is under-tested.

## Suggested fix

Add CLI integration tests for round-trip routing using synthetic tiles and stable rule fixtures.
