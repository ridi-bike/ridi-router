# Fix stale README examples

## Problem

The README contains stale and incorrect examples.

## Evidence

- It references `rule-examples/rules-fast.json`, which does not exist.
- The round-trip example duplicates `--distance 100000`.

## Why it matters

Broken docs mislead users and make the project look less reliable than it is.

## Suggested fix

Update the README to use an existing rule preset and clean up the duplicated round-trip argument.
