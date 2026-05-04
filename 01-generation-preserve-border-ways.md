# Generation fix: preserve border ways without breaking them across tiles

## Problem

The generation pipeline currently extracts nodes, ways, and relations using buffered tile bounds, but the final routable graph is still effectively clipped by local node availability.

A way may be selected for a tile, yet its line segments are only emitted when both endpoint nodes are present in that tile's local node set. This can break a routable way at the tile edge even when the overall structure was intentionally included by overlap logic.

That creates a bad final-state contract:

1. a line or connected structure is partially present in one tile
2. a border point may exist only in a neighboring tile's final RMDF output
3. routing later derives a point reference for that border endpoint and cannot resolve it consistently

## Desired invariant

If a way is included in a tile for routing purposes, its locally relevant connected structure must remain routable within that tile's overlapped representation.

More concretely:

- points may exist in multiple neighboring tiles
- ways may exist in multiple neighboring tiles
- restriction-supporting data may exist in multiple neighboring tiles when needed
- border overlap must survive into final RMDF, not just extraction

## Proposed fix

Preserve duplicated border data in final tile output instead of letting local clipping remove it.

### Point policy

Include point records for border-overlap nodes even when they are duplicated in neighboring tiles.

### Way policy

When a way is admitted into a tile because it intersects buffered bounds, do not break it purely because one of its neighboring points lies outside the canonical tile box.

Instead, the tile's overlapped representation should include the required endpoint points and the corresponding line segments so the way remains connected through the overlap zone.

### Relation / restriction policy

Apply the same principle to restriction-supporting structures:

- if a restriction is meant to be routable near a border
- and its supporting via/from/to local structure is within the overlap zone
- then the supporting point and line data should be duplicated into the tile as needed

The goal is not global duplication of all distant members. The goal is preserving routable local continuity across the overlap zone.

## Scope boundary

This fix is about the final RMDF routing representation, not only extraction.

It is acceptable for the extraction stage to over-collect, as long as the final emitted tile preserves the overlapped routable structure needed at borders.

## Why this should fix the observed panic

The current panic happens when routing follows a line endpoint and cannot find the matching point record in the tile it tries to resolve.

If border-connected ways are preserved in overlapped final output, the relevant endpoint point record will exist in both participating neighboring tiles, and the lookup contract will hold.

## Acceptance criteria

1. A point near a tile boundary may appear in two neighboring RMDF tiles.
2. A way crossing slightly into a neighboring tile and back remains connected in both neighboring overlapped routing tiles.
3. Border-connected line endpoints always have corresponding point records in the tile representation used for routing.
4. Route generation no longer produces missing-point panics for overlap-preserving border cases.
5. Tests should include real duplicated border-point cases and short cross-boundary-returning ways.
