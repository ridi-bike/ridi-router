# Implementation phase 2 — RMDF writer offsets and rule serialization

## Goal
Serialize the generation-side restrictions into RMDF using the existing `RULES` section and make point offsets real.

## Why this is phase 2
Once generation has concrete rules, the writer becomes the bridge from generation data to runtime data.

## Scope
Finish writer-side offset bookkeeping for both:
- point -> line refs
- point -> rule records

Then serialize rules as:
1. `RuleRecord[]`
2. trailing flattened `u64[]` line-index payload

## Main changes

### 1. Introduce one stable point ordering helper
`crates/ridi-router-tiles/src/rmdf/generator/writer.rs` currently repeats the same point grouping/sorting logic across:
- spatial index
- points
- line refs

This phase should centralize that ordering so all offsets are computed from the exact same point sequence.

### 2. Make `PointRecord.lines_offset` real
`serialize_points(...)` currently writes `lines_offset: 0`.

Compute prefix offsets into the flattened `LINE_REFS` payload based on the same point order used by `serialize_line_refs(...)`.

### 3. Make `PointRecord.rules_offset` / `rules_count` real
For each serialized point:
- find `restrictions_by_via[point.osm_id]`
- write the rule-record start offset within the `RuleRecord[]` prefix
- write the number of rules for that point

Points without rules keep zero count.

### 4. Implement `serialize_rules(...)`
Replace the stub with serialization of:
- `RuleRecord` entries
- trailing flattened `u64` payload containing `from` then `to` line-index slices

Each `RuleRecord` should store offsets/counts into the trailing payload, not into the start of the full section.

### 5. Fix header accounting
Important subtlety: once the `RULES` section contains `RuleRecord[] + payload`, this is no longer valid:
- `rule_count = rules.len() / size_of::<RuleRecord>()`

Instead, track `rule_record_count` separately from total rules-section bytes.

## Suggested implementation notes
- Return richer data from writer internals if needed, for example:
  - `serialize_points(...) -> (Vec<u8>, PointLayout)`
  - `serialize_rules(...) -> SerializedRules { bytes, rule_count }`
- Keep line-ref payload values as tile-local generated line indices.
- Do not add a new RMDF section for rule line refs.

## Tests for this phase
Add writer-focused tests that prove:
- `PointRecord.lines_offset` and `lines_count` point to the correct slice in `LINE_REFS`
- `PointRecord.rules_offset` and `rules_count` point to the correct `RuleRecord[]` slice
- `RuleRecord.from_lines_offset` / `to_lines_offset` slice the trailing payload correctly
- `header.rule_count` equals the number of `RuleRecord`s, not the total byte size
- points without rules still serialize correctly

## Done when
- point line offsets are real
- point rule offsets are real
- the `RULES` section contains serialized `RuleRecord[] + flattened u64 payload`
- RMDF header counts remain correct after payload append

## Likely files
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
- writer tests in the same module
