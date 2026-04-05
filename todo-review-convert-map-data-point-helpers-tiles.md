# Review: `MapDataPoint` helper conversion in `crates/ridi-router-tiles`

## Verdict: partially relevant

## Summary
The todo is **still relevant inside `ridi-router-tiles`**, but it is also **stale in scope**.

The core issue is real: `crates/ridi-router-tiles/src/map_data/point.rs` still defines `distance_between` and `bearing` in terms of `&MapDataPointRef`, and both helpers call `.get()` internally. That means the geometry helpers are not pure value-to-value operations.

However, the todo overstates the current impact:

- in `ridi-router-tiles`, these helpers are currently **dead code** according to compiler warnings from `cargo test -p ridi-router-tiles`
- the same design has **already been cleaned up in `crates/ridi-router-routing`**, where the equivalent helpers take `&MapDataPoint`
- in `ridi-router-tiles`, the current ref dereference is not just hidden lookup behavior; `MapDataElementRef::get()` is a **panic stub** in this crate, so these helpers are unusable if they are ever called

So this is not obsolete, but it is not an urgent behavior regression either. It is best treated as a cleanup / consistency task before anyone starts using those helpers again.

## Current evidence from the codebase

### 1. The todo's stated problem still exists in `ridi-router-tiles`
In `crates/ridi-router-tiles/src/map_data/point.rs`:

- `distance_between(&self, point: &MapDataPointRef) -> f32` at lines 27-30
- `bearing(&self, point: &MapDataPointRef) -> f32` at lines 32-35

Both methods build the second point from `point.get().lon` and `point.get().lat`, so they hide ref dereferencing inside geometry code.

### 2. `MapDataPointRef::get()` is not a real lookup in this crate
In `crates/ridi-router-tiles/src/map_data/graph.rs:207-209`:

- `MapDataElementRef<T>::get(&self) -> T` is implemented as
  `panic!("tile-generation refs are not dereferenceable")`

That makes the current helper signatures especially misleading in `ridi-router-tiles`: they suggest a normal geometric operation, but the implementation would panic if actually exercised.

### 3. The only in-crate caller is another dead helper
`crates/ridi-router-tiles/src/map_data/line.rs:36-38` defines:

- `get_len_m(&self) -> f32`
- implemented as `self.points.0.get().distance_between(&self.points.1)`

So `MapDataLine::get_len_m()` also depends on the same hidden dereference pattern and would also panic if used.

A repository search only found these tile-side usages:

- `crates/ridi-router-tiles/src/map_data/point.rs`
- `crates/ridi-router-tiles/src/map_data/line.rs`

There were no active tile-generation call sites beyond that.

### 4. The routing crate already uses the desired design
In `crates/ridi-router-routing/src/map_data/point.rs`:

- `distance_between(&self, point: &MapDataPoint) -> f32` at lines 30-32
- `bearing(&self, point: &MapDataPoint) -> f32` at lines 34-35

In `crates/ridi-router-routing/src/map_data/line.rs:36-38`:

- `len_m(&self, start: &MapDataPoint, end: &MapDataPoint) -> f32`
- implemented as `start.distance_between(end)`

And routing call sites resolve refs before calling geometry helpers, for example:

- `crates/ridi-router-routing/src/router/generator.rs:68-76`
- `crates/ridi-router-routing/src/router/itinerary.rs:108-136`
- `crates/ridi-router-routing/src/router/weights.rs:44-52`
- `crates/ridi-router-routing/src/router/route/segment.rs:22-31`

That strongly suggests the intended refactor has already happened in the runtime/routing path, but not in the tile-generation crate.

### 5. Tests pass, but they do not validate this area
Running `cargo test -p ridi-router-tiles` passed, but the compiler emitted warnings that these methods are unused:

- `crates/ridi-router-tiles/src/map_data/point.rs`: `distance_between` and `bearing` are never used
- `crates/ridi-router-tiles/src/map_data/line.rs`: `get_len_m` is never used

So the acceptance criterion "crate still compiles and tests pass" is already true today, even though the problematic helper signatures remain.

## The actual problem
The real issue is narrower than the todo text implies:

1. `ridi-router-tiles` still contains geometry helper APIs that mix value math with ref dereferencing.
2. In this crate, those dereferences are invalid by design because `MapDataElementRef::get()` panics.
3. The code is currently unused, so the problem is mostly about misleading internal APIs, latent panic risk, and divergence from the already-cleaner routing implementation.

## What behavior is missing or risky

- The tile crate has no safe, value-based geometry helper API for `MapDataPoint` that matches the routing crate.
- If a future caller starts using `MapDataPoint::distance_between`, `MapDataPoint::bearing`, or `MapDataLine::get_len_m` in `ridi-router-tiles`, that code is likely to panic.
- The duplicated `map_data` model between `ridi-router-tiles` and `ridi-router-routing` has already drifted, which increases maintenance cost and confusion.

## Possible solution options

### Option A: Align `ridi-router-tiles` with `ridi-router-routing`
Change tile-side helpers to take resolved values:

- `MapDataPoint::distance_between(&MapDataPoint)`
- `MapDataPoint::bearing(&MapDataPoint)`
- update `MapDataLine::get_len_m()` accordingly, likely to a value-based API similar to routing's `len_m(&start, &end)`

This is the most direct implementation of the todo.

### Option B: Remove the unused tile-side helpers entirely
Because the methods are currently unused and the crate's refs are intentionally non-dereferenceable, another reasonable option is to delete the dead helpers instead of refactoring them.

This would reduce misleading API surface, but it would not preserve a geometry helper interface in the tile crate.

### Option C: Keep helpers but make the panic impossible to miss
This is the weakest option: leave behavior mostly as-is but rename or document the helpers to make the lookup requirement explicit.

Given the routing crate already shows a better pattern, this is probably not worth doing.

## Tradeoffs / implications

- **Option A** improves consistency with `ridi-router-routing` and keeps the helpers available for future use.
- **Option B** is simpler if tile generation truly does not need these helpers.
- If `MapDataLine::get_len_m()` stays zero-argument while refs remain unresolved, the hidden lookup problem will persist somewhere else. The line API likely needs to change too, not just the point API.
- Because `map_data` is private to `ridi-router-tiles` (`crates/ridi-router-tiles/src/lib.rs:1` declares `mod map_data;`, not `pub mod map_data;`), this is an internal refactor, not a public API break for external crate users.

## Recommended direction
Recommend **Option A**, with one adjustment:

- treat this as an internal consistency cleanup
- explicitly include `MapDataLine::get_len_m()` in scope, because it currently preserves the same hidden dereference pattern
- use `crates/ridi-router-routing` as the reference implementation

So the todo is still worth doing, but the implementation plan should say:

- the current tile-side helpers are unused
- the refactor is mainly to remove misleading / panic-prone dead APIs and align with routing
- any caller updates will likely be minimal inside `ridi-router-tiles`, because there are almost no current callers

## Open questions

1. Should `ridi-router-tiles` keep these geometry helpers at all, or should it remove them as dead code?
2. If kept, should `MapDataLine::get_len_m()` become a routing-style `len_m(&start, &end)` API?
3. Is there a broader follow-up to reduce duplication between `crates/ridi-router-tiles/src/map_data` and `crates/ridi-router-routing/src/map_data`?

## Suggested implementation starting point

1. Start with:
   - `crates/ridi-router-tiles/src/map_data/point.rs`
   - `crates/ridi-router-tiles/src/map_data/line.rs`
2. Mirror the signatures and implementation style from:
   - `crates/ridi-router-routing/src/map_data/point.rs`
   - `crates/ridi-router-routing/src/map_data/line.rs`
3. Decide early whether to:
   - refactor the helpers to value-based APIs, or
   - delete them if they are confirmed unnecessary
4. After that, run:
   - `rg "distance_between\(|bearing\(|get_len_m\(" crates/ridi-router-tiles/src`
   - `cargo test -p ridi-router-tiles`

## Related files to inspect next

- `crates/ridi-router-tiles/src/map_data/point.rs`
- `crates/ridi-router-tiles/src/map_data/line.rs`
- `crates/ridi-router-tiles/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/map_data/point.rs`
- `crates/ridi-router-routing/src/map_data/line.rs`
- `crates/ridi-router-routing/src/router/generator.rs`
- `crates/ridi-router-routing/src/router/itinerary.rs`
- `crates/ridi-router-routing/src/router/weights.rs`
- `crates/ridi-router-routing/src/router/route/segment.rs`
