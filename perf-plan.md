# `graph::get_adjacent` improvement plan

## Goal
Reduce time, lock contention, and allocation churn in the adjacency lookup path without changing routing behavior.

Hotspot from `perf.md`:

- `graph::get_adjacent`: **18,791,470 calls**, **329.08 s**, **4.6 GB alloc**
- `tile_manager::get_adjacent_by_id`: **18,791,471 calls**, **30.19 s**, **3.3 GB alloc**

## Current shape
Today the hot path looks like this:

1. `walker` calls `RoutingContext::adjacent(...)`
2. `RoutingContext` forwards directly to `MapDataGraph::get_adjacent(...)`
3. `MapDataGraph::get_adjacent(...)` takes a **write lock** on `tile_manager`
4. `TileManager::get_adjacent_by_id(...)` builds a temporary raw tuple vec
5. `MapDataGraph::get_adjacent(...)` maps that into another vec of refs

That gives us three clear options.

---

## Option 1: task-local adjacency cache in `RoutingContext`

### Objective
Stop recomputing adjacency for the same point within one route search.

### Why this is promising
The walker revisits the same points during:

- fork exploration
- backtracking
- roundabout scans
- retry paths

`RoutingContext` already caches:

- point records
- points
- lines
- tag sets

Adjacency fits the same pattern well.

### Important design choice
Do **not** move `LoadedTile.point_index` into `RoutingContext`.

Keep the current split:

- `LoadedTile.point_index` stays in tile manager as shared tile metadata
- `RoutingContext` keeps task-local search caches
- adjacency becomes another task-local cached value

### Implementation sketch
Files:

- `crates/ridi-router-routing/src/routing_context.rs`

Add to `RoutingCaches`:

```rust
adjacent: HashMap<MapDataPointRef, Vec<(MapDataLineRef, MapDataPointRef)>>,
```

Then change `RoutingContext::adjacent(...)` to:

1. look in the cache
2. return cloned cached result on hit
3. fetch from `graph.get_adjacent(...)` on miss
4. store and return

### Scope
- no change to graph or tile-manager API
- no behavior change
- no lock strategy change
- no file-format change

### Expected benefit
- lower `graph::get_adjacent` call count
- lower `tile_manager::get_adjacent_by_id` call count
- lower adjacency recomputation inside walker loops

### Main limitation
The cache is per `RoutingContext`, so it helps within one itinerary/task only.

It does **not** share results across parallel itinerary tasks.

### Risk
Low.

### Validation
Measure before and after:

- `graph::get_adjacent`
- `tile_manager::get_adjacent_by_id`
- walker hotspots that call adjacency repeatedly

---

## Option 2: read-fast-path before taking the tile-manager write lock

### Objective
Avoid exclusive locking when adjacency can be answered from already loaded tiles.

### Why this is promising
`MapDataGraph::get_adjacent(...)` currently does this on every call:

- `self.tile_manager.write().unwrap()`

That is expensive in a hot path, and especially bad with parallel itinerary generation.

### Implementation sketch
Files:

- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

Add a loaded-only method to tile manager, for example:

```rust
pub fn get_adjacent_by_id_if_loaded(
    &self,
    tile_id: TileId,
    osm_id: u64,
) -> Result<Option<Vec<(TileId, usize, TileId, u64)>>>;
```

Behavior:

- return `Ok(None)` if the center tile is not loaded
- return `Ok(None)` if a cross-tile neighbor is needed but not loaded
- return `Ok(Some(...))` if the full answer is available from loaded tiles only

Then change `MapDataGraph::get_adjacent(...)` to:

1. take `tile_manager.read()`
2. try `get_adjacent_by_id_if_loaded(...)`
3. if that succeeds, map and return
4. otherwise fall back to `tile_manager.write()` + `get_adjacent_by_id(...)`

### Scope
- no behavior change
- no route-search API change
- no file-format change
- mostly lock strategy and lookup-path cleanup

### Expected benefit
- less write-lock contention
- better scaling across Rayon tasks
- faster hot-path lookups when tiles are already warm

### Main limitation
This does not reduce repeated same-point lookups by itself.

It makes each call cheaper under warm-cache conditions, but it does not remove the calls.

### Risk
Low to medium.

Main care point:

- the loaded-only path must preserve current cross-tile behavior
- the fallback boundary must be correct and easy to reason about

### Validation
Compare before and after:

- `graph::get_adjacent`
- `tile_manager::get_adjacent_by_id`
- wall time under the same profiled route generation run
- any changes in parallel worker behavior

---

## Option 3: reduce adjacency allocations and double materialization

### Objective
Shrink per-call heap churn in the adjacency path.

### Why this is promising
Current path often allocates twice per lookup:

1. raw tuple vec in `tile_manager::get_adjacent_by_id(...)`
2. mapped ref vec in `graph::get_adjacent(...)`

The profile shows this clearly:

- `graph::get_adjacent`: **4.6 GB alloc**
- `tile_manager::get_adjacent_by_id`: **3.3 GB alloc**

### Implementation sketch
Files:

- `Cargo.toml`
- `crates/ridi-router-routing/Cargo.toml`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

#### Step 3A: add `smallvec`
Workspace dependency:

```toml
smallvec = "1"
```

Crate dependency:

```toml
smallvec.workspace = true
```

#### Step 3B: replace hot small vecs
Use `SmallVec` for adjacency containers, for example:

- raw adjacency temp buffer in tile manager
- final adjacency ref buffer in graph

Road degree is usually small, so many calls should stay inline.

#### Step 3C: collapse one materialization layer if practical
If the first two steps are not enough, consider returning final ref tuples directly from tile manager so graph no longer remaps every entry.

That could look like:

```rust
Result<SmallVec<[(MapDataLineRef, MapDataPointRef); 8]>>
```

or a caller-filled buffer API.

### Scope
- local plumbing cleanup
- no route-behavior change
- no format change

### Expected benefit
- lower alloc churn in both adjacency functions
- less per-call mapping work
- smaller pressure on allocator and profiler-visible inclusive alloc totals

### Main limitation
This does not cut call count by itself.

If repeated same-point lookups are the real main story, option 1 will usually matter more.

### Risk
Low to medium.

Main care points:

- avoid spreading `SmallVec` churn through too many APIs at once
- keep the first change local and measurable

### Validation
Compare before and after:

- inclusive alloc for `graph::get_adjacent`
- inclusive alloc for `tile_manager::get_adjacent_by_id`
- total route-generation time

---

## Recommended order

### Phase A
1. **Option 1**: task-local adjacency cache
2. measure

### Phase B
3. **Option 2**: read-fast-path before write lock
4. measure

### Phase C
5. **Option 3**: `SmallVec` and allocation cleanup
6. measure

Why this order:

- option 1 tests the highest-confidence hypothesis: repeated same-point lookups
- option 2 improves lock shape and parallel behavior
- option 3 trims alloc churn after we know how much call count remains

---

## What I would not do yet
I would **not** start with eager per-tile precompute of adjacency for every point.

Given your concern, that looks too blunt for tiles with tens of thousands of points. If we ever revisit that direction, it should be a **lazy shared memoization** design, not full eager precompute.

---

## Success criteria
A good outcome from options 1-3 would look like:

- `graph::get_adjacent` time materially down
- `tile_manager::get_adjacent_by_id` time materially down
- adjacency alloc totals materially down
- walker hotspots shrink with them
- route generation wall time improves on the same benchmark run

## Decision rule
If the quick and dirty version of option 1 shows a clear win, do the proper version first.

If option 1 barely moves the numbers, option 2 becomes more important, and option 3 becomes the cleanup pass afterward.