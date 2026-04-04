# Map Data Refactor - Phase 4: Remove singleton and implicit dereference machinery

_Status: completed_

## Purpose

Delete the old architecture after the converted runtime paths no longer depend on it.

This is the sharp cleanup phase that removes:

- process-global graph state
- singleton initialization/access APIs
- implicit ref/tag dereference methods tied to ambient graph access

## Scope

### In scope

- delete `MAP_DATA_GRAPH`
- delete `MapDataGraph::init(...)`
- delete `MapDataGraph::get()`
- remove or redesign `MapDataElement::get_from_tiles(...)`
- remove or redesign `MapDataElementRef<T>::get()`
- remove or redesign `ElementTagValueRef::get()`
- remove or redesign `ElementTagSetRef::get()`
- simplify any leftover code that only existed to support the singleton/deref model

### Out of scope

- broad new performance work
- wrong-context guard mechanisms
- tile TODO completion

## Primary files

Illustrative, not exhaustive.

- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/map_data/point.rs`
- `crates/ridi-router-routing/src/map_data/line.rs`
- `crates/ridi-router-routing/src/map_data/rule.rs`
- any runtime modules still referencing deleted APIs

## Implementation details

### 1. Remove the singleton root completely

`MAP_DATA_GRAPH`, `MapDataGraph::init(...)`, and `MapDataGraph::get()` must be deleted, not merely deprecated.

The plan explicitly prefers removing compatibility paths as soon as explicit replacements exist.

That means:

- no compatibility wrapper that forwards `MapDataGraph::get()` to some hidden current graph
- no new global service locator replacing the old singleton under a different name

### 2. Collapse ref dereference behavior back to plain IDs

Refs should end this phase as lightweight identifiers only.

Expected end state:

- `MapDataPointRef` and `MapDataLineRef` remain cheap ID wrappers
- tag refs remain cheap tile/index identifiers
- real data access happens only through explicit graph/context methods

If a trait like `MapDataElement` still exists after this phase, it should not encode ambient singleton-backed dereference behavior.

### 3. Redesign tag access to avoid hidden graph lookup

`ElementTagValueRef::get()` and `ElementTagSetRef::get()` are especially important because nested tag access can quietly preserve the old architecture.

Expected direction:

- remove singleton-backed tag deref methods
- route all tag resolution through `RoutingContext` / `MapDataGraph`
- keep any remaining tag container helpers free of ambient access assumptions

### 4. Remove migration-only scaffolding introduced by earlier phases

Examples of what to clean up here:

- temporary compatibility helper functions
- transitional trait implementations that existed only to preserve `.get()` call sites
- imports and utility methods that only supported singleton access

This phase should leave the codebase in the actual target architecture, not in a half-converted compatibility state.

## End state

- refs are plain IDs only
- all real data access goes through explicit graph/context methods
- no runtime path reaches into ambient process state

## Acceptance criteria

- `MAP_DATA_GRAPH` is deleted
- `MapDataGraph::init(...)` is deleted
- `MapDataGraph::get()` is deleted
- runtime code no longer contains implicit `.get()`-style dereference for point/line/tag refs
- ref and tag types no longer provide ambient singleton-backed data access
- all remaining real data access is explicit through `MapDataGraph` and/or `RoutingContext`

## Validation checklist

- [x] `graph.rs` no longer defines `MAP_DATA_GRAPH`
- [x] `graph.rs` no longer defines `MapDataGraph::init(...)`
- [x] `graph.rs` no longer defines `MapDataGraph::get()`
- [x] `MapDataElementRef<T>::get()` is removed or redesigned away from ambient lookup
- [x] `ElementTagValueRef::get()` is removed or redesigned away from ambient lookup
- [x] `ElementTagSetRef::get()` is removed or redesigned away from ambient lookup
- [x] runtime modules compile against explicit graph/context access only
- [x] no migration-only singleton compatibility shim remains

### Suggested checks

- [x] `rg "MAP_DATA_GRAPH|MapDataGraph::init|MapDataGraph::get|ElementTagValueRef::get|ElementTagSetRef::get|MapDataElementRef<.*>::get" crates/ridi-router-routing/src`
- [x] `rg "\.get\(\)" crates/ridi-router-routing/src | rg "MapData|ElementTag|tag"`
- [x] full compile/test pass once this deletion phase lands cleanly
- [x] `cargo test -p ridi-router-routing`

## Progress checklist

- [x] remove singleton static and singleton constructors/accessors
- [x] delete or redesign ambient dereference trait/method machinery
- [x] delete or redesign ambient tag dereference methods
- [x] update remaining call sites to explicit graph/context lookups
- [x] remove migration-only compatibility helpers
- [x] run search-based validation for deleted APIs
- [x] confirm phase acceptance criteria are met

## Notes and watch-outs

- Do not leave “temporary” fallback methods behind. This phase exists specifically to remove them.
- Search aggressively for nested tag lookup patterns and formatting/debug helpers that may still rely on deleted APIs.
- If anything still needs the old APIs, Phase 3 was not actually complete.
