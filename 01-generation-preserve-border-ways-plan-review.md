# Review — 01-generation-preserve-border-ways-plan

## Verdict
Mostly implemented.

Phases 1-3 are substantially in place. Phase 4 is only partially complete because the new graph-level invariant check exists but is not wired into either write path.

## What matches the plan

### 1. Shared overlap-materialization step
Implemented.

- New shared helper: `crates/ridi-router-tiles/src/rmdf/generator/tile_materializer.rs`
- Single-PBF path now uses it from `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
- Multi-PBF path now uses it from `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`

This removes the duplicated extraction policy the plan called out.

### 2. Direct ID lookups on `InMemoryPbf`
Implemented.

- `node_by_osm_id(...)`
- `way_by_osm_id(...)`

Added in `crates/ridi-router-tiles/src/osm_data/in_memory_pbf.rs`.

### 3. Segment-aware way admission
Implemented.

`materialize_locally_relevant_way(...)` now:
1. walks consecutive node pairs
2. resolves endpoints from the global PBF store
3. keeps a way when at least one segment intersects buffered bounds
4. materializes the needed endpoint nodes

This replaces the old `has_nodes_in_tile` behavior the plan wanted removed.

### 4. Preserve border nodes without duplicating full remote tails
Implemented.

The materializer inserts only endpoints for locally relevant segments and keeps the original way record. It does not pull in the entire distant tail of a long way.

There is also a direct unit test for this in `tile_materializer.rs`.

### 5. Restriction support rebuilt after closure expansion
Largely implemented.

- restriction relations are re-scanned after node/way expansion
- via node support is materialized
- from/to ways are pulled in through the same local-segment materialization logic
- relations are collected after expansion, not only from the initial seed

This is the right shape for the plan.

## Gaps

### 1. Post-materialization / pre-writer graph validation is not wired
Partial only.

`GenerationGraph::validate_line_endpoints(...)` was added in `crates/ridi-router-tiles/src/generation/graph.rs`, but neither generator path calls it before `write_tile_from_graph(...)`.

Current write paths still go straight from graph build to writer:
- `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`

Evidence: `cargo test` emits a dead-code warning for `validate_line_endpoints`, which confirms it is currently unused.

Impact: the branch adds the validation helper the plan wanted, but not the actual invariant enforcement step.

### 2. Restriction relation admission is still permissive
Potential follow-up.

`relation_has_members_in_closure(...)` keeps a relation when **any** member is in closure. That is close to the old heuristic.

The later graph build still skips unresolved relations, so this is not necessarily wrong, but it is weaker than a stricter “fully supported after expansion” interpretation of the plan.

Impact: behavior is probably acceptable, but unresolved restrictions still depend on downstream skip logic rather than a tighter closure check.

### 3. Test coverage does not fully match the written test plan
Partial.

Covered well:
- sparse border-crossing way remains connected
- long remote tail is not duplicated
- border restriction support is preserved
- single and multi generation now match on border fixture output
- routing no longer panics on the reproduced border case

Still missing or not explicit:
- a way that crosses into a neighboring tile and returns remains connected in both neighboring tiles
- explicit test that unresolved restriction skip behavior still fires after expansion
- explicit assertion that writer no longer reaches `MissingPoint` for the preserved-overlap case

## Overall assessment
The branch captures the core design of the plan and appears to fix the main border-way clipping problem in both generation paths.

The main remaining issue is Phase 4: the invariant check exists, but it is not enforced before RMDF writing. I would treat that as the primary follow-up before calling the plan fully complete.

## Recommended next changes
1. Call `graph.validate_line_endpoints()?` in both write paths before `write_tile_from_graph(...)`.
2. Add the missing regression tests from the plan, especially:
   - cross-boundary-returning way in both neighboring tiles
   - unresolved restriction skip after expansion
   - explicit no-`MissingPoint` writer regression
3. Optionally tighten restriction admission so relations are kept only when their required support is actually resolved after expansion.

## Review basis
Reviewed against the current working tree on branch `ridi-v2`, including these branch changes:
- `crates/ridi-router-tiles/src/rmdf/generator/tile_materializer.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
- `crates/ridi-router-tiles/src/osm_data/in_memory_pbf.rs`
- `crates/ridi-router-tiles/src/generation/graph.rs`
