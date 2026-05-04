# Routing fix: staged border point lookup before cross-tile escalation

## Problem

Routing currently constructs cross-tile endpoint references aggressively and then expects the referenced point record to exist in exactly the chosen tile context.

That is too strict for an overlap-based tile model where:

- points may validly exist in multiple neighboring tiles
- a way may extend slightly into a neighboring tile and return
- the current tile may already contain the needed duplicated endpoint

In those cases, routing should prefer the already loaded current tile view before escalating to the neighboring tile.

## Desired behavior

For border endpoint resolution:

1. first try the point in the current/source tile
2. if not found, try the adjacent/derived neighboring tile
3. if not found in either place, keep the panic because this is still a critical invariant failure during testing

## Proposed fix

Implement staged point lookup for border-connected endpoints.

### Lookup order

When resolving the other endpoint of a line or adjacent edge:

1. try `MapDataPointRef(current_tile, osm_id)`
2. if that point does not exist, try the neighboring tile candidate
3. if neither exists, panic as today

This preserves the strong invariant while matching the intended overlap model better.

## Neighbor tile selection

Two candidate strategies should be considered.

### Option A: derive tile from endpoint coordinates

Use the current tile-size math to derive the neighboring tile from endpoint coordinates.

Pros:

- simple
- deterministic
- already aligned with current routing logic

Cons:

- assumes coordinate-derived ownership is always the best neighbor choice
- may be less robust when overlap duplication means the source tile is the preferred representation

### Option B: use manifest-guided neighbor lookup

Use the manifest neighbor relationship of the current tile to identify the adjacent tile that should be probed.

Pros:

- expresses adjacency explicitly
- can be more semantically correct than raw coordinate ownership for overlap models
- avoids relying entirely on coordinate-derived tile identity

Cons:

- more logic to map endpoint direction to the correct neighbor tile
- still may need coordinate checks for diagonals / exact border cases

## Recommended direction

Start with staged lookup using:

1. current tile first
2. coordinate-derived neighboring tile second

Then evaluate whether manifest-guided neighbor selection would be a better long-term primary mechanism for the second stage.

The key improvement is not the second-stage heuristic by itself. The key improvement is that the current tile gets first chance to satisfy the overlap contract.

## Important constraint

Do not remove the panic yet.

If the point does not exist in either the current tile or the neighboring tile, that is still a critical data-contract violation and should remain loud in testing.

## Expected benefits

- avoids unnecessary neighboring tile loads for short border excursions
- better matches the intended duplicated-border-point model
- remains strict about true data corruption or generation bugs
- complements, but does not replace, the generation-stage fix

## Acceptance criteria

1. If the endpoint point exists in the current tile, routing uses it without loading the neighbor tile.
2. If the endpoint point does not exist in the current tile but exists in the adjacent tile, routing resolves it successfully.
3. If the endpoint point exists in neither tile, routing still panics.
4. Tests cover:
   - duplicated point in both tiles
   - point only in adjacent tile
   - point missing in both tiles
   - short way that crosses a border and returns without requiring unnecessary neighbor loading
