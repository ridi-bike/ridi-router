# Local stage — shared module spec (owner: prototype A)

The "local stage" is everything that happens once one region's data is in memory:
tile assignment → proximity flags → per-tile graph build → RMDF serialization → manifest entries.
It is identical for all three prototypes; only how the region data is obtained differs.

Code (prototype A worktree `/home/toms/dev/ridi-router/.claude/worktrees/agent-af6fb7daa2a23ddfa`):

| file | content |
|---|---|
| `crates/ridi-router-tiles/src/local_stage/mod.rs` | input types, `run()`, tile assignment, restriction index, parallel tile loop |
| `crates/ridi-router-tiles/src/local_stage/proximity.rs` | `GridAnchor`, sparse exact residential/military flags |
| `crates/ridi-router-tiles/src/local_stage/tile.rs` | index-based tile graph + byte-identical RMDF writer (in-memory SHA-256) |
| `crates/ridi-router-tiles/src/local_stage/manifest.rs` | manifest from in-memory entries, manifest parts + merge |
| `crates/ridi-router-tiles/src/generation/graph.rs` | `materialize_restriction` + `RestrictionContext` (restriction semantics shared with `GenerationGraph`) |
| `crates/ridi-router-tiles/src/proximity/area_rasterizer.rs` | reference per-polygon maths, now `pub(crate)` (`polygon_distance_m`, `calculate_bearing`, bbox/centroid) |

To port: copy `local_stage/` and the small changes in `generation/graph.rs`, `generation/mod.rs`,
`proximity/area_rasterizer.rs` (all are additive refactors; `GenerationGraph` behaviour is unchanged),
add `pub mod local_stage;` to `lib.rs`. Dependencies already in the crate: rayon, rstar, geo, sha2, hex,
bytemuck, chrono, serde_json.

## Interface

```rust
use ridi_router_tiles::local_stage::{self, GridAnchor, LocalStageInput, NodeTable, WayTable, WayTags};

let input = LocalStageInput {
    output_dir: &out_dir,              // tiles are written here as tile_<col>_<row>.rmdf
    tile_size_degrees: 0.1,
    anchor: GridAnchor::reference(lat_min, lat_max, lon_min, lon_max) /* or */ GridAnchor::global(),
    core_bbox: None,                   // Some([lon_min, lat_min, lon_max, lat_max]): write only tiles
                                       // whose centre is in this half-open box (regional runs)
    nodes: NodeTable { ids: &ids, lat_e7: &lat, lon_e7: &lon },
    ways: WayTable { ids: &way_ids, offsets: &offsets, refs: &refs, tag_set: &tag_set, tag_sets: &tag_sets },
    restrictions: &restrictions,       // Vec<OsmRelation>
    residential_polygons: &res,        // Vec<geo::MultiPolygon<f64>>
    military_polygons: &mil,
};
let out = local_stage::run(&input, &mut |phase_name| log_phase(phase_name))?;
local_stage::manifest::write_manifest(&out_dir, 0.1, out.tiles, "source.osm.pbf")?;
// regional runs: write_manifest_part(&out_dir, "r007", 0.1, out.tiles, src) and later
// merge_manifest_parts(&out_dir)  (CLI: `generate-tiles -o DIR --merge-manifests`)
```

`run` calls `phase(name)` after `tile_assignment`, `proximity` and `write_tiles` (for the uniform
`PHASE` log lines). It returns one `TileMetadata` per written tile (neighbours are filled by the
manifest functions).

### Input contract (all of it matters for parity)

**`NodeTable`** — every node referenced by a routable way (a superset is fine).
- `ids`: ascending, unique OSM node ids (`u64`, i.e. `osm_id as u64`).
- `lat_e7`, `lon_e7`: decimicro degrees exactly as in the PBF (`osmpbf` `decimicro_lat()`,
  `osmpbfreader` `decimicro_lat`). The stage uses `lat = lat_e7 as f64 * 1e-7` (bit-identical to
  osmpbfreader's `lat()`); graph points use that value `as f32`.
- `lat_e7 == local_stage::MISSING_COORD` (`i32::MIN`) = node referenced but absent from the input
  (such refs behave as in the reference: the node is in no tile, lines touching it are dropped).

**`WayTable`** — routable ways only: `highway` ∈ the 17 `ALLOWED_HIGHWAY_VALUES`
(motorway … road, see `lean/pbf.rs`) or `highway=path` + `motorcycle=yes`.
- `ids` ascending (the reference fed ways in id order).
- `refs[offsets[i]..offsets[i+1]]`: the way's node refs in order, as indices into `NodeTable`
  (`u32::MAX` if the id is not in the table). Keep every ref, including missing ones and repeats.
- `tag_set[i]` indexes `tag_sets: &[WayTags]` (interning is optional; one entry per way is fine).
  `WayTags` holds the raw values of `name`, `ref`, `highway`, `surface`, `smoothness`, `oneway`,
  `junction` (`None` if the key is absent; duplicate keys: last wins). Nothing else is read.

**`restrictions`** — `OsmRelation`s whose `type` tag starts with `"restriction"`, sorted by id,
with full tags and members (`role` mapped: "from"/"to"/"via" → `From/To/Via`, anything else
`Other(role)`). Only relations the reference loader keeps: the relation must be *resolvable*
(all nested relation members are loaded relations — multipolygon / `type=restriction` /
`landuse=residential|military` / any `military=*` — resolvable within the reference's 10-iteration
loop; see `resolve_relations` in `lean/mod.rs`). In practice: drop restrictions with
unresolvable relation members. A relation is offered to a tile iff one of its way members is a
routable way of that tile (all other relations would be skipped by the graph anyway).

**Polygons** — exactly the reference `extract_area_polygons` output and order
(see `lean/polygons.rs`): first closed tagged ways in **file order**
(`landuse=residential`, resp. `landuse=military` or any `military=*`; closed = first ref == last ref,
≥4 resolved coords), then resolved area relations ordered by resolution iteration
(multipolygon: ring assembly via `connect_ways_into_rings`, inner rings by first-coord containment;
roles outer/""/from/to/via = outer, inner = inner; other relations: each closed member way with
≥4 refs). Missing nodes are skipped when building coords. Order only matters for f32 summation
(cells exactly at the threshold); the reference's own relation order is HashMap-random.

**Grid anchor** — `GridAnchor::reference(min/max lat/lon over ALL nodes of the input as f64)`
reproduces the reference grid (origin = bounds `as f32` − 0.0024, with the reference grid
extent). `GridAnchor::global()` = origin (−180, −90), no extent, identical for every region —
use it for split/planet runs.

### What the stage does (and guarantees)

1. **Tile assignment** — a node belongs to every tile whose bounds (f32 maths as the reference)
   buffered by 0.005° contain it (inclusive, compared in f64); a way belongs to every tile that
   contains one of its nodes. Tiles without routable ways are not written (allowed; rmdf-diff
   ignores empty tiles).
2. **Proximity** — only cells containing routable nodes are evaluated (0.0024° residential,
   0.0012° military), with the reference's f32 cell centres, f32 prefilter boxes, polygon
   evaluation order and f32 sums. Exact shortcuts:
   - candidates from an R-tree over the exact prefilter boxes, one query per run of up to 64
     neighbouring cells in a row, then the exact prefilter per cell;
   - "polygon within 500 m": great-circle lower bound on the polygon bbox (reject), then per
     segment: skip segments whose bbox is provably > 500 m + margin away, stop at the first segment
     whose reference closest point (`Line::haversine_closest_point` + `Haversine.distance`) is
     ≤ 500 m; otherwise planar `contains`. Margin = 25 m + great-circle bulge of the longest segment.
     Degenerate cases (multi-part polygons, <3 exterior coords, all segment results NaN, nothing
     computed) fall back to the reference `polygon_distance_m`;
   - the residential sum stops as soon as it exceeds the threshold (all terms ≥ 0 and f32
     rounding is monotone, so the final sum cannot fall back below it).
   - military: candidates per row run, then the reference `is_military_interior` for the 5 points.
3. **Tile graph + RMDF** — per tile, index based: points = the tile's routable nodes in id order,
   lines emitted per way in id order when both consecutive refs are tile points, tag sets interned
   with the reference `GenerationTags`, restrictions materialized with the reference semantics
   (`materialize_restriction`), points ordered by (`encode_cell_id`, id). Output is
   **byte-identical** to `GenerationGraph` + `RmdfWriter` (verified with `cmp` on all Latvia and
   Lithuania tiles). The file is hashed in memory and written once. Restriction skips are counted
   and logged once per run (summed), not per relation per tile.
4. **Manifest** — `TileMetadata` from in-memory results (size, counts, checksum, bounds), sorted by
   (col,row), neighbours with the reference rule (note: the reference only fills neighbours for
   col ≤ 359 / row ≤ 179, i.e. never for 0.1° tiles — kept for parity).

Parallelism: rayon everywhere (proximity per cell run, tiles largest first). Memory owned by the
stage beyond the inputs: (tile,node) pairs 8 B × ~1.2 × routable nodes, (tile,way) pairs,
1 B flag per node-table entry, cell key vectors, per-thread tile buffers.

### Measured (prototype A, dev machine under load 8-19, so indicative only)

| phase (wall s) | Lithuania r0 | Lithuania r1 | Poland r0 (official) | Poland r1 (dev, load ~12) |
|---|---|---|---|---|
| tile_assignment | 0.19 | 0.13 | 1.4 | 1.3 |
| proximity | 3.07 | 0.58 | 24.9 | 5.6 |
| write_tiles | 1.16 | 0.75-1.5 | 11.5 | 8.3 |
| manifest | 1.35 | 0.002-0.06 | 1.8 | 0.01 |

All outputs byte-identical (`cmp`) to the r0 / reference-path writer on Latvia and Lithuania (both
anchors) and rmdf-diff IDENTICAL on Poland vs A-r0 (which matches B-r0/C-r0).

## Round 2 changes (re-sync for vendored copies)

Interface (`LocalStageInput`, `run`, `manifest::*`) is **unchanged**. Re-copy these files:

| file | change |
|---|---|
| `local_stage/mod.rs` | **bug fix**: `tiles_for_point` limits cols to `0..ceil(360/ts)` and rows to `0..ceil(180/ts)` (exclusive), like the reference's `calculate_tiles_from_bounds` (no `tile_3600_*` for lon >= 179.995); file writes moved to dedicated writer threads (`RIDI_WRITER_THREADS`, default 4, bounded channel of 2x rayon threads); cell-key vectors collected without rayon's 2x concat peak; tile size summary log line |
| `local_stage/tile.rs` | `build_and_write_tile` -> `build_tile` (returns bytes + manifest entry, no IO); hotpath spans (`tile.graph/restrictions/layout/serialize/sha256`) behind the crate's `hotpath` feature |
| `local_stage/proximity.rs` | ring chunk bboxes (16 segments) for segment skipping; planar point-segment lower bound per segment; exact replica of geo's winding-number `contains` that only visits chunks straddling the point latitude; "nothing finite but segments skipped" resolved by finding one finite segment instead of the full reference fallback; antimeridian guard for longitude bounds; lazy polygon preparation and lazy geodesic area; per-thread stats in the `Residential:` log line; `RIDI_VERIFY_PROXIMITY=1` checks every (cell, polygon) shortcut against the reference maths and the early exit against the full sum (panics on mismatch) |
| `local_stage/proximity/tests.rs` | new: randomized exactness test (city blocks, 4000-vertex polygons, 3 deg segments, 70-84 deg latitude, holes) |
| `local_stage/manifest.rs` | unchanged |

Crate requirements: `hotpath` dependency and a `hotpath` feature in the vendoring crate (the
`span!` / `measure_block!` macros compile to nothing without the feature); optional
`sha2 = { features = ["asm"] }`.

Exactness evidence: byte-identical tiles on Latvia/Lithuania (vs A-r1), rmdf-diff IDENTICAL on
Poland vs `ref/poland-0.1` and on the edge fixtures (fijie, fijiw, samoa, tonga, svalbard) vs the
legacy goldens; `RIDI_VERIFY_PROXIMITY=1` on Poland checked all 19.0M (cell, candidate polygon)
pairs: 0 mismatches, 0 reference fallbacks.
