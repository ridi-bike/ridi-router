# Review: `ElementTagSetRef::get()` in `crates/ridi-router-tiles`

## Verdict
Relevant

## Summary
The todo is still relevant.

`ElementTagSetRef::get()` still exists in `crates/ridi-router-tiles/src/map_data/graph.rs`, and it still returns a fabricated `ElementTagSet` whose fields are all `NONE` refs rather than resolving real data. That matches the problem statement in the todo.

At the same time, the codebase has already moved in the direction the todo wants: in the tile-generation crate, tag sets are handled owner-side through `GenerationGraph.tags` and serialized directly from that storage. I did not find any real caller in `ridi-router-tiles` that depends on `ElementTagSetRef::get()`. So the todo is not blocked by existing usage, and part of the acceptance criteria is already true.

## Current evidence from the codebase

### 1) The method still exists and returns fake data
In `crates/ridi-router-tiles/src/map_data/graph.rs`:

- `ElementTagSetRef` is defined at `crates/ridi-router-tiles/src/map_data/graph.rs:47`
- `ElementTagSetRef::get()` is defined at `crates/ridi-router-tiles/src/map_data/graph.rs:60`
- Its implementation constructs a new `ElementTagSet` with every field set to `ElementTagValueRef::none(self.tile_id)` at `crates/ridi-router-tiles/src/map_data/graph.rs:61-67`

That means the method does not resolve the referenced tag set index at all.

### 2) The tile-generation path already uses explicit owner-side storage
The actual tag materialization in the tiles crate happens through `ElementTags` on `GenerationGraph`, not through `ElementTagSetRef::get()`:

- `GenerationGraph` stores tag ownership in `pub(crate) tags: ElementTags` at `crates/ridi-router-tiles/src/map_data/generation_graph.rs:27`
- Ways create or deduplicate tag sets via `self.tags.get_or_create(...)` at `crates/ridi-router-tiles/src/map_data/generation_graph.rs:100-103`
- `ElementTags::get_or_create(...)` builds and stores `ElementTagSet` values in `tag_sets` and returns only an index ref at `crates/ridi-router-tiles/src/map_data/graph.rs:126-155`

This is already the owner-side model the todo describes.

### 3) Serialization reads from the owner, not from the ref
When writing RMDF, the code does not call `ElementTagSetRef::get()`. It uses the index on the ref and serializes tag sets from `graph.get_tags().tag_sets`:

- line records write `tag_set_index: line.tags.tag_set_idx` at `crates/ridi-router-tiles/src/rmdf/generator/writer.rs:275-290`
- tag-set serialization iterates `graph.get_tags().tag_sets` at `crates/ridi-router-tiles/src/rmdf/generator/writer.rs:358-373`
- tag-value serialization iterates `graph.get_tags().tag_values` at `crates/ridi-router-tiles/src/rmdf/generator/writer.rs:337-356`

So the real implementation path already treats `ElementTagSetRef` as a lightweight identifier.

### 4) I did not find any tiles-crate caller that relies on self-resolution
Repository search in `crates/ridi-router-tiles/src` found the method definition, but no call sites for `ElementTagSetRef::get()`.

Also, `cargo test -p ridi-router-tiles` passes, and the compiler currently warns that the method is unused:

- warning for `ElementTagSetRef::get` being unused in `crates/ridi-router-tiles/src/map_data/graph.rs:60`

That is strong evidence that removing this method should be low-risk inside this crate.

### 5) There is a later runtime-side mechanism in a different crate
The workspace has a separate runtime crate, `ridi-router-routing`, where tag refs are resolved owner-side through `MapDataGraph` / `TileManager` instead of through the ref type itself:

- `RoutingContext::tag_set(...)` delegates to the graph at `crates/ridi-router-routing/src/routing_context.rs:32-34`
- `MapDataGraph::get_tag_set(...)` resolves a `TagSetRecord` from tile storage at `crates/ridi-router-routing/src/map_data/graph.rs:416-439`

This is relevant because it shows the intended direction already exists elsewhere in the workspace: refs stay lightweight, and a graph/context owner performs resolution.

## The actual problem
`ElementTagSetRef::get()` advertises behavior it does not provide.

The ref contains `tile_id` and `tag_set_idx`, but in `ridi-router-tiles` there is no backing owner attached to the ref that could resolve those values. The method therefore fabricates an all-`NONE` result. That is misleading because:

- it makes a broken operation look valid
- it can silently erase tag information if someone starts using it later
- it works against the current architecture, where tag sets are owned by `ElementTags` and serialized from there

There is a smaller related issue nearby: `ElementTagValueRef::get()` in the same file also returns `None` unconditionally at `crates/ridi-router-tiles/src/map_data/graph.rs:41-43`. That is outside the todo’s exact scope, but it is the same design smell.

## What behavior is missing or risky
What is missing is not tag-set resolution in general; that already exists through owner-side storage and, in the runtime crate, through graph-backed lookup.

What is risky is leaving a fake convenience API in place:

- future code in `ridi-router-tiles` could call `ref.get()` and accidentally drop real tag data
- the method obscures the real contract: `ElementTagSetRef` is just an identifier during generation
- it creates inconsistency with `ridi-router-routing`, where resolution is explicit and graph-backed

## Possible solution options

### Option A: delete only `ElementTagSetRef::get()`
Remove the fake method and keep the rest of the model unchanged.

This is the smallest change and appears sufficient for the todo’s stated goal.

### Option B: delete `ElementTagSetRef::get()` and add an explicit owner-side accessor on `ElementTags`
For example, add a helper on the owning collection such as a `get_tag_set`-style method that takes an `ElementTagSetRef` or index.

This would make legitimate owner-side lookup explicit inside the generation crate if future code needs it.

### Option C: broader cleanup, also removing or quarantining other fake dereference helpers
`ElementTagValueRef::get()` in the tiles crate has the same issue. If the team wants consistency, both fake `get()` methods could be addressed together.

This is broader than the todo and may be better handled as a follow-up unless the implementation is trivial.

## Tradeoffs / implications

### If only `ElementTagSetRef::get()` is removed
Pros:
- matches the current architecture
- low-risk because there are no current call sites in `ridi-router-tiles`
- removes a misleading API immediately

Cons:
- leaves `ElementTagValueRef::get()` as a similar misleading API in the same file

### If explicit owner-side lookup is added
Pros:
- gives future code a correct path if it really needs tag-set materialization during generation
- makes ownership clearer

Cons:
- adds API surface that may not be needed today
- may encourage unnecessary readback from `ElementTags` if serialization remains the only consumer

## Recommended direction
Proceed with the todo, but treat it as a small cleanup rather than a behavioral refactor.

Recommended direction:
1. Remove `ElementTagSetRef::get()` from `crates/ridi-router-tiles/src/map_data/graph.rs`.
2. Do not replace it unless a real caller appears.
3. Keep using owner-side access through `GenerationGraph.tags` / `ElementTags` for generation and serialization.
4. During implementation, quickly check whether `ElementTagValueRef::get()` should also be removed or at least documented as intentionally non-resolving.

## Open questions
- Is `ElementTagValueRef::get()` in `crates/ridi-router-tiles/src/map_data/graph.rs:41-43` intentionally kept for API parity with another crate, or is it just leftover dead code?
- Does the team want an explicit owner-side accessor in `ElementTags`, or is direct owner iteration/indexing enough for all current generation use cases?
- Should the tiles crate reduce exposure of these generation-only types if they are not meant to be dereferenced at all?

## Suggested implementation starting point
Start in:

- `crates/ridi-router-tiles/src/map_data/graph.rs`

Then verify:

1. remove `ElementTagSetRef::get()`
2. run a narrow search for any attempted call sites
3. run `cargo test -p ridi-router-tiles`
4. check whether any warnings or compile errors reveal hidden dependencies

A practical first search would be narrower than the original todo note:

- `rg -n "ElementTagSetRef::get|\.tags\.get\(|tag_set_idx" crates/ridi-router-tiles/src`

The todo’s suggested `rg "...|\.get\(\)"` is too broad to be very useful now.

## Related files to inspect next
- `crates/ridi-router-tiles/src/map_data/graph.rs`
- `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
- `crates/ridi-router-routing/src/routing_context.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
