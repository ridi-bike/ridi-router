# Implementation plan — preserve border ways in final RMDF tiles

## Overview

```text
buffered tile query
  -> select candidate highway ways + restriction relations
  -> materialize overlap-preserving local closure
       - keep locally relevant segments
       - duplicate required border endpoint nodes
       - duplicate restriction-supporting local structure
  -> build GenerationGraph
  -> write RMDF
```

## Goal

Preserve routable border continuity in the final RMDF tile, not just during extraction.

## Current contract failure

1. Tile extraction already queries buffered bounds.
2. A way can be admitted to a tile even when some of its referenced nodes are outside the tile-local node set.
3. `GenerationGraph::insert_way(...)` only emits a line when both endpoint nodes already exist.
4. Missing endpoint nodes silently clip border segments.
5. The RMDF writer and routing layer later assume every emitted line endpoint has a matching point record.

That failure exists in both generation paths:

- single-PBF generation via `PbfStreamer`
- directory / multi-PBF generation via `MultiPbfGenerator`

## Implementation strategy

## 1. Introduce one shared overlap-materialization step

Add a shared helper that runs after buffered spatial queries and before graph construction.

Responsibilities:

1. accept buffered tile bounds plus the candidate nodes, ways, and relations
2. compute which way segments are locally relevant for this tile
3. fetch any missing endpoint nodes needed to keep those segments connected
4. re-evaluate restriction support against the expanded local closure
5. return a tile payload whose final graph can be written without border clipping

Reason:

Both `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs` and `crates/ridi-router-tiles/src/rmdf/generator/mod.rs` currently duplicate the same extraction policy. The fix should live in one place so single-file and multi-file generation stay aligned.

## 2. Add direct ID lookups to the in-memory PBF store

Add small read-only helpers on `InMemoryPbf` for at least:

- node by OSM ID
- way by OSM ID if needed for restriction support

Reason:

The store already keeps `nodes_by_id`, `ways_by_id`, and `relations_by_id`. The overlap-materialization step needs direct endpoint lookup when a selected way references a border node outside the initial buffered node query result.

## 3. Make way admission segment-aware instead of node-presence-aware

Replace the current admission rule:

- keep way if `has_nodes_in_tile`

with a local-structure rule:

1. iterate each consecutive node pair in the way
2. resolve endpoint coordinates from the global node store
3. mark a segment as locally relevant when it intersects the buffered tile bounds
4. keep the way if it has at least one locally relevant segment

Reason:

The current `has_nodes_in_tile` check can still reject or under-materialize a way whose geometry is relevant to the tile but whose endpoint distribution is sparse.

## 4. Materialize required border nodes for every locally relevant segment

For every locally relevant segment:

1. ensure both endpoint nodes are present in the tile node map
2. duplicate missing endpoint nodes into the tile even if they fall outside the canonical tile box
3. keep the original way record

Important detail:

Do not duplicate the entire remote tail of a long way. Only duplicate the nodes required to support the contiguous local segment run that intersects the buffered overlap zone.

Why this works with the current graph builder:

`GenerationGraph::insert_way(...)` already skips segments whose endpoints are absent. If the tile node set includes the full local overlap run plus its boundary endpoints, the graph builder can keep the local connected portion without needing to split the original OSM way object.

## 5. Rebuild restriction support from the expanded local closure

After the way/node closure is expanded:

1. rebuild the tile-local `way_ids` and `node_ids`
2. filter restriction relations against the expanded closure, not the pre-expansion seed
3. ensure the via node and its via-adjacent from/to line structure are materialized when that structure is within the overlap zone
4. keep current skip behavior only for relations that are still unresolved after expansion

Reason:

The same clipping problem that breaks border ways can also prevent `from` / `to` / `via` support from materializing near tile borders.

## 6. Add an explicit post-materialization invariant check

Before writing RMDF, validate:

1. every emitted line endpoint exists in the graph point map
2. every kept border-connected local segment has both endpoint point records available

This can be a dedicated validation helper or stronger debug/test assertions around graph construction.

Reason:

Today the system fails late in the RMDF writer or during routing. The generator should detect broken local closure earlier.

## Main touchpoints

- `crates/ridi-router-tiles/src/osm_data/in_memory_pbf.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/pbf_streamer.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/mod.rs`
- `crates/ridi-router-tiles/src/generation/graph.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`

## Recommended rollout

## Phase 1 — factor the shared closure logic

1. create a shared overlap-materialization helper
2. route both generator paths through it
3. add `InMemoryPbf` ID lookup helpers

## Phase 2 — preserve border way continuity

1. replace node-presence-based way admission
2. detect locally relevant segments
3. duplicate missing border endpoint nodes
4. confirm graph construction preserves local connected runs

## Phase 3 — preserve border restriction continuity

1. re-filter relations after closure expansion
2. materialize needed via/from/to local structure
3. keep unresolved-only skips loud in logs/tests

## Phase 4 — tighten validation

1. add pre-writer graph validation
2. add stats or warnings for clipped segments that still occur
3. fail tests on unexpected orphaned local line endpoints

## Test plan

## Unit coverage

1. A way with one endpoint inside the buffered bounds and the other just outside remains connected in the generated graph.
2. A short way that crosses into a neighboring tile and returns remains connected in both neighboring overlapped tiles.
3. A long way with a distant tail does not cause full-way duplication outside the local overlap run.
4. A restriction near a border keeps the needed via/from/to local structure after expansion.
5. Relation skip behavior still triggers for truly unresolved relations.

## Integration coverage

1. Single-PBF generation produces duplicated border points where needed.
2. Multi-PBF generation produces the same border-preserving behavior.
3. RMDF writing no longer hits `MissingPoint` for overlap-preserving border fixtures.
4. Routing no longer panics for the reproduced border case.

## Acceptance mapping

1. Duplicated border points in neighboring tiles: covered by Phase 2 tests.
2. Cross-boundary-returning way remains connected in both tiles: covered by Phase 2 + integration tests.
3. Border-connected line endpoints always have point records: covered by Phase 4 validation.
4. Missing-point panic disappears for preserved-overlap cases: covered by integration routing test.
5. Real duplicated-border fixtures exist: covered by new generator and routing fixtures.

## Non-goals

1. Do not globally duplicate all nodes of every selected way.
2. Do not relax routing invariants as part of this change.
3. Do not fold in the staged routing fallback from `02-routing-border-point-lookup.md`; that remains a separate follow-up.
