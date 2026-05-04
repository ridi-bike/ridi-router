# Phase 2 — Cache decoded tag values and clean up the hottest tag users

## Goal
Stop re-decoding the same tag strings in routing hot paths.

## Why this phase is separate
The profile says tag lookup is still expensive, but the real waste is repeated decoding inside walker- and weight-heavy code. This phase changes cache behavior and some call-site APIs, so it deserves its own landing.

## Main files
- `crates/ridi-router-routing/src/routing_context.rs`
- `crates/ridi-router-routing/src/router/weights.rs`
- `crates/ridi-router-routing/src/router/route/mod.rs`
- `crates/ridi-router-routing/src/router/route/score.rs`

## Concrete changes
1. Add a decoded tag-value cache to `RoutingCaches`, keyed by `ElementTagValueRef`.
2. Implement cached tag lookup in `RoutingContext`.
3. Avoid re-introducing allocation at the API boundary:
   - either add a borrowed accessor like `with_tag_value(...)`
   - or switch routing-only call sites to a cached small-string type
   - do **not** keep a cache internally and then allocate a fresh `String` on every hot call
4. Update the hottest users first:
   - `segment_name`
   - `segment_hw_ref`
   - `segment_highway`
   - `segment_surface`
   - `segment_smoothness`
   - `route::is_back_on_road_within_distance(...)`
5. If the types make it easy, compare road identity by tag refs before falling back to decoded string equality in the backward road scan.
6. Add tests that prove repeated tag reads hit the cache and keep existing semantics for missing values.

## Validation
- `cargo test -p ridi-router-routing`
- Re-run the profiled route:
  - `RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia`
- Compare these hotspots first:
  - `graph::get_tag_value`
  - `tile_manager::get_tag_value_if_loaded`
  - `route::is_back_on_road_within_distance`
  - `weights::weight_heading`
  - `weights::weight_no_short_detours`

## Exit criteria
- Repeated tag reads stop allocating fresh strings in hot routing code.
- Tag lookup hotspots drop meaningfully in both time and alloc.
- Weight and route tests still pass unchanged.

## Open implementation choice
Borrowed accessor chosen: add `RoutingContext::with_tag_value(...)` and keep cached values as `smartstring::alias::String` so hot routing code can reuse decoded tag strings without allocating fresh `String`s on every read.
