# Perf plan phase 2: introduce detector data structures in `Route`

## Goal

Add the new loop-detector structures to `Route` and wire them into route mutation.

This phase is still infrastructure. The query path does not need to switch yet.

## Important note

Even though the overall rewrite is intended to land as one big change, this phase describes the first implementation chunk so the work stays understandable.

## Work items

### 1. Extend `Route` with detector-owned state

Add a field such as:

```rust
loop_detector: LoopDetector,
```

The exact type names can vary, but the structure should support:

- per-segment metadata storage
- exact point hit lookup
- road-local spatial lookup
- clean push/pop rollback

### 2. Define `LoopMeta`

This should contain the minimum data needed by loop detection:

- endpoint point ref
- point id if useful
- endpoint lat
- endpoint lon
- `hw_ref: Option<RoadKey>`
- `name: Option<RoadKey>`
- precomputed spatial cell id if useful

Do not store unrelated hydrated state.

### 3. Define `LoopDetector`

Suggested fields:

- `metas: Vec<LoopMeta>`
- `point_hits: HashMap<MapDataPointRef, Vec<usize>>`
- `hw_ref_cells: HashMap<RoadKey, HashMap<CellId, Vec<usize>>>`
- `name_cells: HashMap<RoadKey, HashMap<CellId, Vec<usize>>>`

### 4. Define `RoadKey`

Use an interned, cheap-comparison key for road identity.

Desired properties:

- stable across tiles
- cheap to compare and hash
- can represent values for both `hw_ref` and `name`

### 5. Define `CellId`

Represent a coarse spatial bucket derived from the endpoint coordinates.

Desired properties:

- deterministic
- cheap to compute
- good enough for local candidate narrowing

### 6. Keep `Route` cloning correct

Make sure `Route` still derives or implements the traits it needs after adding detector state.

In particular, verify:

- `Clone`
- `Debug` if needed
- `PartialEq` if still applicable

## Suggested file targets

- `crates/ridi-router-routing/src/router/route/mod.rs`
- possibly a new helper module under `crates/ridi-router-routing/src/router/route/`

## Deliverable

`Route` owns a detector structure with the right data model in place.

## Acceptance criteria

- project compiles with the new types added
- `Route` still supports existing call sites
- detector state layout matches the plan
- no query behavior is switched yet unless required for compilation
