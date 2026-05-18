# Retained Rendering Plan for `ridi-app`

## Goal

Make UI interaction always responsive, even when map data or geometry is incomplete.

The main thread should handle input, viewport changes, and drawing already-prepared render artifacts. Heavy work should run off the UI thread and publish retained artifacts as they become ready.

```text
Main/UI thread
  ├─ handle touch / mouse / keyboard
  ├─ update viewport transform
  ├─ choose best available rendered tiles
  └─ draw retained tile artifacts

Worker thread
  ├─ fetch tile bytes
  ├─ decode MVT tiles
  ├─ apply map-rendering.ron rules
  ├─ triangulate / build geometry
  └─ send completed render parts back to UI
```

## Design Principles

1. **Never block input on map preparation**
   - No PMTiles fetch, MVT decode, triangulation, or mesh construction in the frame loop.
   - The frame loop may request work, but it must keep rendering with what it already has.

2. **Keep old geometry visible while new geometry is prepared**
   - Panning should move existing geometry immediately.
   - Zooming should keep the previous zoom-level geometry visible, scaled by the viewport transform, until the new zoom-level tiles are ready.
   - Rotation/resizing should reuse already-built geometry and only update transforms.

3. **Deliver tiles in render-order parts**
   - A tile should not appear only after all layers are done.
   - Worker should send completed parts in the order defined by `map-rendering.ron` render rules: water/earth/landuse first, then roads, labels, etc.

4. **Prefer partial correctness over blocking completeness**
   - It is acceptable for some tile layers to be missing briefly.
   - It is not acceptable for touch gestures to stall.


## Implementation Mode

This is a big-bang renderer rewrite.

- Do not preserve backward compatibility with the immediate-mode renderer.
- The codebase may be broken between implementation tasks.
- Implement the full retained pipeline before testing/releasing.
- Partial delivery still applies at runtime: workers may stream tile/rule/chunk results while the UI stays responsive.

## Current Problem

The immediate-mode loop still does too much per frame:

```text
for every frame:
  compute visible tiles
  iterate style rules
  iterate tile features
  match filters
  look up cached triangulation
  transform triangle vertices to screen space
  call draw_triangle many times
```

Even with triangulation cache hits, the app still touches ~55,000 cached feature geometries per frame. Static frame time is still measured in seconds.

## Target Architecture

```text
                  ┌───────────────────────────┐
                  │        Main Thread         │
                  │  input + viewport + draw   │
                  └─────────────┬─────────────┘
                                │ work requests
                                ▼
                  ┌───────────────────────────┐
                  │       Worker Thread        │
                  │ fetch/decode/build parts   │
                  └─────────────┬─────────────┘
                                │ completed parts
                                ▼
                  ┌───────────────────────────┐
                  │     Rendered Tile Cache    │
                  │ tile → retained artifacts  │
                  └───────────────────────────┘
```

## Data Model

### Tile Identity

```rust
struct TileKey {
    address: TileAddress,
    style_revision: u64,
}
```

`style_revision` increments if `map-rendering.ron` changes or is reloaded.

### Render Rule Identity

Each `RenderInstruction` from `map-rendering.ron` should get a stable order/index:

```rust
struct RuleKey {
    zoom_range_index: usize,
    rule_index: usize,
    rule_id: String,
}
```

`rule_id` is diagnostic only. Draw ordering uses `(zoom_range_index, rule_index)` so the exact order from the rendering spec is preserved.

### Rendered Tile

```rust
struct RenderedTile {
    key: TileKey,
    parts: Vec<RenderedTilePart>,
    complete: bool,
}
```

### Rendered Tile Part

A part corresponds to one render rule, or one chunk of one render rule if the geometry is large. `chunk_index` preserves deterministic ordering within split geometry.

```rust
struct RenderedTilePart {
    rule_key: RuleKey,
    chunk_index: usize,
    layer_name: String,
    kind: RenderedPartKind,
}
```

Possible kinds:

```rust
enum RenderedPartKind {
    FillMesh(FillMesh),
    LineBatch(LineBatch),
    PointBatch(PointBatch),
    LabelBatch(LabelBatch),
}
```

### Mesh Coordinates

Retained geometry should be stored in normalized tile-local coordinates, not screen coordinates.

Preferred:

```rust
struct TileVertex {
    // 0.0..1.0 within the MVT tile.
    // Convert from MVT coordinates with: [x / extent, y / extent].
    local: [f32; 2],
}
```

Then draw with a viewport/tile transform:

```text
normalized tile-local → global Mercator/world → clip/screen
```

The production path should transform in the vertex shader. CPU screen-coordinate conversion is only acceptable for diagnostics or temporary debugging.

## Main Thread Responsibilities

The main loop should do only fast work:

1. Read input.
2. Update viewport.
3. Compute visible tile keys.
4. Submit missing work requests to the scheduler/worker.
5. Drain completed worker messages and upload GPU resources within the per-frame budget.
6. Select best available rendered tiles.
7. Draw retained artifacts using current viewport transform.
8. Draw overlays.
9. `next_frame().await`.

Pseudo-code:

```rust
loop {
    let input = read_input();
    viewport.update(input);

    let visible = visible_tile_set(viewport);
    render_scheduler.request_visible_tiles(visible, target_zoom);

    drain_completed_worker_parts();
    upload_ready_gpu_parts_with_budget();

    let draw_set = render_cache.best_available_tiles(viewport, target_zoom);
    draw_retained_tiles(draw_set, viewport.transform());
    next_frame().await;
}
```

## Worker Thread Responsibilities

The worker owns heavy preparation work:

1. Receive tile work requests.
2. Drop requests that are already stale.
3. Fetch PMTiles bytes if needed.
4. Decode MVT.
5. Apply rendering rules in order.
6. Build retained geometry chunks for each rule.
7. Send completed parts, completion, missing-tile, or failure messages back to the main thread.

Pseudo-code:

```rust
while let Ok(request) = work_rx.recv() {
    match request {
        WorkRequest::BuildTile { tile, zoom, style_revision, generation } => {
            let tile_key = TileKey {
                address: tile,
                style_revision,
            };

            if is_stale(style_revision, generation) {
                continue;
            }

            let bytes = match fetch_tile(tile) {
                Ok(Some(bytes)) => bytes,
                Ok(None) => {
                    completed_tx.send(WorkerMessage::TileMissing { tile_key, generation });
                    continue;
                }
                Err(error) => {
                    completed_tx.send(WorkerMessage::TileFailed { tile_key, generation, error });
                    continue;
                }
            };

            if is_stale(style_revision, generation) {
                continue;
            }

            let decoded = match decode_mvt(bytes) {
                Ok(decoded) => decoded,
                Err(error) => {
                    completed_tx.send(WorkerMessage::TileFailed { tile_key, generation, error });
                    continue;
                }
            };

            for (rule_index, rule) in spec.rules_for_zoom(zoom).enumerate() {
                if is_stale(style_revision, generation) {
                    break;
                }

                for part in build_rule_parts(&decoded, rule_index, rule) {
                    if is_stale(style_revision, generation) {
                        break;
                    }
                    completed_tx.send(WorkerMessage::CompletedPart {
                        tile_key,
                        rule_key: part.rule_key.clone(),
                        generation,
                        part,
                    });
                }
            }

            if !is_stale(style_revision, generation) {
                completed_tx.send(WorkerMessage::TileComplete { tile_key, generation });
            }
        }
    }
}
```

## Work Scheduling

### Priority

Work should be prioritized by current interaction needs:

1. Visible tiles at current target zoom.
2. Edge tiles likely to enter during pan.
3. Parent/child fallback tiles for zoom transitions.
4. Adjacent zoom levels for prewarming.


### Backpressure

The scheduler is main-owned. It should maintain the priority queue, deduplication state, retry state, and byte/item budgets before sending work to the worker.

Rules:

- Never block the main thread when the work queue is full.
- If the bounded work queue is full, keep the request in the scheduler and retry on a later frame.
- Bound queued work by item count and approximate output byte cost.
- Prefer newer visible work over old queued work from earlier generations.
- The completion queue should also be bounded. If it fills, the worker may block, but it should re-check staleness before doing more expensive work.

### Cancellation / Staleness

The worker does not need hard cancellation at first. Instead, work and results carry a generation:

```rust
struct WorkGeneration(u64);
```

A generation increments when the visible tile request changes due to pan, zoom, or resize. The main thread also exposes the latest useful generation/style revision to the worker, for example through atomics.

The worker should check staleness before fetch, before decode, before each rule, and between large chunks. The main thread must still ignore completed work that targets an obsolete style revision or no-longer-useful generation.

### Deduplication

Do not queue duplicate work:

```rust
enum TileBuildState {
    Missing,
    Queued,
    Building,
    Partial,
    Complete,
    Failed,
}
```

## Zoom Behavior

### Smooth Zoom Within Same Tile Zoom

If `viewport.zoom_level(MAX_TILE_ZOOM)` does not change:

- Reuse existing rendered tiles.
- Only update transform.
- No geometry rebuild.

### Crossing Tile Zoom Levels

If target tile zoom changes:

1. Keep drawing previous zoom-level rendered tiles.
2. Request visible tiles for the new zoom.
3. As new zoom tiles arrive, draw them over or replace old tiles tile-by-tile.
4. Once enough new zoom tiles are ready, switch primary draw set to the new zoom.

Fallback options:

```text
new z tile missing → draw parent tile from z-1 scaled up
old z tile available → keep drawing it until replacement arrives
```

## Panning Behavior

Panning should never rebuild visible existing tiles.

When panning exposes new edge tiles:

1. Existing retained tiles move immediately with the viewport transform.
2. Newly exposed tiles are requested from the worker.
3. Blank or parent fallback is used until the new tile parts arrive.

## Layer / Rule Delivery

`map-rendering.ron` already defines render order. The worker should emit parts in that order.

Example order from current spec:

1. `water_ocean_fill`
2. `earth_fill`
3. `landuse_forest`
4. `landuse_grass`
5. `landuse_industrial_concrete`
6. `landuse_farm`
7. `landuse_park`
8. more water/roads/labels...

A tile can therefore appear progressively:

```text
background → earth/water → landuse → roads → labels
```

The main thread must preserve global render order. It must not draw tile A completely, then tile B completely. It should iterate render rules first and draw the selected target/fallback coverage for every tile for that rule before moving to the next rule.

Draw order is:

```text
rule order → zoom/fallback coverage policy → tile order → chunk_index
```

Arrival order never determines draw order.

## Rendering Strategy

These are implementation task groups, not releasable phases. The retained renderer should be built as one big-bang replacement before validation/release.

### Fill Meshes

Fills are the current bottleneck.

Build per-tile/per-rule fill meshes:

```rust
struct FillMesh {
    rule_key: RuleKey,
    chunk_index: usize,
    vertices: Vec<TileVertex>,
    indices: Vec<u16>,
}
```

Draw one retained mesh chunk per tile/rule instead of thousands of `draw_triangle` calls.

Expected impact:

```text
many triangle calls per frame → one mesh draw per fill rule chunk per tile
```

### Dotted Fill Shader / Pattern

Dotted fills must not enumerate dots on the CPU each frame.

Use a shader/pattern path:

- Reuse the retained fill mesh as the polygon mask.
- Compute the dot pattern in the fragment shader using tile-local or world coordinates.
- Pass pattern uniforms such as spacing, radius, dot color, and optional base fill color.
- If one shader cannot cleanly handle base color plus dots, draw the base fill pass first and a pattern pass second.
- Do not use the current CPU `point_in_triangle` grid scan in the retained renderer.

### Line Batches

Move line geometry to retained batches.

Straight simple lines can be batched. Dashed, dotted, and cased lines may need separate mesh generation or shader parameters. The main thread must not rescan MVT features to draw lines.

### Point and Label Batches

Move point geometry and label candidate positions into retained batches.

Glyph/text drawing may still use the available text API, but MVT feature scans, label extraction, and label layout should not happen in the frame loop.

## GPU Transform / Shader Offload

The target retained renderer should avoid CPU screen-coordinate conversion per frame.

Worker thread output:

```text
tile-local CPU mesh data
```

Main thread GPU path:

```text
upload tile-local mesh once
set viewport/tile transform uniforms each frame
draw GPU mesh
```

The vertex shader should perform:

```text
tile-local coordinate → world/Mercator coordinate → clip/screen coordinate
```

This keeps pan, smooth zoom, and rotation/resize cheap because only uniforms change.

### Macroquad Capability Findings

Macroquad has two relevant layers:

1. Public `macroquad::models::Mesh`
   - `Mesh` is CPU-side: `vertices: Vec<Vertex>`, `indices: Vec<u16>`.
   - `draw_mesh(&mesh)` calls `context.gl.geometry(&mesh.vertices, &mesh.indices)`.
   - `geometry()` appends vertices/indices into macroquad's per-frame batch buffers.
   - This helps batching compared with many `draw_triangle` calls, but it is not true persistent GPU-buffer retained mode.

2. Lower-level miniquad / internal GL
   - miniquad exposes `new_buffer`, `buffer_update`, `apply_bindings`, `apply_pipeline`, uniforms, and `draw`.
   - This is the path for true persistent GPU vertex/index buffers.
   - It is lower-level and likely must be driven from the render/main thread where the graphics context exists.

Decision: use miniquad-level GPU buffers directly for the retained renderer. Do not spend implementation effort on public `macroquad::Mesh` as an intermediate retained path, because it still submits CPU vertex/index slices into macroquad's per-frame batch.

### Background Thread GPU Upload

Assume GPU resource creation/upload must happen on the main/render thread.

The background worker should not upload to GPU directly. It should produce CPU-side mesh packets:

```rust
struct CpuMeshPacket {
    tile_key: TileKey,
    rule_key: RuleKey,
    chunk_index: usize,
    vertices: Vec<TileVertex>,
    indices: Vec<u16>,
    cpu_bytes: usize,
}
```

The main thread drains packets and uploads them to GPU in a small per-frame upload budget. After upload, it swaps or appends the ready GPU part into the scene cache.

This preserves UI responsiveness and avoids relying on cross-thread graphics-context behavior.

If we later prove macroquad/miniquad supports safe context work from another thread on Android, we can revisit this. Treat it as unsupported until proven otherwise.

### GPU Resource Lifetime

All GPU resource creation, updates, and deletion must happen on the main/render thread.

Cache eviction should enqueue explicit main-thread deletion of miniquad buffers. Do not rely on worker-thread drops or background-thread destructors for GPU cleanup.

Every retained part should track approximate CPU and GPU bytes so cache eviction can enforce the soft and hard budgets.

### Vertex Color Format Decision

For fill meshes, prefer color as a per-mesh/rule uniform at first:

```rust
struct TileVertex {
    local: [f32; 2],
}

struct GpuMeshPart {
    rule_key: RuleKey,
    chunk_index: usize,
    color: Color,
    fill_pattern: Option<FillPatternUniforms>,
    vertex_buffer: BufferId,
    index_buffer: BufferId,
    index_count: i32,
    gpu_bytes: usize,
}
```

Implications:

- Smaller vertex buffers: position-only vertices are cheaper to upload and store.
- Simpler batching by rule: each rule already has one style/color in `map-rendering.ron`.
- Changing a style color later only updates a uniform, not all vertices.
- If one mesh must contain multiple colors later, we can add per-vertex color then.

For line/label stages, revisit this. Lines may need per-vertex attributes for joins, caps, dash phase, or width. Labels likely need texture coordinates and color.

### Index Format

Use `u16` indices for retained mesh chunks initially.

Rules:

- Split every tile+rule mesh into chunks below 65,535 vertices.
- Keep `chunk_index` stable for deterministic draw order.
- This is required even when using miniquad directly, because backend index-size support is mixed and public macroquad `Mesh` is `u16`-only.
- Revisit `u32` indices only after verifying every target backend.


### Memory Budget

A 100–200 MB retained render cache is reasonable as an initial budget.

Use a soft budget first:

```text
target: 100 MB
hard cap: 200 MB
```

Evict by a combined distance/age score:

1. obsolete style revisions
2. non-visible tiles farthest from the current viewport center
3. among similarly distant tiles, oldest least-recently-used first
4. protect visible tiles and previous-zoom fallback tiles unless over the hard cap

Track approximate CPU and GPU bytes per rendered tile/part.

Eviction must account for both CPU-side queued/retained data and GPU buffers. GPU buffers are destroyed only through the main-thread deletion queue.

### Experimental Replacement Policy

Design replacement policy as configurable.

Start with:

```text
tile-by-tile replacement
layer-by-layer/part-by-part delivery
```

That means a new tile can progressively replace the fallback as parts arrive. The renderer should allow experimenting with policies:

```rust
enum ReplacementPolicy {
    TileByTile,
    LayerByLayer,
    WholeViewportWhenReady,
}
```

Default should be `TileByTile` with progressive layer delivery.

Default progressive mode is non-strict: draw available tile parts as soon as they arrive, but always use global rule-first render ordering. Do not draw in arrival order or tile-complete order.

### Missing Tile Retry Policy

Tiles should generally not be missing. Failed or missing tile requests should be retried periodically, but the policy should be abstracted.

Initial policy: retry failed/missing visible tile requests every 10th move/zoom generation.

```rust
trait TileRetryPolicy {
    fn should_retry(&self, tile: TileAddress, generation: WorkGeneration, failure: TileFailure) -> bool;
}
```

Start with:

```text
retry when generation % 10 == 0 and tile is visible/needed
```

A generation increments when the visible tile request changes due to pan, zoom, or resize.



## Threading Model

### Recommended Channels

Use bounded standard channels first:

```rust
std::sync::mpsc::SyncSender<WorkRequest>
std::sync::mpsc::Receiver<WorkRequest>
std::sync::mpsc::SyncSender<WorkerMessage>
std::sync::mpsc::Receiver<WorkerMessage>
```

The main thread should use non-blocking send behavior for work submission. If this becomes limiting, switch to `crossbeam-channel` for better select/try-send ergonomics.

### Shared State

Avoid sharing mutable render cache directly across threads.

Worker sends owned result messages:

```rust
enum WorkerMessage {
    CompletedPart {
        tile_key: TileKey,
        rule_key: RuleKey,
        generation: WorkGeneration,
        part: RenderedTilePart,
    },
    TileComplete {
        tile_key: TileKey,
        generation: WorkGeneration,
    },
    TileMissing {
        tile_key: TileKey,
        generation: WorkGeneration,
    },
    TileFailed {
        tile_key: TileKey,
        generation: WorkGeneration,
        error: String,
    },
}
```

Main thread owns the render cache, drains worker messages before draw-set selection, uploads GPU resources, inserts completed artifacts, and queues GPU deletions.

## Decisions

1. **Rendering backend**
   - Use miniquad-level GPU buffers directly.
   - Do not implement retained rendering through public `macroquad::Mesh`.

2. **Implementation mode**
   - Big-bang rewrite.
   - The codebase may be broken between implementation tasks.
   - Validate/release only after the full retained pipeline is implemented.

3. **Mesh coordinates**
   - Store retained geometry in normalized tile-local coordinates.
   - Transform to clip/screen coordinates in the vertex shader.

4. **Vertex color format**
   - Start with position-only vertices and per-rule/per-mesh color uniforms.
   - Add per-vertex color only if later required.

5. **Index format / chunking**
   - Use `u16` index chunks initially.
   - Chunk every tile+rule mesh below 65,535 vertices.
   - Verify target backend support before considering `u32` indices.

6. **Dotted fills**
   - Use a shader/pattern path.
   - Do not enumerate dots on the CPU in the retained renderer.

7. **Tile fetching thread**
   - Worker owns its own `PmtilesSource` and Tokio runtime.

8. **GPU upload and lifetime**
   - Worker creates CPU mesh packets only.
   - Main/render thread uploads packets to GPU under a 3 ms per-frame upload budget.
   - Main/render thread also deletes evicted GPU buffers.

9. **Worker scheduling**
   - Use main-owned priority scheduling, deduplication, retry state, and backpressure.
   - Work and completion queues are bounded.
   - Worker checks staleness before expensive stages and between chunks.

10. **Memory budget**
   - Use 100 MB target and 200 MB hard cap initially.
   - Track both CPU bytes and GPU bytes per retained part.

11. **Worker granularity**
   - Work and completion parts are tile+rule+chunk.

12. **Zoom fallback / replacement policy**
   - Use previous zoom as fallback.
   - Start with tile-by-tile and layer-by-layer replacement.
   - Non-strict progressive rendering: draw every available part, sorted by global rule-first render order.

13. **Cache eviction**
   - Evict farthest and oldest first.
   - Protect visible tiles and previous-zoom fallback tiles where possible.
   - Queue GPU buffer deletion on the main/render thread.

14. **Missing tile retry policy**
   - Abstract retry policy.
   - Initial behavior: re-attempt failed/missing visible tiles every 10th move/zoom generation.

15. **Text rendering**
   - Retain label candidates and layout metadata.
   - Glyph drawing may use the existing text API if it does not rescan MVT features in the frame loop.

16. **Thread communication**
   - Use bounded `std::sync::mpsc::sync_channel` channels initially.
   - Include `CompletedPart`, `TileComplete`, `TileMissing`, and `TileFailed` messages.


## Implementation Plan

This is a big-bang implementation plan. The steps are work organization only, not incremental releases. Backward compatibility with the current immediate-mode renderer is not required.

### Step 1: Core Retained Renderer Modules

- Add `render_worker.rs`, `render_cache.rs`, `render_scheduler.rs`, and `retained_renderer.rs`.
- Define `TileKey`, `RuleKey`, `WorkGeneration`, `WorkRequest`, `WorkerMessage`, `RenderedTilePart`, `TileBuildState`, `ReplacementPolicy`, and retry policy types.
- Replace the synchronous `VisibleTileDownloader` frame-loop path with scheduler-owned work submission.

### Step 2: Main-Owned Scheduler and Backpressure

- Compute visible target tiles and fallback tiles from the viewport.
- Maintain priority, deduplication, retry, generation, and tile-state data on the main thread.
- Use bounded work/completion channels.
- Never block the main thread when work queues are full.
- Prefer newer visible work over stale queued work.

### Step 3: Worker Fetch, Decode, and Build Pipeline

- Worker owns its own `PmtilesSource` and Tokio runtime.
- Worker fetches PMTiles bytes, decodes MVT tiles, applies render rules in order, and builds retained CPU artifacts.
- Worker checks staleness before fetch, before decode, before each rule, and between large chunks.
- Worker emits `CompletedPart`, `TileComplete`, `TileMissing`, and `TileFailed` messages.

### Step 4: Retained CPU Artifacts

- Store geometry in normalized tile-local coordinates.
- Build fill mesh chunks with `u16` indices.
- Build line batches, point batches, and label candidate/layout batches.
- Represent dotted fills as shader/pattern metadata, not CPU-generated dot geometry.
- Track approximate CPU bytes per packet/part.

### Step 5: Main-Owned GPU Upload and Cache

- Drain worker messages before draw-set selection each frame.
- Upload ready CPU packets to miniquad GPU buffers under a 3 ms/frame budget.
- Insert uploaded parts into `RenderedTileCache`.
- Track GPU bytes per part.
- Evict by style revision, distance, age, and memory pressure.
- Delete evicted GPU buffers only on the main/render thread.

### Step 6: Retained Miniquad Renderer

- Draw GPU mesh parts with shader transforms from tile-local to clip/screen coordinates.
- Use per-mesh/rule uniforms for color and dotted fill pattern parameters.
- Preserve global rule-first render order: rule order, zoom/fallback coverage policy, tile order, then chunk order.
- Keep overlays/debug UI working after raw miniquad drawing.

### Step 7: Zoom Fallback and Progressive Replacement

- Keep previous zoom rendered tiles visible during zoom transitions.
- Add `best_available_tiles(viewport, target_zoom)`.
- Use parent/previous zoom fallback until target zoom tiles are ready.
- Use tile-by-tile and layer-by-layer replacement with non-strict progressive delivery.

### Step 8: Remove Immediate Hot Path

- Remove PMTiles fetch/decode from the frame loop.
- Remove MVT feature scans from the frame loop.
- Remove per-frame fill triangulation and screen-coordinate triangle conversion.
- Remove the old `draw_triangle` fill path from production rendering.

### Step 9: Final Validation and Release

- Run format, tests, and lint only after the full retained pipeline is implemented.
- Measure frame time during static view, pan, and zoom.
- Verify touch overlay responsiveness during tile loading and geometry building.
- Verify old map content remains visible during zoom fallback.
- Verify missing/failed tiles retry according to policy.

## Success Criteria

1. Touch overlay updates at interactive rates during pan/zoom.
2. Static viewport frame time drops from seconds to milliseconds.
3. During zoom, old map remains visible immediately.
4. New detail appears progressively as worker parts complete.
5. No tile fetch/decode/triangulation happens directly in the frame loop.
