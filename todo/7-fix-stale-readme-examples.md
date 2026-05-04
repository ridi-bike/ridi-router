# Fix stale README examples

## Status

Resolved.

## Resolution

- README route examples now use the existing `rule-examples/rules-default.json` preset.
- The round-trip example now lists `--distance 100000` only once.

## Verification

- `rg -n "rules-fast|distance 100000" README.md`
