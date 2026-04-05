# Review: `MapDataLine` helpers in `crates/ridi-router-tiles`

## Verdict
Partially relevant

## Summary
The todo is **still pointing at a real code smell**, but its urgency and framing are stale.

In `crates/ridi-router-tiles`, `MapDataLine::line_id()` and `MapDataLine::get_len_m()` still perform hidden dereference work through `.get()` calls. That matches the todo description.

However, the active tile-generation path has already moved away from depending on `MapDataLine` for line serialization. The current generator uses `GenerationLine` with explicit node IDs in `crates/ridi-router-tiles/src/map_data/generation_graph.rs`, and the RMDF writer resolves point data explicitly in `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`.

So this is **not a current production-path blocker**. It is mostly **stale internal API debt / dead-code cleanup**, with one important caveat: the old helpers are not just stylistically outdated, they are built on dereference calls that would panic if exercised in this crate.

## Current evidence from the codebase

### 1) The todo’s core claim is still true in `line.rs`
In `crates/ridi-router-tiles/src/map_data/line.rs`:

- `line_id()` dereferences both endpoint refs:
  - `self.points.0.get().id`
  - `self.points.1.get().id`
- `get_len_m()` dereferences through `self.points.0.get().distance_between(&self.points.1)`
- `Debug` also depends on `line_id()` and further `.get()` calls

This is exactly the hidden lookup pattern described in the todo.

### 2) Dereferencing refs in this crate is not implemented
In `crates/ridi-router-tiles/src/map_data/graph.rs`, `MapDataElementRef<T>::get()` is:

- `panic!("tile-generation refs are not dereferenceable")`

That means the helper style in `line.rs` is not merely “ambient”; in this crate it is fundamentally incompatible with the current generation-side ref model.

### 3) The same issue exists in related debug helpers
The old dereference pattern also appears in:

- `crates/ridi-router-tiles/src/map_data/point.rs`
  - `Debug` maps line refs with `l.get().line_id()`
- `crates/ridi-router-tiles/src/map_data/rule.rs`
  - `Debug` maps line refs with `l.get().line_id()`

So the todo is a bit too narrow if the goal is to remove hidden lookup work consistently.

### 4) The active tile-generation flow already uses explicit data instead
In `crates/ridi-router-tiles/src/map_data/generation_graph.rs`:

- generation uses `GenerationLine`, not `MapDataLine`
- `GenerationLine` stores explicit `from_node_id` and `to_node_id`
- there is an explicit comment that `point.lines` is not updated because it expects `MapDataLineRef`

In `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`:

- `serialize_lines()` builds a `node_map`
- it explicitly looks up `point_a` and `point_b` from `line.from_node_id` / `line.to_node_id`
- line serialization writes explicit endpoint fields from those resolved points

That is already the “explicit inputs” style the todo asks for.

### 5) The crate currently compiles and tests pass because these helpers are unused
`cargo test -p ridi-router-tiles` passes.

The build also emits warnings that strongly suggest the relevant helpers are dead code today:

- `crates/ridi-router-tiles/src/map_data/line.rs`
  - `is_roundabout` and `get_len_m` are never used
- `crates/ridi-router-tiles/src/map_data/point.rs`
  - `distance_between` and `bearing` are never used
- `crates/ridi-router-tiles/src/map_data/graph.rs`
  - `MapDataElementRef::get` is never used

So the current code survives because the problematic helpers are not on the exercised path.

### 6) A later refactor already exists in the routing crate
The strongest evidence that this todo has been partly superseded is in `crates/ridi-router-routing`.

In `crates/ridi-router-routing/src/map_data/line.rs`:

- there is no `line_id()` helper
- `get_len_m()` has already been replaced by `len_m(&self, start: &MapDataPoint, end: &MapDataPoint)`

In `crates/ridi-router-routing/src/map_data/point.rs`:

- `distance_between(&self, point: &MapDataPoint)` takes an explicit resolved point, not a ref wrapper
- `Debug` no longer tries to dereference line refs into IDs

This is effectively the replacement design the todo was asking for.

## If still relevant

### The actual problem
The real problem is now narrower than the todo suggests:

- `crates/ridi-router-tiles/src/map_data/line.rs` still exposes old-style helper methods that hide dereference work
- those helpers are incompatible with the crate’s current generation-side ref model because `.get()` panics
- similar stale dereference-heavy debug code still exists in `point.rs` and `rule.rs`

### What behavior is missing or risky
What is risky is not current output correctness. The current generation path already avoids these helpers.

The risk is:

- future code may accidentally call these helpers and hit a panic
- the internal API still suggests that tile-generation refs are dereferenceable when they are not
- the tiles crate and routing crate now have diverging `MapDataLine` / `MapDataPoint` helper styles, which makes maintenance harder

### Possible solution options

#### Option A: Minimal cleanup in `line.rs`
- remove `line_id()`
- replace `get_len_m()` with `len_m(&self, start: &MapDataPoint, end: &MapDataPoint)`
- simplify `Debug` so it prints refs directly instead of dereferencing them

#### Option B: Broader cleanup across the old map-data helpers
Do Option A, and also:
- update `crates/ridi-router-tiles/src/map_data/point.rs` debug output to avoid `l.get().line_id()`
- update `crates/ridi-router-tiles/src/map_data/rule.rs` debug output to avoid `l.get().line_id()`
- consider changing point helper signatures to use explicit `&MapDataPoint` inputs, matching the routing crate

#### Option C: Remove or isolate dead generation-side legacy types
If `MapDataLine`, `MapDataRule`, and `point.lines` are no longer needed in tile generation, a larger cleanup could:
- reduce or remove unused legacy helpers/types from `crates/ridi-router-tiles/src/map_data`
- keep `GenerationGraph` / `GenerationLine` as the actual generation model

This is a broader refactor and probably separate from this todo.

### Tradeoffs / implications
- **Minimal cleanup** is low risk and aligns with the todo.
- **Broader cleanup** gives more consistency because the same hidden deref pattern exists in debug code outside `line.rs`.
- **Full dead-code removal** may be cleaner long term, but it needs a clearer decision about which internal types are still meant to exist in `ridi-router-tiles`.

### Recommended direction
Treat this todo as **internal cleanup plus API alignment**, not as an urgent functional fix.

Recommended next implementation direction:

1. Align `crates/ridi-router-tiles/src/map_data/line.rs` with the already-refactored routing version.
2. Remove hidden `.get()` usage from `line.rs` completely.
3. In the same pass, clean up `Debug` impls in `point.rs` and `rule.rs`, because they rely on the same invalid dereference pattern.

That keeps the change practical and consistent with the current architecture.

### Open questions
- Is `MapDataLine` still intended to be a meaningful generation-side type in `ridi-router-tiles`, or is it now mostly legacy scaffolding?
- Should the tiles crate deliberately mirror the routing crate’s `MapDataLine` / `MapDataPoint` helper API where possible?
- Are `point.lines` and `MapDataRule` expected to become real generation-time data later, or should their debug helpers be reduced to ref-only output now?

### Suggested implementation starting point
Start with these files:

1. `crates/ridi-router-tiles/src/map_data/line.rs`
   - remove `line_id()`
   - replace `get_len_m()` with explicit-input shape
   - simplify `Debug`
2. `crates/ridi-router-tiles/src/map_data/point.rs`
   - remove debug-time dereference of line refs
   - consider aligning `distance_between` / `bearing` signatures with the routing crate
3. `crates/ridi-router-tiles/src/map_data/rule.rs`
   - remove debug-time dereference of line refs
4. Compare against:
   - `crates/ridi-router-routing/src/map_data/line.rs`
   - `crates/ridi-router-routing/src/map_data/point.rs`

## Related files to inspect next
- `crates/ridi-router-tiles/src/map_data/line.rs`
- `crates/ridi-router-tiles/src/map_data/point.rs`
- `crates/ridi-router-tiles/src/map_data/rule.rs`
- `crates/ridi-router-tiles/src/map_data/graph.rs`
- `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
- `crates/ridi-router-routing/src/map_data/line.rs`
- `crates/ridi-router-routing/src/map_data/point.rs`
