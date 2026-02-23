
warning: `ridi-router` (bin "ridi-router") generated 18 warnings
    Finished `dev` profile [optimized + debuginfo] target(s) in 0.20s
     Running `target/debug/ridi-router generate-tiles --input /home/toms/dev/ridi-router/map-data/input --output /home/toms/dev/ridi-router/map-data/output --tile-size-deg 0.1`
2026-02-23T06:22:42.845424Z  INFO main Process{service="ridi-router"}:run: ridi_router::router_runner: src/router_runner.rs:443: Generating tiles from "/home/toms/dev/ridi-router/map-data/input" to "/home/toms/dev/ridi-router/map-data/output" (tile_size_deg=0.1°)
2026-02-23T06:22:42.845454Z  INFO main Process{service="ridi-router"}:run: ridi_router::rmdf::generator: src/rmdf/generator/mod.rs:47: Starting RMDF tile generation
2026-02-23T06:22:42.845462Z  INFO main Process{service="ridi-router"}:run: ridi_router::rmdf::generator: src/rmdf/generator/mod.rs:51: Loading PBF file into memory with pre-computed proximity flags...
2026-02-23T06:22:42.845465Z  INFO main Process{service="ridi-router"}:run: ridi_router::osm_data::in_memory_pbf: src/osm_data/in_memory_pbf.rs:190: Loading PBF file with pre-computed flags: "/home/toms/dev/ridi-router/map-data/input"
2026-02-23T06:22:42.845471Z  INFO main Process{service="ridi-router"}:run: ridi_router::osm_data::in_memory_pbf: src/osm_data/in_memory_pbf.rs:193: Pass 1: Loading nodes...
2026-02-23T06:22:42.845899Z  INFO main Process{service="ridi-router"}:run: ridi_router::osm_data::in_memory_pbf: src/osm_data/in_memory_pbf.rs:195: Loaded 0 nodes in 0.00s
2026-02-23T06:22:42.845911Z  INFO main Process{service="ridi-router"}:run: ridi_router::osm_data::in_memory_pbf: src/osm_data/in_memory_pbf.rs:202: Pass 2: Loading ways...
2026-02-23T06:22:42.846113Z  INFO main Process{service="ridi-router"}:run: ridi_router::osm_data::in_memory_pbf: src/osm_data/in_memory_pbf.rs:206: Loaded 0 ways (0 residential, 0 military) in 0.00s
2026-02-23T06:22:42.846121Z  INFO main Process{service="ridi-router"}:run: ridi_router::osm_data::in_memory_pbf: src/osm_data/in_memory_pbf.rs:215: Pass 3: Loading relations...
2026-02-23T06:22:42.846271Z  INFO main Process{service="ridi-router"}:run: ridi_router::osm_data::in_memory_pbf: src/osm_data/in_memory_pbf.rs:812: Collected 0 relations, computing bounding boxes...
2026-02-23T06:22:42.846285Z  INFO main Process{service="ridi-router"}:run: ridi_router::osm_data::in_memory_pbf: src/osm_data/in_memory_pbf.rs:219: Loaded 0 relations (0 residential, 0 military) in 0.00s
2026-02-23T06:22:42.846291Z  INFO main Process{service="ridi-router"}:run: ridi_router::osm_data::in_memory_pbf: src/osm_data/in_memory_pbf.rs:228: Extracting area polygons...
2026-02-23T06:22:42.846296Z  INFO main Process{service="ridi-router"}:run: ridi_router::osm_data::in_memory_pbf: src/osm_data/in_memory_pbf.rs:244: Extracted 0 residential and 0 military polygons in 0.00s
2026-02-23T06:22:42.846303Z  INFO main Process{service="ridi-router"}:run: ridi_router::osm_data::in_memory_pbf: src/osm_data/in_memory_pbf.rs:252: Computing proximity flags...
2026-02-23T06:22:42.846311Z  INFO main Process{service="ridi-router"}:run: ridi_router::proximity::flag_computer: src/proximity/flag_computer.rs:37: Computing proximity flags for 0 nodes (0 residential, 0 military polygons)

thread 'main' panicked at src/proximity/rasterized_grid.rs:159:38:
PbfBounds should have lat_min
stack backtrace:
   0: __rustc::rust_begin_unwind
             at /rustc/6b00bc3880198600130e1cf62b8f8a93494488cc/library/std/src/panicking.rs:697:5
   1: core::panicking::panic_fmt
             at /rustc/6b00bc3880198600130e1cf62b8f8a93494488cc/library/core/src/panicking.rs:75:14
   2: core::panicking::panic_display
             at /rustc/6b00bc3880198600130e1cf62b8f8a93494488cc/library/core/src/panicking.rs:269:5
   3: core::option::expect_failed
             at /rustc/6b00bc3880198600130e1cf62b8f8a93494488cc/library/core/src/option.rs:2049:5
   4: core::option::Option<T>::expect
             at /home/toms/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/core/src/option.rs:958:21
   5: ridi_router::proximity::rasterized_grid::RasterizedProximityGrid::new
             at ./src/proximity/rasterized_grid.rs:159:23
   6: ridi_router::proximity::area_rasterizer::AreaRasterizer::new
             at ./src/proximity/area_rasterizer.rs:28:19
   7: ridi_router::proximity::flag_computer::compute_proximity_flags
             at ./src/proximity/flag_computer.rs:45:26
   8: ridi_router::osm_data::in_memory_pbf::InMemoryPbf::from_pbf_file_with_flags
             at ./src/osm_data/in_memory_pbf.rs:254:9
   9: ridi_router::rmdf::generator::TileGenerator::generate
             at ./src/rmdf/generator/mod.rs:52:24
  10: ridi_router::router_runner::RouterRunner::run
             at ./src/router_runner.rs:449:21
  11: ridi_router::main
             at ./src/main.rs:53:18
  12: core::ops::function::FnOnce::call_once
             at /home/toms/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/core/src/ops/function.rs:250:5
note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.
  toms on  ~/dev/ridi-router feat-ridi-map-format  1
 #
