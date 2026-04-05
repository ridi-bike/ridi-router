# Review: remove `MapDataElementRef<T>::get()` in `crates/ridi-router-tiles`

## Verdict: partially relevant

## Summary
The todo is still aimed at a real problem, but the framing is now partly stale.

`crates/ridi-router-tiles/src/map_data/graph.rs` still defines `MapDataElementRef<T>::get()`, and that method is still a panic stub: `panic!("tile-generation refs are not dereferenceable")`.

So the API problem described in the todo is real: the tiles crate still exposes a dereference-style method on a type that is not actually dereferenceable.

However, the active tile-generation path has already moved away from relying on ambient ref dereference. Current generation uses `GenerationGraph` and `GenerationLine`, and the RMDF writer resolves point data explicitly from node IDs. That means this is no longer a production-path blocker. It is mostly cleanup and API alignment work, with the important caveat that stale callers still exist in the tiles crate source and would panic if exercised.

## Current evidence from the codebase

### 1. `MapDataElementRef<T>::get()` still exists and still panics
In `crates/ridi-router-tiles/src/map_data/graph.rs`:

- `MapDataElementRef<T>` stores only `tile_id` and `element_id`
- it exposes `get_tile_id()` and `get_element_id()`
- it also still exposes `get()`
- `get()` is implemented as:
  - `panic!("tile-generation refs are not dereferenceable")`

That directly matches the todo's stated problem.

### 2. There are still callers inside `ridi-router-tiles`
The method is not just leftover API surface. Source in the tiles crate still calls it.

Examples:

- `crates/ridi-router-tiles/src/map_data/point.rs`
  - `distance_between(&self, point: &MapDataPointRef)` uses `point.get().lon` and `point.get().lat`
  - `bearing(&self, point: &MapDataPointRef)` does the same
  - `Debug` maps `self.lines` with `l.get().line_id()`

- `crates/ridi-router-tiles/src/map_data/line.rs`
  - `line_id()` uses `self.points.0.get().id` and `self.points.1.get().id`
  - `get_len_m()` calls `self.points.0.get().distance_between(&self.points.1)`
  - `Debug` also dereferences both point refs through `.get()`

- `crates/ridi-router-tiles/src/map_data/rule.rs`
  - `Debug` maps `from_lines` and `to_lines` with `l.get().line_id()`

So the todo text is correct that the crate still advertises and uses an old dereference model.

### 3. The active generation flow already uses explicit resolved values instead
In `crates/ridi-router-tiles/src/map_data/generation_graph.rs`:

- generation builds `GenerationLine`
- `GenerationLine` stores explicit `from_node_id` and `to_node_id`
- there is an explicit comment that `point.lines` is not updated because `MapDataPoint.lines` expects `MapDataLineRef`
- the comment also says the writer will build line refs when serializing to RMDF format

In `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`:

- `serialize_lines()` builds a `node_map` from `graph.get_points()`
- it resolves `point_a` from `line.from_node_id`
- it resolves `point_b` from `line.to_node_id`
- it writes line records from those explicitly resolved points

This is already the explicit-resolution style the todo wants.

### 4. A later refactor already exists in the routing crate
The routing crate shows what the replacement design looks like.

In `crates/ridi-router-routing/src/map_data/graph.rs`:

- `MapDataElementRef<T>` has `get_tile_id()` and `get_element_id()`
- there is no `get()` method

In `crates/ridi-router-routing/src/routing_context.rs`:

- refs are resolved explicitly through context methods like `point(&MapDataPointRef)` and `line(&MapDataLineRef)`

In `crates/ridi-router-routing/src/map_data/point.rs` and `crates/ridi-router-routing/src/map_data/line.rs`:

- geometry helpers take resolved `&MapDataPoint` values
- debug output no longer dereferences refs through `.get()`

So the tiles todo overlaps with a refactor pattern that already landed elsewhere in the repo.

### 5. Current tests pass because the bad API is effectively dead on the exercised path
`cargo test -p ridi-router-tiles` currently passes.

The warnings from that run are useful evidence:

- `crates/ridi-router-tiles/src/map_data/graph.rs`
  - `MapDataElementRef::new`, `get_tile_id`, and `get_element_id` are never used
- `crates/ridi-router-tiles/src/map_data/line.rs`
  - `is_roundabout` and `get_len_m` are never used
- `crates/ridi-router-tiles/src/map_data/point.rs`
  - `distance_between` and `bearing` are never used

That strongly suggests the problematic dereference-based helpers are not on the currently tested generation path.

## If still relevant

### The actual problem
The real problem is not that tile generation is currently broken.

The real problem is that `ridi-router-tiles` still contains a stale internal ref API:

- `MapDataElementRef<T>::get()` claims dereference semantics that are invalid in this crate
- several helper and debug methods still depend on that invalid API
- future use of those helpers would panic at runtime
- the tiles crate still communicates the wrong model to maintainers: refs look dereferenceable, but only ID access is actually supported

### What behavior is missing or risky
Missing behavior:

- there is no legitimate way in the tiles crate to turn `MapDataElementRef<T>` into `T`
- callers that need actual values must already have some explicit resolution context, but the old helper signatures still hide that requirement

Risk:

- someone may call these helpers later and hit the panic
- dead code and live code still mix two incompatible models: ID-only refs vs dereference-style helpers
- the todo acceptance criterion "no code in the tiles crate depends on ambient ref dereference" is not yet true at source level, even though it is mostly true in the active path

### Possible solution options

#### Option A: remove `get()` and fix direct callers in the tiles crate
- delete `MapDataElementRef<T>::get()` from `crates/ridi-router-tiles/src/map_data/graph.rs`
- update `point.rs`, `line.rs`, and `rule.rs` so they no longer dereference refs
- prefer printing refs directly in `Debug` rather than expanding them
- change geometry helpers to take resolved values instead of refs, or remove unused helpers if they are generation-side dead code

#### Option B: first remove/refactor the stale helpers, then delete `get()`
- convert or remove helper methods that currently need `.get()`
- once all source callers are gone, remove `MapDataElementRef<T>::get()`

This is mechanically safer if the goal is to keep changes easy to review.

#### Option C: larger cleanup of legacy map-data types in `ridi-router-tiles`
- decide whether `MapDataLine`, `MapDataRule`, and some helper/debug code are still needed at all in the generation crate
- if not, remove more of the legacy surface rather than only `get()`

This is broader than the todo and may belong in a separate follow-up.

### Tradeoffs / implications
- **Minimal fix** removes the misleading API and panic path quickly.
- **Broader cleanup** gives better consistency because `.get()` usage is spread across helper and debug code, not just one method.
- **Matching the routing crate** reduces conceptual drift across crates and gives a clear replacement model: ID refs plus explicit resolution context.
- Some code in `ridi-router-tiles/src/map_data/*` appears lightly used or currently unused, so deleting or reshaping helpers may be low-risk but should still be validated by tests.

### Recommended direction
Treat the todo as **still relevant, but narrower than originally written**.

Recommended direction:

1. Use the routing crate as the model.
2. Remove all tiles-crate source calls to `MapDataElementRef<T>::get()`.
3. Replace dereference-heavy helpers with either:
   - explicit resolved-value parameters, or
   - simpler debug output that prints refs/IDs only.
4. After source callers are gone, delete `MapDataElementRef<T>::get()`.
5. Re-run `cargo test -p ridi-router-tiles`.

This keeps the work practical and aligned with the code that already exists in `ridi-router-routing`.

### Open questions
- Are `MapDataLine::line_id()`, `MapDataLine::get_len_m()`, `MapDataPoint::distance_between()`, and `MapDataPoint::bearing()` intended to survive in the tiles crate, or are they just leftover legacy helpers?
- Should `Debug` impls in `point.rs`, `line.rs`, and `rule.rs` preserve the same level of detail, or is ref/ID-based output enough?
- Is there any non-test or downstream code path outside this crate that still expects `MapDataElementRef<T>::get()` from `ridi-router-tiles` specifically?

### Suggested implementation starting point
Start with a caller inventory in these files:

- `crates/ridi-router-tiles/src/map_data/point.rs`
- `crates/ridi-router-tiles/src/map_data/line.rs`
- `crates/ridi-router-tiles/src/map_data/rule.rs`
- `crates/ridi-router-tiles/src/map_data/graph.rs`

Then compare each helper with its routing-crate counterpart in:

- `crates/ridi-router-routing/src/map_data/point.rs`
- `crates/ridi-router-routing/src/map_data/line.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/routing_context.rs`

## Related files to inspect next
- `crates/ridi-router-tiles/src/map_data/graph.rs`
- `crates/ridi-router-tiles/src/map_data/point.rs`
- `crates/ridi-router-tiles/src/map_data/line.rs`
- `crates/ridi-router-tiles/src/map_data/rule.rs`
- `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/map_data/point.rs`
- `crates/ridi-router-routing/src/map_data/line.rs`
- `crates/ridi-router-routing/src/routing_context.rs`
