# Implementation Phase 3: read-mostly loaded-tile access for point, line, and tag readers

## Goal
Stop serializing loaded-tile hits through a write lock.

## Why this is phase 3
This is the first concurrency-shape change. It should come after the point/rule hydration waste is reduced, so the new read path is simpler and easier to reason about.

## Scope

### In scope
1. Split tile access into:
   - read-only fast path for already loaded tiles
   - write path only for load / miss / eviction work
2. Replace exact per-hit LRU mutation with approximate recency metadata suitable for read hits, for example:
   - `last_used: AtomicU64`
   - or similar cheap per-tile recency state
3. Apply the read-mostly pattern first to point reads.
4. Extend the same pattern to nearby hot readers where it stays clean:
   - `get_line_from_tiles`
   - `get_tag_set`
   - `get_tag_value`
5. Keep eviction decisions under the write lock.

### Explicitly out of scope
- shared graph-level caches
- exact per-hit global LRU ordering
- route-generation task-local caches

## Expected code touch points
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- possibly tile registry / loaded-tile bookkeeping structs

## Design target
Common path:
1. take read lock
2. if tile already loaded, hydrate directly and atomically bump recency
3. if tile missing, drop read lock
4. take write lock, load tile, update eviction metadata, retry

## Deliverables
- loaded-tile point reads no longer require the write lock
- line and tag reads follow the same loaded-hit pattern where practical
- eviction remains roughly recency-based, not exact LRU
- tests covering repeated hits, misses, and basic concurrent access safety

## Validation
1. Run `cargo check --workspace` and full routing tests.
2. Add or run concurrency-focused tests for repeated loaded-tile hits.
3. Re-run perf and compare:
   - wall-clock route generation time
   - `graph::get_point_from_tiles`
   - `graph::get_line_from_tiles`
   - `graph::get_tag_set`
   - `tile_manager` lock-heavy helpers
4. Watch for regressions in tile miss handling and eviction behavior.

## Exit criteria
- common loaded-tile reads are read-mostly
- Rayon workers are no longer forced through one write path on every hit
- approximate recency is in place and eviction still behaves reasonably

## Main risk
This phase changes synchronization behavior. The biggest risk is making loaded-tile reads fast but subtly unsafe or making eviction bookkeeping inconsistent.

## Rollback plan
If extending to line/tag reads is too broad, land point-read fast path first, then extend the same pattern in follow-up commits inside this phase.
