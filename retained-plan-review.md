# Retained Plan Branch Review

Reviewed current branch `ridi-v2` against `retained-plan.md`.

## Summary

The branch implements the main retained-rendering shape from the plan:

- main loop handles input, viewport, scheduling, message drain, upload, draw, and overlays
- worker owns PMTiles fetch, MVT decode, rule matching, geometry build, and result messages
- retained artifacts use tile-local coordinates
- GPU buffers are created and deleted on the main/render thread
- scheduling uses bounded channels and non-blocking work submission
- global rule-first draw ordering is attempted
- `cargo check -p ridi-app` passes

Main remaining issues are correctness and plan-compliance gaps in fallback selection, part ordering, dotted fills, line styling, and cleanup of obsolete immediate-mode files.

## Findings

### 1. Zoom fallback only works when no target tiles are ready

Severity: High

`RenderedTileCache::best_available_tiles` only uses `fallback` when `keys.is_empty()`.

Path: `crates/ridi-app/src/render_cache.rs`

This means once any target-zoom tile has at least one part, previous-zoom fallback tiles are no longer drawn for the rest of the viewport unless a direct parent tile is available. That does not match the plan:

- keep previous zoom-level geometry visible while new zoom tiles are prepared
- replace old tiles tile-by-tile
- use parent/previous zoom fallback until target tiles are ready

Expected behavior: each visible target tile should choose the best coverage independently: target tile if available, otherwise parent/previous-zoom coverage for that area.

### 2. Part and chunk draw order is not deterministic

Severity: High

The plan requires:

```text
rule order → zoom/fallback coverage policy → tile order → chunk_index
```

The current renderer sorts rule keys, then iterates `tile.parts` in insertion order. It does not sort by `chunk_index` inside a rule.

Path: `crates/ridi-app/src/render_cache.rs`

This is made worse by `pending_uploads` being a `Vec` drained with `pop()`, which reverses worker arrival order. For example, line casing and normal line parts are sent in one order, uploaded in reverse order, and then drawn in insertion order.

Expected behavior: sort or store parts by `(rule_key, chunk_index, subpass)` before drawing. Arrival/upload order should never affect draw order.

### 3. Dotted fill shader does not render dot color correctly

Severity: High

`draw_gpu_fill` computes a dot color, but the shader only receives one usable color. The `aux_color` value is mostly discarded: only `aux_color.3` is packed into `aux.w`, and the fragment shader never uses it as the dot color.

Path: `crates/ridi-app/src/render_cache.rs`

Current result: dotted fills use the base color, and if there is no base color the dots can be discarded entirely.

Expected behavior: pass both base fill color and dot color to the shader, then output dot color inside the dot radius and base color outside it.

### 4. Retained line batches ignore width and dash styling

Severity: Medium

`LineBatch` stores `width_px` and `dash`, but upload/draw turns every segment into `PrimitiveType::Lines` with no line-width or dash handling.

Path: `crates/ridi-app/src/render_cache.rs`

This satisfies “do not rescan MVT features”, but it does not preserve map-rendering style semantics for roads, dashes, dotted lines, or cased lines.

Expected behavior: generate retained line meshes for width/casing/dashes, or implement a shader path that receives enough attributes/uniforms to draw those styles correctly.

### 5. Old immediate-mode renderer files remain in the app crate

Severity: Medium

The branch leaves the old immediate-mode modules as source files:

- `crates/ridi-app/src/tile_renderer.rs`
- `crates/ridi-app/src/visible_tiles.rs`

They are not currently wired into `main.rs`, so the frame loop does not appear to use them. However, the plan’s Step 8 says to remove PMTiles fetch/decode from the frame loop and remove the old `draw_triangle` fill path from production rendering. Keeping these files makes it harder to verify that the immediate path is gone.

Expected behavior: remove them, or clearly quarantine them as non-production diagnostics.

### 6. Worker state never transitions to `Building`

Severity: Low

`TileBuildState::Building` exists, but the scheduler marks tiles `Queued` on send and later `Partial`, `Complete`, or `Failed`. No code marks a tile as `Building`.

Path: `crates/ridi-app/src/render_scheduler.rs`

This is not currently blocking, but it means the planned deduplication state machine is only partially represented.

### 7. Staleness tolerance may admit obsolete visual results

Severity: Low

The worker treats work as stale only when `generation.0 + 2 < latest_generation`, and the main thread accepts completed parts when `generation.0 + 4 >= scheduler.generation().0`.

Paths:

- `crates/ridi-app/src/render_worker.rs`
- `crates/ridi-app/src/render_cache.rs`

The plan says the main thread must ignore completed work that targets an obsolete or no-longer-useful generation. The current tolerance may be intentional to avoid over-canceling during pans, but it should be documented as a policy because it can show tiles from older view generations.

## Plan Coverage

### Implemented or mostly implemented

- retained renderer modules exist
- main-owned scheduler exists
- bounded work and completion channels are used
- worker owns PMTiles source and Tokio runtime
- worker fetches, decodes, applies rules, and emits parts
- retained artifacts use normalized tile-local coordinates
- fill meshes use `u16` indices and chunking
- main thread uploads GPU buffers under a 3 ms budget
- GPU deletion is queued on the main thread
- cache tracks CPU and GPU bytes
- rule-first rendering is attempted
- label candidates are retained instead of rescanning MVT features in the frame loop

### Partially implemented

- zoom fallback and replacement policy
- deterministic chunk/subpart ordering
- dotted fill shader path
- line styling
- retry/backpressure state machine
- cache eviction scoring

### Not clearly complete

- exact tile-by-tile fallback coverage during zoom transitions
- retained line meshes for width, joins, caps, dashes, and casing
- explicit removal/quarantine of old immediate-mode renderer files

## Validation Performed

```text
cargo check -p ridi-app
```

Result: pass.
