# Map Data Refactor - Phase 3: Convert lower-level helpers and domain methods

_Status: completed_

## Purpose

Remove hidden lookup behavior from the deeper routing and map-data helper layers.

This is the core redesign phase where refactor work stops being mostly mechanical and starts changing helper responsibilities.

## Scope

### In scope

- convert implicit-deref-heavy runtime helpers away from hidden `.get()` chains
- redesign method signatures where apparently-owned domain methods still reach into map data implicitly
- move true lookup helpers toward `RoutingContext`
- make pure geometry/scoring/formatting helpers operate on already-resolved values or plain coordinates
- clean up formatting paths that still dereference refs transitively

### Out of scope

- removing singleton/ref `.get()` definitions themselves from the codebase entirely
- full test-harness rewrite
- tile TODO completion

## Primary files

Illustrative, not exhaustive.

- `crates/ridi-router-routing/src/router/weights.rs`
- `crates/ridi-router-routing/src/router/route/mod.rs`
- `crates/ridi-router-routing/src/router/route/segment.rs`
- `crates/ridi-router-routing/src/router/route/segment_list.rs`
- `crates/ridi-router-routing/src/router/route/score.rs`
- `crates/ridi-router-routing/src/router/clustering.rs`
- `crates/ridi-router-routing/src/router/itinerary.rs`
- `crates/ridi-router-routing/src/map_data/point.rs`
- `crates/ridi-router-routing/src/map_data/line.rs`
- `crates/ridi-router-routing/src/map_data/rule.rs`

## Implementation details

### 1. Identify helpers that only look pure

A major risk in this codebase is that some methods look like plain domain logic but secretly perform graph lookups through refs.

Typical examples called out by the plan:

- `point_ref.get().lat`
- `line_ref.get().tags.get().highway()`
- `MapDataPoint::distance_between(&MapDataPointRef)`
- `MapDataPoint::bearing(&MapDataPointRef)`
- `MapDataLine::line_id()`
- `MapDataLine::get_len_m()`
- `Segment::get_bearing()`

Each such helper needs one of two outcomes.

### 2. Move true lookup helpers to explicit context use

If a helper's real job is "resolve something from refs," make that explicit.

Preferred direction:

- add `&RoutingContext` to the method/function signature
- or move the lookup into a context helper and pass resolved values into the domain helper

Good candidates:

- nested tag lookup chains
- adjacency-driven helper logic
- score/stat helpers that need line/point/tag resolution

### 3. Keep pure helpers pure

If the helper is really geometry, scoring math, clustering math, or formatting, make it operate on data it already has.

Preferred direction:

- accept coordinates instead of refs
- accept resolved `MapDataPoint` / `MapDataLine` values instead of refs
- compute bearings, distances, and segment properties from explicit inputs

The rule of thumb is simple:

- lookup work should be explicit
- pure work should stop looking things up

### 4. Redesign domain methods instead of preserving misleading APIs

Do not keep a misleading method surface just to minimize call-site edits.

Examples:

- if `MapDataPoint::bearing(...)` really needs another point lookup, either make it accept the resolved point or require `&RoutingContext`
- if `MapDataLine::get_len_m()` really depends on resolving endpoint refs, either pass the endpoint data in or make the lookup dependency explicit
- if formatting helpers trigger transitive lookup, replace them with minimal formatting or explicit formatter helpers that take context

### 5. Audit tag access especially carefully

Tag access is one of the easiest ways for hidden global dereference to survive.

Pay attention to:

- `ElementTagSetRef::get()`-style chains
- helpers that call `line.tags.get().highway()` or similar nested access
- formatting/debug code that reads tag values during display

By the end of this phase, runtime helpers should no longer depend on that style of access even if the old methods still exist temporarily.

## Dependencies and handoff to the next phase

This phase is complete when runtime routing code no longer relies on implicit ref/tag `.get()` behavior for its helper logic.

Phase 4 should then be able to delete singleton and implicit dereference machinery with confidence instead of hunting for remaining hidden users.

## Acceptance criteria

- runtime routing code no longer depends on `MapDataElementRef<T>::get()`
- runtime routing code no longer depends on tag ref `.get()` methods
- converted helper APIs make lookup requirements explicit instead of hiding them inside apparently-owned domain methods
- pure geometry/scoring/clustering helpers operate on explicit inputs rather than performing hidden map-data resolution
- formatting/debug helpers used by runtime paths no longer perform transitive implicit dereferencing

## Validation checklist

- [x] `weights.rs` no longer relies on implicit point/line/tag dereference
- [x] `route/*` helpers no longer hide ref resolution inside pure-looking methods
- [x] `clustering.rs` uses explicit lookup or explicit geometry inputs
- [x] `itinerary.rs` uses explicit lookup or explicit geometry inputs
- [x] `map_data/point.rs` helpers either accept resolved values or explicit `&RoutingContext`
- [x] `map_data/line.rs` helpers either accept resolved values or explicit `&RoutingContext`
- [x] `map_data/rule.rs` no longer depends on hidden global lookup patterns in runtime paths
- [x] formatting/debug paths used during routing are minimal or explicitly context-aware
- [x] runtime routing code does not depend on nested `.get().get()` chains

### Suggested checks

- [x] `rg "\.get\(\)" crates/ridi-router-routing/src/router crates/ridi-router-routing/src/map_data/point.rs crates/ridi-router-routing/src/map_data/line.rs crates/ridi-router-routing/src/map_data/rule.rs`
- [x] spot-check converted signatures for explicit context or explicit resolved-value inputs
- [x] run routing-focused tests after helper conversion
- [x] `cargo test -p ridi-router-routing`

## Progress checklist

- [x] audit deep helper methods that still hide lookups
- [x] classify each helper as lookup helper vs pure helper
- [x] move lookup helpers toward `RoutingContext` or explicit context parameters
- [x] convert geometry helpers to explicit coordinate/resolved-value inputs
- [x] convert scoring/stat/tag helpers to explicit lookup patterns
- [x] clean up formatting/debug helpers that still dereference refs transitively
- [x] remove remaining runtime uses of ref/tag `.get()` from converted areas
- [x] confirm phase acceptance criteria are met

## Notes and watch-outs

- This phase is where architecture can regress if convenience wins over clarity.
- Avoid preserving misleading APIs that look pure but still do hidden lookup work.
- Keep `Debug` and `Display` minimal; do not use them as an excuse to keep context-free dereferencing around.
- Temporary compilation breakage outside the converted area is acceptable, but the converted runtime/helper paths should match the acceptance criteria above.
