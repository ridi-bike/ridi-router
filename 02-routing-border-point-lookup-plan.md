# Implementation plan — staged border point lookup before cross-tile escalation

## Overview

```text
adjacent line lookup in current tile
  -> determine other endpoint osm_id + coords
  -> try endpoint in current tile first
  -> if missing, derive neighbor tile from coords
  -> try endpoint in derived neighbor tile
  -> if still missing, keep panic / invariant failure
```

## Goal

Prefer the already loaded current tile view for border endpoint resolution, while preserving the existing loud failure when the endpoint exists in neither tile.

## Current contract failure

1. `TileManager::get_adjacent_by_id(...)` derives the other endpoint's tile immediately from endpoint coordinates.
2. It returns `MapDataPointRef::new(other_tile_id, other_osm_id)` without first checking whether the current tile already contains the duplicated endpoint.
3. `MapDataGraph` and `RoutingContext` later hydrate that exact point ref.
4. If the point exists only in the current tile's overlapped representation, hydration follows the derived neighbor ref and the lookup fails loudly.
5. That is stricter than the intended overlap contract, where duplicated border points may validly live in both tiles and current-tile resolution should win.

## Implementation strategy

## 1. Introduce one staged endpoint-resolution helper in `TileManager`

Add a small helper used by adjacency lookup that accepts:

1. current tile id
2. other endpoint osm id
3. other endpoint coordinates
4. whether neighbor loading is allowed

Responsibilities:

1. probe `current_tile` first for `other_osm_id`
2. if found, return `MapDataPointRef::new(current_tile, other_osm_id)`
3. otherwise derive `neighbor_tile` from endpoint coordinates
4. if `neighbor_tile == current_tile`, return the current-tile ref directly
5. otherwise perform the existing neighbor-availability / neighbor-loading path
6. if found in neighbor, return `MapDataPointRef::new(neighbor_tile, other_osm_id)`
7. if found in neither place, preserve the current invariant failure behavior

Reason:

The key behavior change is lookup order, not a broader routing redesign.

## 2. Refactor `get_adjacent_by_id(...)` to use staged resolution

Replace the inline endpoint tile selection in `get_adjacent_by_id(...)` with the helper.

Responsibilities:

1. keep current line iteration and dead-end filtering behavior
2. keep coordinate-derived neighbor selection as the second-stage heuristic
3. keep `is_tile_available(...)` handling for true neighbor traversal cases
4. keep `ensure_tile_loaded(...)` only for the second stage
5. return adjacency refs whose endpoint tile id reflects the tile where the point record was actually found

Important detail:

Do not load a neighbor tile if the endpoint already exists in the current tile.

## 3. Refactor `get_adjacent_by_id_if_loaded(...)` to use the same staged logic

Apply the same lookup order to the cold-path-safe variant.

Responsibilities:

1. keep returning `Ok(None)` when the center tile is not loaded
2. keep returning `Ok(None)` when staged lookup would require a cold neighbor tile
3. return `Some(...)` when the current tile alone can satisfy the endpoint lookup
4. return `Some(...)` when both required tiles are already warm

Reason:

`MapDataGraph::get_adjacent(...)` first tries `get_adjacent_by_id_if_loaded(...)`, so the staged behavior must be consistent in both paths.

## 4. Preserve the current invariant failure shape

Do not soften the failure yet.

Implementation notes:

1. if current-tile lookup fails and neighbor lookup also fails, keep the existing panic/error path loud in tests
2. keep missing-neighbor-tile dead-end filtering only for the case where the manifest truly lacks the derived neighbor tile
3. distinguish "neighbor tile absent" from "neighbor tile present but endpoint missing in both tiles"

Reason:

This change is a routing lookup correction, not a relaxation of data integrity checks.

## 5. Keep neighbor selection coordinate-derived for now

Use the existing `TileId::from_coords(...)` / `tile_id_for_coords(...)` math for stage two.

Notes:

1. do not add manifest-guided directional neighbor mapping in this change
2. isolate the second-stage tile selection behind the new helper so manifest-guided lookup can replace it later
3. document in comments that current-tile-first is the contract, while neighbor selection remains a heuristic

## Main touchpoints

- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/routing_context.rs`
- `crates/ridi-router-routing/src/test_utils.rs`
- `crates/ridi-router-test-support/src/lib.rs`

## Recommended rollout

## Phase 1 — factor staged endpoint resolution

1. add a helper in `TileManager` for current-first endpoint lookup
2. keep the helper private to the tile manager initially
3. route both adjacency methods through it

## Phase 2 — preserve behavioral parity outside the border case

1. keep existing dead-end behavior for manifest-missing neighbors
2. keep existing line refs anchored to the source tile's line index
3. keep existing cache and hydration flow in `MapDataGraph` and `RoutingContext`

## Phase 3 — add focused tests

1. current tile contains duplicated border point and neighbor stays cold
2. current tile misses the point but warm/cold neighbor lookup succeeds as today
3. point missing in both tiles still fails loudly
4. short cross-border-returning way does not trigger unnecessary neighbor loading

## Test plan

## Unit coverage

1. `get_adjacent_by_id(...)` returns the current-tile point ref when the endpoint exists in both tiles.
2. `get_adjacent_by_id(...)` loads and uses the derived neighbor when the endpoint is absent from the current tile.
3. `get_adjacent_by_id_if_loaded(...)` succeeds without loading the neighbor when the current tile already has the duplicated endpoint.
4. `get_adjacent_by_id_if_loaded(...)` returns `None` when the current tile lacks the point and the needed neighbor tile is still cold.
5. `get_adjacent_by_id(...)` still filters out lines whose derived neighbor tile is absent from the manifest.
6. staged lookup still fails loudly when the endpoint exists in neither current nor neighbor tile.

## Integration coverage

1. `MapDataGraph::get_adjacent(...)` exposes the current-first behavior through the existing tile-manager-backed graph path.
2. `RoutingContext::adjacent(...)` caches the staged result without changing visible behavior for non-border cases.
3. A synthetic cross-tile fixture demonstrates that a short border excursion can resolve fully inside the source tile when the duplicated endpoint is present there.

## Acceptance mapping

1. Endpoint exists in current tile: covered by Phase 3 test 1 and unit coverage 1/3.
2. Endpoint exists only in adjacent tile: covered by Phase 3 test 2 and unit coverage 2/4.
3. Endpoint exists in neither tile: covered by Phase 3 test 3 and unit coverage 6.
4. Short border-crossing-returning way avoids unnecessary neighbor loading: covered by Phase 3 test 4 and integration coverage 3.

## Non-goals

1. Do not remove the panic / loud invariant failure.
2. Do not switch to manifest-guided neighbor selection yet.
3. Do not change tile generation in this follow-up.
4. Do not broaden this into a general multi-tile search beyond current tile plus one derived neighbor.
