# Tile generation redesign — prototype brief (shared by prototypes A, B, C)

## Goal
The current RMDF routing-tile generator (`crates/ridi-router-tiles`) needs ~64x the PBF size in RAM
(Latvia 138 MB PBF -> 8.8 GB peak, 3 min; Lithuania 221 MB -> 11 GB, 10 min; Poland 2.1 GB would need
~130 GB and swaps for days). Target: generate tiles for the **whole planet (~85 GB PBF, ~9B nodes,
~1B ways)** on a 12-core machine with **24 GB RAM + swap**, as fast as possible, producing output
**semantically identical** to the current generator.

Three prototypes explore different architectures. Each is built in its own git worktree. You own one.
Read the current code first: `crates/ridi-router-tiles/src/` (esp. `osm_data/in_memory_pbf.rs`,
`proximity/*`, `rmdf/generator/mod.rs` (MultiPbfGenerator::extract_tile_data_for_redb,
build_generation_graph_from_tile, write_rmdf_tile), `generation/*`, `rmdf/generator/writer.rs`).

## The spec is the current behaviour
The reference outputs in `map-data/bench/ref/<dataset>-0.1/` were produced by the current code
(`--input-dir` mode, one PBF, `--tile-size-deg 0.1`). Your output must match them per `rmdf-diff`.
Nuances you must reproduce (read the code, it is the spec):
- Routable ways: `highway` in the 17 ALLOWED_HIGHWAY_VALUES, or `highway=path` + `motorcycle=yes`.
- Tile membership: node belongs to every tile whose bounds buffered by 0.005 deg contain it; a way
  goes into a tile if any of its nodes is in that tile's node set; graph lines are emitted only when
  both endpoints are present; restriction relations (`type` starts with "restriction") go into a tile
  if any member is in the tile (node/way), or has a relation member. Ways, relations, nodes are fed to
  `GenerationGraph` sorted by OSM id (that's what the redb key order produced). Only routable nodes
  end up in output (writer drops unconnected points), so you don't need non-routable nodes.
- Residential polygons: closed ways with `landuse=residential`; relations with `landuse=residential`
  (multipolygon -> ring assembly via `connect_ways_into_rings`, inner rings matched by first-coord
  containment; non-multipolygon -> each closed member way). Military: `landuse=military` or any
  `military=*` key. Relations whose nested relation members can't be resolved are dropped (see
  `compute_relation_bboxes_iterative`). Missing nodes are skipped when building coords.
- Residential proximity per 0.0024 deg cell, evaluated at the cell centre (f32 maths as in
  `rasterized_grid.rs`): sum of WHOLE polygon geodesic areas of polygons within 500 m
  (haversine_closest_point / Haversine distance, `could_be_within` prefilter with 1.5x margin),
  bucketed in 8 sectors by bearing to the vertex-average centroid; flag = sum > THRESHOLD_AREA_M2.
  (The sectors only exist to merge grids across PBFs; with a single input only the sum matters.)
- Military nogo per 0.0012 deg cell: Full iff all 5 sample points (centre + 4 corners) are inside
  some military polygon (`contains`). Only Full counts.
- A node's flags = the values of the cell containing it.
- **Grid anchor**: the reference grid origin is (min lon/lat over ALL nodes in the PBF, cast to f32)
  minus 0.0024. Cell centres depend on it, so for parity you must reproduce it. Support env var
  `RIDI_GRID_ANCHOR=reference` (default; compute bounds over all nodes — a cheap streaming min/max)
  and `RIDI_GRID_ANCHOR=global` (origin -180/-90, what a planet run would use). Flags may differ
  between the two modes; that is expected.
- f32 summation order of polygon areas can flip cells sitting exactly at the threshold, and the
  reference's own relation-polygon order is HashMap-random. Aim for zero flag mismatches; if a few
  remain, prove they're threshold-borderline (report the cell sums), don't hand-wave.
- Keep `manifest.json` (use the existing `ManifestGenerator`). Skipping empty tiles (no routable
  points) is allowed; rmdf-diff ignores empty tiles.

## Interface
In your worktree, `target/release/ridi-router-cli generate-tiles --input <pbf> -o <dir> --tile-size-deg 0.1`
must run YOUR pipeline (single-file mode; you may leave `--input-dir` as is). Scratch/temporary data
goes under `$TMPDIR` (the bench script sets it to a disk dir). **Never put bulk data in /tmp** — it is a
12 GB RAM tmpfs. Build the final binary with the repo's normal `cargo build --release -p ridi-router-cli`
(for quick dev iterations you may use `CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`).

Log uniform phase lines so results are comparable:
`PHASE name=<phase> wall_s=<f> rss_mb=<current RSS> hwm_mb=<VmHWM>` at the end of each phase (via
tracing info!). Also log a final `PHASE name=total ...`.

## Tools
- `map-data/bench/tools/rmdf-diff/target/release/rmdf-diff <ref_dir> <cand_dir> [--json f] [--max-examples N]`
  — semantic diff, exit 0 iff identical (ignoring empty tiles).
- `map-data/bench/run-bench.sh <label> <binary> <dataset> [ENV=val ...]` — capped (MemoryMax=20G),
  measured run + diff vs reference, appends to `map-data/bench/results.jsonl`.
- Profiling: the repo uses the `hotpath` crate (0.14.1; see `crates/ridi-router-cli/Cargo.toml` features
  `hotpath`/`hotpath-alloc`, `#[hotpath::main]` in cli `main.rs`, `#[hotpath::measure_all]` /
  `#[hotpath::measure]` / `hotpath::measure_block!` in `crates/ridi-router-routing`). Add the same
  `hotpath`/`hotpath-alloc` features to `ridi-router-tiles` (forwarded from the cli features) and
  instrument your pipeline's phases and hot functions. Profile with
  `cargo build --release -p ridi-router-cli --features hotpath` (or `hotpath-alloc` for allocation
  counts/bytes). Benchmark binaries in `map-data/bench/bins/` must be built WITHOUT these features.
  `osmium` 1.19 is installed (`perf`/`heaptrack` may not be available yet).
- Datasets: `map-data/bench/inputs/{latvia,lithuania,slovakia,poland}.osm.pbf`. References exist
  for latvia and lithuania. Poland (2.1 GB) has no reference (too slow for the old code); prototypes
  are cross-checked against each other there.

## Shared-machine etiquette (3 prototypes are developed in parallel on one box)
- Dev runs: latvia/lithuania only, each wrapped in
  `systemd-run --user --scope -p MemoryMax=6G -p MemorySwapMax=2G <cmd>` so you can't starve the others.
  If your prototype can't do Lithuania in 6 GB, that's a finding — report it.
- Don't run poland or slovakia; the orchestrator runs official benchmarks serially on a quiet machine.
  Your own timings are noisy (other agents are compiling) — use them for relative comparisons only.
- Don't commit. Don't touch other worktrees or `map-data/bench/ref`, `results.jsonl`.
- When you finish a round, copy your release binary to `map-data/bench/bins/<proto>-r<round>`
  (e.g. `A-r0`) and report its path.

## Report at the end of each round (concise)
1. Architecture as built (data flow, passes, on-disk/in-memory structures, parallelism).
2. Parity: rmdf-diff summary on latvia and lithuania (both anchors: reference must be identical;
   report global-mode flag deltas vs reference for information).
3. Your phase timings + peak RSS on latvia & lithuania (noisy).
4. A planet scaling model: expected peak memory and wall time for ~85 GB planet on 24 GB/12 cores,
   with the reasoning and which structure dominates memory. Be honest about what's unknown.
5. Bottlenecks you see and what you'd do next round, ranked by expected gain.
