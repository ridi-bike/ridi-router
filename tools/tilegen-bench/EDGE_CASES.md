# Planet-scale edge cases — round 2 (prototype C, correctness role)

Goal: use the three independent implementations (A-r1, B-r1, C-r1) plus the legacy generator
(`ref/ridi-router-cli-baseline`) to find bugs in regions Europe does not exercise.

## TL;DR

| # | Finding | Who | Severity | Status |
|---|---------|-----|----------|--------|
| 1 | **A-r1 emits tile column 3600** (lon 180..180.1) for nodes with lon >= 179.995. The reference never creates col 3600 (max col 3599). | A-r1 only | bug (extra tiles, duplicated nodes) | repro below; B, C correct |
| 2 | Global grid ends exactly at +180 / +90: nodes whose **f32 lon rounds to 180.0** (lon >= ~179.9999924) fall outside the proximity grid → residential/nogo flags forced `false`. | all three (consistent) | minor, needs a decision | 8 routable nodes in edge-antimeridian |
| 3 | No antimeridian wraparound: tile buffers, proximity prefilters and haversine search do not wrap. OSM splits ways at ±180 with **distinct** nodes, so no routable segment crosses 180 (0 in data); the graph is disconnected at 180 by data, not by us. | all + legacy | by design (reference) | documented |
| 4 | Legacy generator cannot build inputs with a large lon/lat extent (antimeridian extracts, wide high-latitude extracts): "Grid would require N cells … exceeds 110000000" → PBF skipped / run fails. | legacy only | reference limitation | new impls have no limit; planet must use the global anchor |
| 5 | Residential `could_be_within` prefilter uses a fixed 500 m·1.5/111 km buffer in **degrees of longitude** → north of ~48° polygons between 0.00676°·cos(lat)·111 km and 500 m east/west of a cell are skipped (60°N: >376 m; 78°N: >156 m). | all + legacy | reference quirk (parity) | keep for parity; fix knowingly later |
| 6 | RMDF spatial-index `cell_id` for negative coordinates: two's-complement i16 packed into u32; writer sorts by u32 and reader binary-searches the same key → consistent (negative lats just sort after positive). | all | OK | verified by reading writer + reader |
| 7 | f32 maths near ±180: cell-centre error ≤ 3.2e-5° (~3.6 m); f32 lon spacing near 180 is 1.5e-5°. Nodes within 1e-7° of a 0.0024° cell edge land in a different cell than exact maths in ~50 % of cases — deterministic and identical in all implementations (f32 is the spec). | all | OK | documented |

Everything else: **IDENTICAL** (A vs B vs C on 6 edge datasets; legacy vs C on 5 small fixtures).

## Datasets (map-data/bench/inputs/edge-*.osm.pbf)

Built by `tools/edge/r2-make.sh` from `map-data/input-world` (read-only).

| file | content | size | nodes |
|---|---|---|---|
| edge-antimeridian | fiji, kiribati, tuvalu, tonga, samoa, wallis-et-futuna, american-oceania, us-pacific (Alaska+Aleutians, Hawaii), Chukotka bbox both sides of 180 | 265 MB | 44.9 M |
| edge-nz | new-zealand (incl. Chatham Is. at -176) | 397 MB | 55.3 M |
| edge-chile | chile (S/W hemisphere, Antarctic claim nodes to -63°) | 340 MB | 58.0 M |
| edge-highlat | Norway >= 67°N incl. Svalbard, Nunavut >= 68°N, Greenland | 923 MB | 116.7 M |
| edge-kanto | Japan Kanto bbox (Tokyo metro; very dense) | 443 MB | 55.8 M |
| edge-polygons | israel-and-palestine + north-korea (huge/complex military & residential polygons) | 215 MB | 30.2 M |

## Cross-implementation results (RIDI_GRID_ANCHOR=global, tile 0.1°, 8G cap)

| dataset | non-empty tiles A / B / C | A vs B | A vs C | wall A / B / C (s, noisy) |
|---|---|---|---|---|
| antimeridian | 9100 / 9093 / 9093 | **7 extra in A (col 3600)**, other 9093 identical | same | 38 / 13 / 19 |
| nz | 2848 / 2848 / 2848 | IDENTICAL | IDENTICAL | 9 / 12 / 28 |
| chile | 6303 / 6303 / 6303 | IDENTICAL (15.27 M points) | IDENTICAL | 22 / 22 / 46 |
| highlat | 2504 / 2504 / 2504 | IDENTICAL | IDENTICAL | 12 / 8 / 31 |
| kanto | 295 / 295 / 295 | IDENTICAL | IDENTICAL | 16 / 14 / 26 |
| polygons | 1889 / 1889 / 1889 | IDENTICAL | IDENTICAL | 9 / 11 / 18 |

No crashes. Flags (residential + nogo) are identical everywhere, so all three use the same global
grid definition (origin exactly -180/-90, no buffer).

## Legacy (reference) vs C-r1, reference anchor, small inputs (fixtures/edge)

| input | legacy | result |
|---|---|---|
| samoa (lat<0, lon<0) | ok | IDENTICAL (49 tiles, 61 063 points, 16 rules) |
| tonga (lat<0, lon<0) | ok | IDENTICAL (46 tiles) |
| svalbard (`extract -s simple -b 15,78,16.6,78.4`) | ok | IDENTICAL (34 tiles) |
| fijie (`-s simple -b 179,-17.5,180,-16`, nodes at exactly lon 180.0) | ok | IDENTICAL (75 tiles; legacy has 13 col-3599 tiles, **0 col-3600**) |
| fijiw (`-s simple -b -180,-17.5,-179,-16`, nodes at exactly -180.0) | ok | IDENTICAL (23 tiles, col 0) |
| fiji full; svalbard via complete_ways extract | **fails** (grid too large, finding 4) | n/a |

A-r1 on fijie: 75 identical + **5 extra non-empty tiles tile_3600_730..734** → confirms finding 1 against the reference itself.

## Details

### 1. Tile column 3600 (A-r1)
Reference: `calculate_tiles_from_bounds` clamps `lon_max + 0.01` to 180, then
`col_max = ceil((180 + 180) / 0.1f32 as f64) = ceil(3599.99995) = 3600`, iterated exclusively → max col
**3599**. Tile 3599's buffered bounds are clamped to lon_max = 180.0 (inclusive), so nodes at 180.0
belong to col 3599 only. Same for rows: max row 1799.
A-r1 additionally creates col 3600 for nodes with lon >= 179.995 (its buffered lo = 179.995), so
those nodes appear in two tiles, and 5–7 extra tiles are written per antimeridian dataset.
Repro: `tools/edge/r2-a3600.sh` (fixture `fixtures/edge/fijie.osm.pbf`, golden `fixtures/edge/legacy-fijie`).
Note: A's current worktree `stream/geom.rs` already guards `c < max_col` (3600 exclusive), so the
A-r1 binary either predates that or used another code path. The combined candidate must be checked.
Rule for the final generator: valid cols `0..ceil(360/ts)`, valid rows `0..ceil(180/ts)`, both
exclusive; buffered bounds clamped to ±180/±90 and inclusive.

### 2. Global proximity grid edge (+180 / +90)
Global grid: `lon_min=-180, lat_min=-90`, `cols = ceil((180 - -180)/0.0024f32) = 150000`,
`rows = 75000`, military 300000 x 150000. Cell index `((lon as f32 + 180) / 0.0024) as i32`:
lon 180.0 → 150000 (out of grid); lon 179.9999999 → f32 180.0 → out of grid; lon -180 → col 0.
Out-of-grid lookups return false, exactly as the reference's `get_cell` does. In
edge-antimeridian 8 routable nodes are affected (at +180; the 8 at -180 are fine). lat 90 is
likewise out (no roads). Reference-anchor mode never hits this (bounds + 0.0024 buffer).
Proposal: decide explicitly for planet mode — either accept it (documented, a handful of nodes on
the antimeridian), or define global `cols = ceil(...) + 1` / clamp the index to `cols-1`. Every
implementation must make the same choice; currently all three agree on "false".

### 3. Antimeridian
Reference semantics: no wrap anywhere. Tile col 0 buffered lo = -180, col 3599 hi = 180;
polygons are planar in lon/lat (`geo`); `could_be_within` and the haversine closest point see a polygon
at -179.999 and a cell at 179.999 as 360° apart. OSM splits ways (and areas) at ±180 using two
distinct nodes at -180 and +180 (e.g. n1600491595 (-180, 68.9588655) / n1600484215 (180, 68.9588655),
Chukotka). Measured on edge-antimeridian: 137 342 routable ways, 494 routable nodes with |lon| >= 179.995,
16 exactly at ±180, **0 segments with |Δlon| > 180**. Consequence: routing cannot cross 180 (data +
reference semantics); the reader's grid-ring search (`tile_manager.rs grid_ring_cell_ids`) does not
wrap either. All three implementations and the legacy agree (fijie/fijiw IDENTICAL).

### 4. Legacy grid size limit
`RasterizedProximityGrid::try_new` refuses > 110 M cells, so any input spanning ~±180 or a wide
high-latitude extent fails (fiji: 903 M cells; Svalbard via a complete_ways extract that drags in far
relation members: 201 M). The new implementations evaluate cells sparsely and have no such limit.
References for such regions can only be built from tight `-s simple` extracts (as done for the fixtures).

### 5. High-latitude prefilter quirk
`BBox::could_be_within(lat, lon, 500)` expands lon by 500/111000*1.5 = 0.00676° regardless of latitude.
East/west coverage = 0.00676 · cos(lat) · 111 km: 750 m at the equator, 500 m at 48.2°, 376 m at 60°,
156 m at 78° (Svalbard). Polygons whose bbox lies further east/west than that but within 500 m are not
counted. This is the reference behaviour (all implementations and the legacy agree; Latvia/Lithuania
parity depends on it). Track it as a known quirk; changing it changes flags everywhere north of ~48°.

### 6. RMDF cell_id with negative coordinates
`GridCellEntry::encode_cell_id`: `((round(lat*100) as i16 as u32) << 16) | (round(lon*100) as i16 as u32 & 0xFFFF)`.
The range ±9000/±18000 fits i16. The writer orders cells with a `BTreeMap<u32,…>`; the reader
(`tile_manager.rs find_grid_cell_entry`) binary-searches by the same u32, so negative values are consistent.
lon 179.995 → 18000 and -179.995 → -18000 are distinct cells (no wrap; consistent with 3).

### 7. f32 precision
Global grid near ±180: f32 lon spacing 1.5e-5° (1.7 m), cell-centre error ≤ 3.2e-5° (3.6 m),
column index < 2^24 everywhere (max 300000), so there is no integer precision loss. Nodes exactly on (or
within 1e-7° of) a 0.0024° cell edge: 2359/4641 sampled edge positions go to the neighbouring cell
under the f32 formula compared with exact maths. This is the specified behaviour, and all implementations
match bit-for-bit (identical flags on 30+ M points). Tile membership compares the f64 node
coordinate with f32-derived bounds widened to f64. A decimicro coordinate essentially never equals
such a bound, except the clamped ±180/±90 bounds, which are inclusive (covered by fijie/fijiw).

## Proposed permanent regression set (map-data/bench/fixtures/edge, 42 MB)

Golden-output tests (rmdf-diff must be IDENTICAL, reference anchor):
1. `fijie.osm.pbf` vs `legacy-fijie/` — nodes at exactly +180; asserts max col 3599, no col 3600.
2. `fijiw.osm.pbf` vs `legacy-fijiw/` — nodes at exactly -180, col 0, negative lon.
3. `samoa.osm.pbf` vs `legacy-samoa/` — S/W hemisphere incl. restrictions (16 rules).
4. `tonga.osm.pbf` vs `legacy-tonga/`.
5. `svalbard.osm.pbf` vs `legacy-svalbard/` — 78°N (high-latitude prefilter quirk).

Property tests (global anchor, any fixture, no golden needed):
6. Every tile file name has `col < ceil(360/ts)` and `row < ceil(180/ts)`.
7. Every point in tile (c,r) lies inside that tile's buffered, ±180/±90-clamped bounds.
8. Spatial-index entries are strictly increasing by u32 cell_id, and every point's `encode_cell_id(lat,lon,100)`
   equals its cell entry (covers negative coordinates).
9. A node with f32 lon == 180.0 has flags == 0 (or whatever finding 2 decides) — pins the global-grid edge rule.
10. No line record with |lon_a - lon_b| > 180.

Synthetic unit fixtures (tiny hand-written .osm → .pbf, for the generator crate's tests):
11. Multipolygon with member roles `via`/`from`/`""` treated as outer, unclosed chain dropped, inner
    ring matched by first-coordinate containment.
12. Area relation with a relation member (resolvable / unresolvable), 1 and 2 nesting levels.
13. Restriction whose via node is only in a tile's 0.005° buffer (rule materialised in each tile iff
    from/to lines are local there).
14. Node placed on a residential cell edge (decimicro offset from -180 that is a multiple of 24000): the flag
    uses the f32 cell, not the exact one.

## Reproduce
- `tools/edge/r2-make.sh` (datasets), `tools/edge/r2-cross.sh <dataset>` (A/B/C global + cross-diff),
  `tools/edge/r2-legacy.sh` (legacy vs C on tight extracts), `tools/edge/r2-a3600.sh` (finding 1),
  `tools/edge/r2-am.py` (antimeridian statistics), `tools/edge/r2-f32.py` (f32 grid analysis),
  `tools/edge/r2-fixtures.sh` (fixtures + goldens).
- Cross-run outputs (13 GB): `tmp/C-dev/edge-out/<bin>-<dataset>/` — safe to delete.
