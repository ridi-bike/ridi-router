# Review: point-record offsets in RMDF generation

## Verdict: partially relevant

## Summary
The todo is still relevant for `lines_offset`, but it is stale for `rules_offset`.

There is still a real bug in the RMDF writer: point records are written with `lines_offset: 0`, even though the line-ref side table is serialized and later read through `PointRecord.lines_offset` + `PointRecord.lines_count`.

By contrast, the `rules_offset` part of the todo is not a small unfinished detail anymore. The rules table is not serialized at all, relation insertion into the generation graph is still a stub, and the routing side currently rebuilds points with `rules: Vec::new()`. So `rules_offset` cannot be fixed meaningfully without a larger turn-restriction pipeline.

## Current evidence from the codebase

1. The todo description matches a live TODO in the writer.
   - `crates/ridi-router-tiles/src/rmdf/generator/writer.rs:228-236`
   - `lines_offset: 0, // TODO: Calculate from line refs`
   - `rules_offset: 0, // TODO: Calculate from rules`

2. Line refs are already serialized as a flattened side table.
   - `crates/ridi-router-tiles/src/rmdf/generator/writer.rs:298-334`
   - `serialize_line_refs()` builds a flat `Vec<u64>` of line indices in the same sorted point order used by `serialize_points()`.
   - That means the missing piece is per-point offset bookkeeping, not the existence of the line-ref section.

3. The runtime reader already depends on real `lines_offset` values.
   - `crates/ridi-router-routing/src/rmdf/tile_manager.rs:177-182`
   - `get_adjacent_by_id()` slices the line-ref array with:
     - `point.lines_offset as usize`
     - `point.lines_count as usize`
   - If every point has `lines_offset == 0`, every point reads from the front of the line-ref table instead of its own range.

4. The RMDF format clearly expects point-local offsets.
   - `crates/ridi-router-common/src/format.rs:77-88`
   - `PointRecord` stores both `lines_offset`/`lines_count` and `rules_offset`/`rules_count`.

5. Rules serialization is not implemented.
   - `crates/ridi-router-tiles/src/rmdf/generator/writer.rs:376-380`
   - `serialize_rules()` currently returns `Ok(Vec::new())` with a TODO comment.

6. Generation-time relation handling is also not implemented.
   - `crates/ridi-router-tiles/src/map_data/generation_graph.rs:153-159`
   - `insert_relation()` explicitly says turn restrictions are skipped for now and returns `Ok(())`.
   - The generator still collects restriction relations and passes them into the graph:
     - `crates/ridi-router-tiles/src/rmdf/generator/mod.rs:560-591`
     - `crates/ridi-router-tiles/src/rmdf/generator/mod.rs:929-933`
   - But that work stops at the stubbed `insert_relation()`.

7. Routing does not yet consume rules from RMDF tiles.
   - `crates/ridi-router-routing/src/map_data/graph.rs:358-366`
   - When reconstructing a `MapDataPoint` from tiles, it sets `rules: Vec::new(), // TODO: Fetch rules from tiles`.

8. Existing synthetic RMDF fixtures already use real `lines_offset` values.
   - `crates/ridi-router-routing/src/routing_api.rs:306-314`
   - `crates/ridi-router-cli/tests/generate_route_cli.rs:191-199`
   - Those tests manually assign `lines_offset: line_refs.len() as u64` as they build point records.
   - That is strong evidence that the intended format behavior is “offset into flattened line-ref array”, not a placeholder.

9. Current tests do not prove the writer is correct here.
   - `crates/ridi-router-tiles/tests/public_api_success.rs:55-57`
   - The only end-to-end public API test that exercises generated tiles is ignored.
   - `cargo test -p ridi-router-tiles` currently passes, but there are no active tile-writer tests asserting point-record offsets or line-ref round-tripping.

## If still relevant

### The actual problem
`RmdfWriter::serialize_points()` writes point records before assigning each point its starting position inside the flattened line-ref table. As a result, generated RMDF point records can claim the right `lines_count` but still point at the wrong slice because `lines_offset` is always zero.

For `rules_offset`, the todo text understates the problem. The issue is not just “write the real offset”; the whole rules pipeline is unfinished.

### What behavior is missing or risky
- Generated RMDF tiles can contain invalid per-point line-ref references.
- Routing code that uses `point.lines_offset` may read the wrong adjacent lines for most points.
- Any future work on rule offsets is blocked by missing rule generation, missing rule serialization, and missing rule loading.
- The todo’s current wording may encourage implementing `rules_offset` as if it were a local writer-only fix, which would be misleading.

### Possible solution options

#### Option A: split the work
1. Fix `lines_offset` now.
2. Defer `rules_offset` into a separate turn-restriction/rules-serialization todo.

#### Option B: do a larger RMDF point/rule pass
Implement in one effort:
- relation insertion into `GenerationGraph`
- flattened rules table generation
- per-point `rules_offset` bookkeeping
- tile loading of rules
- routing use of tile rules

### Tradeoffs / implications
- Option A is smaller, testable, and fixes a real correctness bug immediately.
- Option B is more complete, but much larger than the original todo suggests and crosses crate boundaries (`ridi-router-tiles`, `ridi-router-routing`, and RMDF IO).
- Keeping both concerns in one todo risks mixing a contained writer bug with unfinished turn-restriction support.

### Recommended direction
Treat this as two separate tracks:

1. **Keep and prioritize the `lines_offset` part.**
   - This is a real writer defect with clear evidence and a contained fix.

2. **Rewrite or replace the `rules_offset` part.**
   - The current todo text is stale here.
   - It should be replaced with a broader task about end-to-end turn-restriction support and RMDF rule serialization.

### Open questions
- What exact ordering guarantee should be formalized for points within the same grid cell? Right now both `serialize_points()` and `serialize_line_refs()` use the same grouping/sorting shape, which is good, but it would be safer to centralize that ordering logic.
- Should `lines_offset` remain an index into the `LINE_REFS` array, or be documented more explicitly in the shared RMDF format docs/tests?
- Once rules are implemented, should `rules_offset` also be an index into a flattened `RULES` array, or an offset into a separate side structure for rule-local line lists?

### Suggested implementation starting point
Start in `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`.

Practical first step:
- factor out the current point ordering logic used by both `serialize_points()` and `serialize_line_refs()` into one shared iterator/order builder
- while flattening line refs, record each point’s starting index and count
- use that metadata when building `PointRecord`
- add a test that writes a tiny graph, reloads the file through the existing RMDF reader, and verifies that each point’s `lines_offset`/`lines_count` selects the expected line indices

Do **not** try to “finish” `rules_offset` inside that same change unless you also implement relation ingestion and rule consumption.

## Related files to inspect next
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
- `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
- `crates/ridi-router-common/src/format.rs`
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/routing_api.rs`
- `crates/ridi-router-cli/tests/generate_route_cli.rs`
