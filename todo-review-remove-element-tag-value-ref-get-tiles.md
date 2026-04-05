# Review: `ElementTagValueRef::get()` in `crates/ridi-router-tiles`

## Verdict: partially relevant

## Summary
The todo is **not fully obsolete**, because `ElementTagValueRef::get()` still exists in `crates/ridi-router-tiles/src/map_data/graph.rs` and still returns placeholder data (`None`). That means the misleading API surface is still present.

However, the todo is also **partly stale**:
- there do not appear to be any current callers in `ridi-router-tiles` that depend on `ElementTagValueRef::get()`
- actual tag lookup behavior has already been moved to the routing side, where tag refs are resolved through `MapDataGraph` / `RoutingContext` instead of through the ref itself
- `cargo test -p ridi-router-tiles` already reports `ElementTagValueRef::get()` as unused dead code

So the removal still makes sense, but the implementation is likely smaller than the todo text suggests.

## Current evidence from the codebase

### 1. The misleading helper still exists in the tiles crate
In `crates/ridi-router-tiles/src/map_data/graph.rs`:
- `ElementTagValueRef` is a small struct with `tile_id` and `tag_value_idx`
- `ElementTagValueRef::get(&self) -> Option<String>` still exists
- its implementation is just `None`

That confirms the todo's core observation: the method looks like a dereference API, but does not perform real lookup.

### 2. Related ambient helpers also still exist in the same file
The same tiles file also still contains:
- `ElementTagSetRef::get()` returning an `ElementTagSet` filled with `ElementTagValueRef::none(...)`
- `ElementTagSet::name()`, `hw_ref()`, `highway()`, `surface()`, and `smoothness()`, each delegating to `.get()` on the underlying `ElementTagValueRef`

Those methods have the same shape as a real lookup API, but in the tiles crate they do not resolve data from any backing store.

### 3. The tiles generation path already treats tag refs as identifiers, not lookup handles
The generation flow in `crates/ridi-router-tiles/src/map_data/generation_graph.rs` builds tag refs with `self.tags.get_or_create(...)` and stores them on generated lines.

When tiles are serialized in `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`, the code writes:
- raw tag value strings from `graph.get_tags().tag_values`
- raw tag-set records from `graph.get_tags().tag_sets`
- tag indices from `tag_set.name.tag_value_idx`, `tag_set.hw_ref.tag_value_idx`, etc.

That path uses the refs as **indexes for serialization**, not as ambient lookup objects.

### 4. Real lookup behavior exists elsewhere now
In `crates/ridi-router-routing/src/map_data/graph.rs`:
- `ElementTagValueRef` exists without a `.get()` method
- `MapDataGraph::get_tag_value(&ElementTagValueRef)` performs real lookup through `TileManager`
- `MapDataGraph::get_tag_set(&ElementTagSetRef)` reconstructs a tag set from tile-backed records

In `crates/ridi-router-routing/src/routing_context.rs`:
- `RoutingContext::tag_value(...)` delegates to `MapDataGraph::get_tag_value(...)`
- `RoutingContext::tag_set(...)` delegates to `MapDataGraph::get_tag_set(...)`

This is strong evidence that a later refactor already established the intended replacement pattern: **lookup belongs to graph/context code, not to the ref type itself**.

### 5. There are no current tiles callers to update
A repository search for `ElementTagValueRef::get(` and related tag-set getter usage did not find active callers in `crates/ridi-router-tiles` or `crates/ridi-router-routing`.

Also, `cargo test -p ridi-router-tiles` currently emits dead-code warnings for:
- `ElementTagValueRef::get`
- `ElementTagSetRef::get`
- `ElementTagSet::{name, hw_ref, highway, surface, smoothness}`

That matters because the original todo says to "update any callers", but current evidence suggests there may be **no callers left**.

## If still relevant

### The actual problem
The tiles crate still exposes a misleading internal API in `crates/ridi-router-tiles/src/map_data/graph.rs`:
- `ElementTagValueRef` looks dereferenceable, but is only an identifier
- `.get()` suggests real data access while returning placeholder data
- companion methods on `ElementTagSetRef` and `ElementTagSet` reinforce the same false model

### What behavior is missing or risky
The immediate runtime risk looks low because the methods appear unused today.

The real risk is maintenance risk:
- future code in the tiles crate could accidentally call `.get()` and silently receive `None`
- the tiles crate's local API shape no longer matches the routing crate's newer design
- duplicated but inconsistent graph/tag APIs across `ridi-router-tiles` and `ridi-router-routing` make future refactors harder

### Possible solution options

#### Option A: Minimal cleanup
Remove only:
- `ElementTagValueRef::get()`

Pros:
- smallest change
- directly satisfies the literal todo title

Cons:
- leaves `ElementTagSetRef::get()` and `ElementTagSet` accessors with the same misleading pattern
- only partially cleans up the stale API

#### Option B: Remove the whole ambient tag-access surface in the tiles crate
Remove:
- `ElementTagValueRef::get()`
- `ElementTagSetRef::get()`
- `ElementTagSet::{name, hw_ref, highway, surface, smoothness}`

Keep:
- `ElementTagValueRef` as a plain identifier
- `ElementTagSetRef` / `ElementTagSet` as serialization-oriented data structures

Pros:
- aligns the tiles crate with how the code already behaves
- removes the full misleading API family, not just one method
- matches the routing-side design more closely

Cons:
- slightly broader cleanup than the original todo text
- may require checking tests or internal helper code for any latent use sites

#### Option C: Keep the methods but make misuse explicit
Example directions:
- panic in `get()`
- deprecate methods with strong comments

Pros:
- catches accidental use faster than returning `None`

Cons:
- keeps the wrong API around
- less clean than deleting dead helpers outright
- not consistent with the routing-side design that already moved lookup elsewhere

### Tradeoffs / implications
- The original todo overstates the amount of caller migration likely needed.
- The main remaining work is API cleanup, not behavior migration.
- If this cleanup is done, it should probably remove the adjacent misleading helpers too, otherwise the crate will still imply ambient lookup through tag refs.
- Because `ridi-router-tiles` does not expose `map_data` publicly from `src/lib.rs`, this looks like an internal cleanup with limited external API fallout.

### Recommended direction
Treat the todo as **still relevant, but narrower than written**.

Recommended implementation direction:
1. remove `ElementTagValueRef::get()` from `crates/ridi-router-tiles/src/map_data/graph.rs`
2. in the same pass, strongly consider removing `ElementTagSetRef::get()` and the `ElementTagSet` string accessors from that same file
3. verify that the tiles generation path continues to use tag refs only as indices for serialization
4. run `cargo test -p ridi-router-tiles`

That would align the tiles crate with the already-established routing-side pattern.

### Open questions
- Should the cleanup remove only `ElementTagValueRef::get()`, or the whole misleading ambient tag accessor family in the tiles crate?
- Is there any reason to keep the tiles and routing `map_data/graph.rs` files structurally similar, even when the routing crate owns real lookup behavior?
- Are there any out-of-tree users or local branches relying on these internal methods despite the current workspace showing no callers?

### Suggested implementation starting point
Start in:
- `crates/ridi-router-tiles/src/map_data/graph.rs`

First inspect/remove:
- `ElementTagValueRef::get()`
- `ElementTagSetRef::get()`
- `ElementTagSet::{name, hw_ref, highway, surface, smoothness}`

Then verify no remaining usage with searches similar to:
- `rg "ElementTagValueRef::get|ElementTagSetRef::get|\.name\(\)|\.hw_ref\(\)|\.highway\(\)|\.surface\(\)|\.smoothness\(\)" crates/ridi-router-tiles crates/ridi-router-routing`

Finally validate with:
- `cargo test -p ridi-router-tiles`

## Related files to inspect next
- `crates/ridi-router-tiles/src/map_data/graph.rs`
- `crates/ridi-router-tiles/src/map_data/generation_graph.rs`
- `crates/ridi-router-tiles/src/rmdf/generator/writer.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/routing_context.rs`
