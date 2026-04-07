# Implementation Phase 4: route-generation task-local caches

## Goal
Exploit repeated within-route reuse without adding shared-cache contention.

## Why this is phase 4
After phases 1-3, hydration should already be cheaper. This phase then removes repeated re-hydration of immutable graph objects inside each route-generation task.

## Scope

### In scope
1. Add per-route-generation-task caches, not graph-global caches.
2. Start caches unbounded.
3. Initial cache set:
   - point cache
   - line cache
   - tag-set cache
4. Add tag-value cache only if profiling still shows meaningful string lookup churn.
5. Keep cache ownership local to a single route-generation task / worker.

### Suggested cache keys
- point: `MapDataPointRef` or `(TileId, osm_id)`
- line: `MapDataLineRef` or `(TileId, line_index)`
- tag set: `ElementTagSetRef`
- optional tag value: `ElementTagValueRef`

### Suggested cache values
Start with plain owned values:
- `MapDataPoint`
- `MapDataLine`
- `ElementTagSet`
- optional `String`

Only switch specific caches to `Arc<_>` if clone cost shows up after profiling.

## Likely integration point
Introduce cache state into the per-task routing context used inside Rayon route-generation work, instead of adding shared mutable state to `MapDataGraph`.

## Expected code touch points
- `crates/ridi-router-routing/src/routing_context.rs`
- `crates/ridi-router-routing/src/router/generator.rs`
- possibly navigator / walker call chains if the context API needs mutable or interior-mutable cache access

## Deliverables
- repeated point/line/tag lookups within one route-generation task hit local caches
- no cross-thread cache sharing
- no additional lock contention in the common path
- tests or focused checks proving cache hits preserve identical results

## Validation
1. Run routing tests.
2. Run `cargo check --workspace`.
3. Re-run perf and compare:
   - `graph::get_point_from_tiles`
   - `graph::get_line_from_tiles`
   - `graph::get_tag_set`
   - `graph::get_tag_value`
   - wall-clock route generation time
4. Track RSS and basic cache growth during long runs.

## Exit criteria
- repeated immutable hydrations within one worker are measurably reduced
- no shared-cache synchronization was introduced
- memory growth is acceptable for the current workloads

## Main risk
Because `RoutingContext` is currently a thin immutable wrapper, the cache plumbing may require API reshaping or interior mutability. Keep the cache surface narrow.

## Rollback plan
If wiring all caches at once is noisy, land point cache first, then line cache, then tag caches in separate commits within this phase.


## Implementation notes
- Added task-local `RoutingContext` caches for `MapDataPoint`, `MapDataLine`, and `ElementTagSet`.
- Cache keys are `MapDataPointRef`, `MapDataLineRef`, and `ElementTagSetRef`.
- `Generator` now creates a fresh `RoutingContext` per Rayon itinerary task, so caches stay worker-local and are never shared across threads.
- Caches are intentionally unbounded in this phase.
- `tag_value` caching was left out on purpose, per the phase plan, until profiling shows it is still worth adding.