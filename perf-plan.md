# Performance plan

Based on `perf.md` and a code pass over the hot path, the easiest wins are still in the walker/classification/tag stack.

## What the profile says

From `perf.md`:

- Biggest wall-clock buckets:
  - `walker::move_forward_to_next_fork_with_context`: **16.56 s**
  - `walker::classify_fork_segments_for_segment_with_context`: **9.30 s**
  - `walker::get_roundabout_exits_with_context`: **7.65 s**
  - `graph::get_tag_value`: **6.49 s**
  - `walker::get_fork_segments_for_segment_with_context`: **5.69 s**
  - `walker::move_backwards_to_prev_fork_with_context`: **4.23 s**
  - `route::is_back_on_road_within_distance`: **3.43 s**
  - `weights::weight_heading`: **3.05 s**
- Biggest alloc buckets:
  - `move_forward_to_next_fork_with_context`: **873.9 MB**
  - `get_roundabout_exits_with_context`: **625.9 MB**
  - `classify_fork_segments_for_segment_with_context`: **440.3 MB**
  - `get_fork_segments_for_segment_with_context`: **324.9 MB**
  - `move_backwards_to_prev_fork_with_context`: **307.8 MB**
  - `weight_heading`: **161.2 MB**

That matches the code: we are still doing a lot of repeated cloning and recomputation in the walker.

---

## Priority 1: stop cloning cached data inside walker classification

### Why this is easy
The code already has borrowed cache accessors in `RoutingContext`, but the hottest walker functions do not use them.

Relevant code:

- `crates/ridi-router-routing/src/routing_context.rs:67-80` has `with_point(...)`
- `crates/ridi-router-routing/src/routing_context.rs:160-172` has `with_adjacent(...)`
- but walker still clones data in:
  - `crates/ridi-router-routing/src/router/walker.rs:160-163`
  - `crates/ridi-router-routing/src/router/walker.rs:198-202`
  - `crates/ridi-router-routing/src/router/walker.rs:163`
  - `crates/ridi-router-routing/src/router/walker.rs:201`

Today the walker does all of this in the hot path:

- clones `center_point.rules`
- clones adjacency with `ctx.adjacent(...)`
- clones `MapDataLine` with `ctx.line(...)`

That is exactly the kind of churn that shows up as both wall time and alloc.

### Change

1. Rewrite `classify_segments_for_point_with_context(...)` to:
   - borrow point data with `with_point(...)`
   - borrow adjacency with `with_adjacent(...)`
   - iterate directly over borrowed slices
2. Add `RoutingContext::with_line(...)` and use that instead of `ctx.line(...)` in the walker hot loop.
3. Keep `Segment` construction at the boundary, but avoid cloning everything before we know a branch survives filters.

### Expected effect

- Good alloc win in `classify_fork_segments_for_segment_with_context`
- Small-to-medium wall-clock win in both classification and forward walking
- Low risk, because this is mostly an API usage cleanup, not routing logic changes

### Validation

Re-run the same perf route and compare:

- `classify_fork_segments_for_segment_with_context`
- `get_fork_segments_for_segment_with_context`
- `move_forward_to_next_fork_with_context`

---

## Priority 2: add a tag value cache in `RoutingContext`

### Why this is easy
`RoutingContext` already caches:

- points
- lines
- tag sets
- adjacency

But it does **not** cache decoded tag values.

Relevant code:

- `crates/ridi-router-routing/src/routing_context.rs:17-23` cache fields
- `crates/ridi-router-routing/src/routing_context.rs:149-150` just forwards `tag_value(...)`
- `crates/ridi-router-routing/src/map_data/graph.rs:415-436` returns `Option<String>`
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs:881-897` does `to_string()` on lookup

That means repeated tag reads allocate fresh strings over and over.

This lines up with the profile:

- `graph::get_tag_value`: **6.49 s**
- `tile_manager::get_tag_value_if_loaded`: **4.60 s**

### Change

1. Add `tag_values: HashMap<ElementTagValueRef, Option<smartstring::alias::String>>` to `RoutingCaches`.
2. Make `RoutingContext::tag_value(...)` cache the decoded value on first lookup.
3. Update the hot users first:
   - `weights.rs` helpers like `segment_name`, `segment_hw_ref`, `segment_highway`, `segment_surface`, `segment_smoothness`
   - `route::is_back_on_road_within_distance(...)`
   - any walker logic that hits tag reads while exploring

### Expected effect

- Strong alloc win
- Real wall-clock win, because the current code repeatedly decodes the same tag strings
- Helps several hotspots at once, not just one function

### Validation

Compare before/after on:

- `graph::get_tag_value`
- `tile_manager::get_tag_value_if_loaded`
- `route::is_back_on_road_within_distance`
- `weights::weight_heading`
- `weights::weight_no_short_detours`

---

## Priority 3: cache fork classification and roundabout exits by oriented segment

### Why this looks safe
The biggest repeated work is classification.

Relevant code:

- `crates/ridi-router-routing/src/router/walker.rs:182-221` classification
- `crates/ridi-router-routing/src/router/walker.rs:224-231` fork-segment wrapper
- `crates/ridi-router-routing/src/router/walker.rs:237-286` roundabout exit generation

Also, `classify_fork_segments_for_segment_with_context(...)` currently looks back into route history at `walker.rs:189-197` to find the previous point, even though for a normal oriented segment the previous point is already implied by the line endpoints plus the segment end.

So there is a good chance the result is cacheable by oriented segment (`line_ref + end_point`) instead of recomputing it millions of times.

### Change

1. Add a small cache keyed by oriented segment for:
   - fork classification result
   - roundabout exits result
2. Derive the incoming point from the current segment instead of route history where possible.
3. Reuse the cached result from:
   - `move_forward_to_next_fork_with_context(...)`
   - `move_backwards_to_prev_fork_with_context(...)`
   - roundabout traversal helpers

### Expected effect

- High wall-clock upside
- Medium alloc upside
- Especially helpful because backtracking revisits the same junctions repeatedly

### Validation

Watch these first:

- `classify_fork_segments_for_segment_with_context`
- `get_fork_segments_for_segment_with_context`
- `get_roundabout_exits_with_context`
- `move_backwards_to_prev_fork_with_context`

---

## Priority 4: remove obvious allocation churn in roundabout handling

### Why this is easy
`get_roundabout_exits_with_context(...)` is doing several short-lived allocations per call.

Relevant code:

- `crates/ridi-router-routing/src/router/walker.rs:242` allocates a `HashSet`
- `crates/ridi-router-routing/src/router/walker.rs:247` allocates `Vec<Vec<Segment>>` shape indirectly
- `crates/ridi-router-routing/src/router/walker.rs:254` converts `SegmentList` into `Vec`
- `crates/ridi-router-routing/src/router/walker.rs:256-266` builds another temporary `Vec`
- `crates/ridi-router-routing/src/router/walker.rs:286` flattens into yet another `Vec`

For a function that is already at **625.9 MB** cumulative alloc, this is low-hanging fruit.

### Change

1. Accumulate exits into one flat buffer instead of `Vec<Vec<Segment>>` + flatten.
2. Prefer `SmallVec<[Segment; 4]>` or similar for the common case.
3. Do the same cleanup in `move_to_roundabout_exit_with_context(...)`, which has similar `SegmentList -> Vec` churn.
4. If Priority 3 lands, have roundabout lookup return cached exits directly.

### Expected effect

- Clear alloc reduction
- Some wall-clock improvement from less copying and less allocator traffic
- Very local change, easy to benchmark in isolation

---

## Priority 5: short-circuit expensive per-fork weight evaluation

### Why this is easy
Navigator currently computes **all** per-fork weights, collects them into a `Vec`, and only then decides whether the fork is already dead.

Relevant code:

- `crates/ridi-router-routing/src/router/navigator.rs:253-274`
- `crates/ridi-router-routing/src/router/navigator.rs:99-132`
- per-fork weight order is set in `crates/ridi-router-routing/src/router/generator.rs:369-429`

Right now that means a fork can be rejected early by a cheap rule, but we still go on to run expensive work like:

- `weight_no_short_detours(...)`
- `weight_heading(...)`

### Change

1. Replace `map(...).collect::<Vec<_>>()` with a simple loop.
2. Stop immediately on:
   - `LastSegmentDoNotUse`
   - `ForkChoiceDoNotUse`
3. Accumulate total weight directly instead of building a temporary `Vec<WeightCalcResult>`.
4. Keep expensive look-ahead weights late in the order.

### Expected effect

- Easy wall-clock win
- Small alloc win
- Low risk, because it preserves the existing semantics

### Validation

Watch:

- `weights::weight_heading`
- `weights::weight_no_short_detours`
- overall route wall time

---

## Secondary cleanup: make backward road-name scan cheaper

This is not the first thing I would do, but it is an easy follow-up once tag caching is in place.

Relevant code:

- `crates/ridi-router-routing/src/router/route/mod.rs:489-537`
- `crates/ridi-router-routing/src/router/weights.rs:256-275`

`is_back_on_road_within_distance(...)` scans backwards and repeatedly does:

- `ctx.line(...)`
- `ctx.tag_set(...)`
- `ctx.tag_value(...)`

That is a bad fit for a function called **54,138** times.

### Change

1. After tag caching lands, re-measure.
2. If still hot, add a faster road-identity path:
   - compare cached tag refs first where possible
   - fall back to decoded string comparison only when needed
3. Consider a borrowed slice helper for route chunks to avoid repeated `to_vec()` cloning from:
   - `crates/ridi-router-routing/src/router/route/mod.rs:447-467`

---

## What I would do first

### Pass 1

1. Borrowed access in walker hot loops
2. Tag value cache in `RoutingContext`
3. Per-fork weight short-circuiting

These are the easiest, lowest-risk changes with the best chance of helping both wall time and alloc.

### Pass 2

4. Roundabout allocation cleanup
5. Cache classification / roundabout exits by oriented segment

These likely have bigger upside, but they touch routing behavior a bit more, so I would land them after Pass 1 unless you want to swing for the larger walker win immediately.

---

## What not to spend the next cycle on

- adjacency plumbing
- tile loading broad rewrites
- top-level navigator orchestration

The current profile still says the main problem is repeated walker work, repeated branch classification, roundabout handling, and repeated tag decoding inside those paths.
