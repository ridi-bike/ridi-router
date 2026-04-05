# Review: shallow formatting in `crates/ridi-router-tiles`

## Verdict: relevant

## Summary

The todo is still relevant, but the wording is partially stale.

The core concern is real: `Debug` formatting in the tiles crate still dereferences tile-generation refs via `.get()`, which is exactly what the todo says to avoid. In the tiles crate, those refs are explicitly **not dereferenceable**: `MapDataElementRef::get()` panics in `crates/ridi-router-tiles/src/map_data/graph.rs`.

So this is not just a style issue. In the current code, formatting some `MapDataPoint`, `MapDataLine`, and `MapDataRule` values can panic if they contain refs.

The stale part of the todo is that it names `point.rs` and `line.rs`, but `rule.rs` has the same problem and should be included. Also, `Display` is already shallow in the affected types; the main issue is `Debug`.

## Current evidence from the codebase

### 1) Tile-generation refs are intentionally non-dereferenceable

In `crates/ridi-router-tiles/src/map_data/graph.rs`:

- `MapDataElementRef::get()` panics with `"tile-generation refs are not dereferenceable"`.
- `Display for MapDataElementRef<T>` is already shallow and structural: it prints `Ref(tile:{:?}, id:{})`.

This means any formatting path that calls `.get()` on these refs is incompatible with the current tiles-crate model.

Relevant file:
- `crates/ridi-router-tiles/src/map_data/graph.rs`

### 2) `MapDataPoint` `Debug` still dereferences line refs

In `crates/ridi-router-tiles/src/map_data/point.rs`, `impl Debug for MapDataPoint` builds `lines={:?}` by doing:

- `self.lines.iter()`
- `.map(|l| l.get().line_id())`

That is a ref dereference during formatting.

It also computes `junction` via `self.is_junction()`, which is cheap and structural, so that part is fine. The risky part is the `.get()` chain used to print line IDs.

Relevant file:
- `crates/ridi-router-tiles/src/map_data/point.rs`

### 3) `MapDataLine` `Debug` still dereferences endpoint refs

In `crates/ridi-router-tiles/src/map_data/line.rs`, `impl Debug for MapDataLine` prints:

- `id={}` via `self.line_id()`
- `points=({},{})` via `self.points.0.get().id` and `self.points.1.get().id`

`line_id()` itself also dereferences both endpoint refs:

- `format!("{}-{}", self.points.0.get().id, self.points.1.get().id)`

So the current `Debug` impl is not shallow.

Relevant file:
- `crates/ridi-router-tiles/src/map_data/line.rs`

### 4) `MapDataRule` `Debug` also dereferences refs

In `crates/ridi-router-tiles/src/map_data/rule.rs`, `impl Debug for MapDataRule` formats line IDs by doing:

- `.map(|l| l.get().line_id())`

This has the same problem as the point and line formatting.

Relevant file:
- `crates/ridi-router-tiles/src/map_data/rule.rs`

### 5) `Display` is already shallow where it exists

The current `Display` impls in the tiles crate are already structurally shallow:

- `crates/ridi-router-tiles/src/map_data/point.rs` prints `Point({}: {}, {})`
- `crates/ridi-router-tiles/src/map_data/line.rs` prints `Line({}-{})`, relying on `Display` for the refs, which is already shallow

So the todo text is a bit too broad when it says `Debug / Display`. The practical issue is mostly `Debug`.

### 6) There is already a shallow pattern in the routing crate

The corresponding files in `crates/ridi-router-routing/src/map_data/` already use shallow `Debug` implementations:

- `point.rs`: `Debug` uses `debug_struct` and prints counts/fields, not dereferenced refs
- `line.rs`: `Debug` prints `points`, `direction`, and `tags` directly
- `rule.rs`: `Debug` prints `rule_type`, `from_line_count`, and `to_line_count`

This strongly suggests the tiles crate was not updated to match a later refactor that already happened in the routing crate.

Relevant files:
- `crates/ridi-router-routing/src/map_data/point.rs`
- `crates/ridi-router-routing/src/map_data/line.rs`
- `crates/ridi-router-routing/src/map_data/rule.rs`

### 7) The crate currently compiles and tests pass, but that does not prove formatting is safe

`cargo test -p ridi-router-tiles` passes today.

That means the issue is not currently covered by failing tests. It does **not** mean the formatting is safe; it more likely means these `Debug` paths are not exercised in tests.

## The actual problem

The tiles crate now models refs as opaque identifiers during tile generation, but some `Debug` implementations still assume those refs can be resolved into full objects.

That creates two concrete risks:

1. **Formatting can panic** in normal debugging/logging flows because `.get()` on tile-generation refs panics.
2. **Formatting is semantically misleading** because it suggests ambient lookup is available when the tiles crate explicitly says it is not.

## What behavior is missing or risky

Missing behavior:

- Safe, shallow `Debug` formatting for map-data types in the tiles crate.
- Consistent formatting semantics between the tiles crate and the routing crate.

Risky behavior:

- `Debug` on `MapDataPoint`, `MapDataLine`, and `MapDataRule` can invoke `.get()` on non-dereferenceable refs.
- Any future logging, tracing, assertion failure, or diagnostic dump that prints these types may fail unexpectedly.

## Possible solution options

### Option A: Port the shallow `Debug` style from `ridi-router-routing`

Adopt the same style already used in the routing crate:

- `MapDataPoint`: print structural fields plus counts
- `MapDataLine`: print `points`, `direction`, `tags`
- `MapDataRule`: print `rule_type`, `from_line_count`, `to_line_count`

This is the lowest-risk option because there is already a local example of the intended shape.

### Option B: Keep more detail, but only from structural ref data

Instead of counts only, print the refs directly:

- point `lines: Vec<MapDataLineRef>`
- line `points: (MapDataPointRef, MapDataPointRef)`
- rule `from_lines` / `to_lines` as ref values

This stays shallow because `MapDataElementRef` already has structural `Debug`/`Display` support.

### Option C: Remove helper-derived IDs from formatting only

A narrower change would leave helper methods like `line_id()` alone, but stop using them from `Debug`.

That would satisfy the todo’s formatting goal without changing non-formatting helpers.

## Tradeoffs / implications

- **Option A** gives the cleanest and most stable output, but it shows less detail than the current formatting intended to show.
- **Option B** preserves more inspectability while still avoiding hidden resolution. It may produce noisier output.
- **Option C** is the smallest change if the goal is strictly about formatting, but it leaves related non-shallow helper methods in place, which may keep the old assumptions alive elsewhere.

A separate judgment call is whether `MapDataLine::line_id()` should remain in the tiles crate in its current form. It is not part of formatting, but it still depends on dereferencing refs, so it does not fit well with the current model.

## Recommended direction

Treat this todo as still active and expand it slightly:

1. Update `Debug` impls in:
   - `crates/ridi-router-tiles/src/map_data/point.rs`
   - `crates/ridi-router-tiles/src/map_data/line.rs`
   - `crates/ridi-router-tiles/src/map_data/rule.rs`
2. Make them shallow and structural, following the routing-crate pattern unless there is a clear reason to keep more detail.
3. Leave current `Display` impls alone unless a concrete inconsistency is found.
4. Consider a follow-up todo for non-formatting helpers like `MapDataLine::line_id()` if they are not valid under the current ref model.

## Open questions

1. Should tiles-crate `Debug` match routing-crate `Debug` exactly, or only in spirit?
2. Is `MapDataLine::line_id()` still used or intended to be used in the tiles crate?
3. Do we want future tests that explicitly format representative `MapDataPoint` / `MapDataLine` / `MapDataRule` values to prevent regressions?
4. Should `MapDataRule` output show counts only, or shallow ref lists for debugging convenience?

## Suggested implementation starting point

Start with the three `Debug` impls in:

- `crates/ridi-router-tiles/src/map_data/point.rs`
- `crates/ridi-router-tiles/src/map_data/line.rs`
- `crates/ridi-router-tiles/src/map_data/rule.rs`

Use the routing crate as the comparison target:

- `crates/ridi-router-routing/src/map_data/point.rs`
- `crates/ridi-router-routing/src/map_data/line.rs`
- `crates/ridi-router-routing/src/map_data/rule.rs`

After that, re-run:

- `cargo test -p ridi-router-tiles`

If touching anything beyond formatting, also review:

- `crates/ridi-router-tiles/src/map_data/graph.rs`
- `crates/ridi-router-tiles/src/map_data/line.rs`

## Related files to inspect next

- `crates/ridi-router-tiles/src/map_data/point.rs`
- `crates/ridi-router-tiles/src/map_data/line.rs`
- `crates/ridi-router-tiles/src/map_data/rule.rs`
- `crates/ridi-router-tiles/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/map_data/point.rs`
- `crates/ridi-router-routing/src/map_data/line.rs`
- `crates/ridi-router-routing/src/map_data/rule.rs`
