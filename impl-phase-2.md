# Implementation Phase 2: Add borrowed `RoutingContext` accessors

## Goal
Introduce non-cloning `RoutingContext` accessors for hot-path read access, without changing routing behavior.

## Why this phase exists
The current hot path clones `MapDataPoint` and adjacency data even on cache hits. Later phases need borrowed access so the walker and look-ahead helper can inspect cached structures cheaply.

## In scope
- Add `with_...` style accessors in `crates/ridi-router-routing/src/routing_context.rs`
- Preserve current `point()` and `adjacent()` APIs for compatibility
- Ensure cached and uncached reads expose the same visible data
- Keep this change narrow and internal-facing

## Preferred API direction
Examples:

```rust
ctx.with_point(point_ref, |point| {
    // inspect point by reference
});

ctx.with_adjacent(point_ref, |adjacent| {
    // inspect adjacency by reference
});
```

Exact names may vary, but the requirements are:
- no `MapDataPoint` clone when read-only inspection is enough
- no adjacency vector clone when read-only iteration is enough
- no behavior change for callers

## Design constraints
- Keep ownership and borrowing rules simple enough that later walker code can use them ergonomically
- Do not force a whole-codebase migration yet
- Keep the old cloning APIs available until the refactor is complete
- Avoid exposing new public surface area unless it is already consistent with crate boundaries and intended visibility

## Tasks
1. Add `with_point(...)`
2. Add `with_adjacent(...)`
3. Ensure both work for first access and cached access
4. Add or wire up the Phase 1 equivalence tests for these APIs
5. Keep existing callers working unchanged

## Out of scope
- No walker refactor yet
- No heading look-ahead helper yet
- No wide call-site conversion across the codebase

## Deliverables
- Borrowed accessors in `routing_context.rs`
- Tests showing equivalence with current `point()` and `adjacent()` behavior
- Minimal internal documentation or comments explaining intended hot-path use

## Validation
Recommended checks:
- `cargo test -p ridi-router-routing routing_context`
- `cargo test -p ridi-router-routing`

## Exit criteria
- New accessors compile cleanly
- Visible behavior matches existing clone-based APIs
- Cached access works correctly
- Later phases can use the new accessors without needing API redesign

## Handoff to next phase
Phase 3 should treat these accessors as the preferred way to inspect point and adjacency data inside the new shared classifier.
