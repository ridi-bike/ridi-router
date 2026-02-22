I get a warning when I compile.
please investigate and tell me where this may have been used in the past and why it might not be used anymore. look at git history

```
warning: unused import: `area_rasterizer::AreaRasterizer`
  --> src/proximity/mod.rs:16:9
   |
16 | pub use area_rasterizer::AreaRasterizer;
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: `#[warn(unused_imports)]` on by default

warning: unused imports: `GridCell` and `bearing_to_sector`
  --> src/proximity/mod.rs:18:27
   |
18 | pub use rasterized_grid::{bearing_to_sector, GridCell, RasterizedProximityGrid, GRID_CELL_SIZE_DEG};
   |                           ^^^^^^^^^^^^^^^^^  ^^^^^^^^

warning: unused import: `File`
 --> src/rmdf/generator/writer.rs:5:15
  |
5 | use std::fs::{File, OpenOptions};
  |               ^^^^

warning: unused import: `TileManifest`
 --> src/rmdf/generator/mod.rs:6:39
  |
6 | pub use manifest::{ManifestGenerator, TileManifest};
  |                                       ^^^^^^^^^^^^

warning: unused import: `writer::RmdfWriter`
 --> src/rmdf/generator/mod.rs:8:9
  |
8 | pub use writer::RmdfWriter;
  |         ^^^^^^^^^^^^^^^^^^

warning: unused import: `generator::*`
  --> src/rmdf/mod.rs:16:9
   |
16 | pub use generator::*;
   |         ^^^^^^^^^^^^

warning: unused import: `io::*`
  --> src/rmdf/mod.rs:17:9
   |
17 | pub use io::*;
   |         ^^^^^

warning: unused import: `validation::*`
  --> src/rmdf/mod.rs:19:9
   |
19 | pub use validation::*;
   |         ^^^^^^^^^^^^^

warning: unused import: `std::panic::catch_unwind`
 --> src/router_runner.rs:2:5
  |
2 | use std::panic::catch_unwind;
  |     ^^^^^^^^^^^^^^^^^^^^^^^^

warning: unused import: `time::Instant`
 --> src/router_runner.rs:3:62
  |
3 | use std::{num::ParseFloatError, path::PathBuf, str::FromStr, time::Instant};
  |                                                              ^^^^^^^^^^^^^

warning: unused import: `IpcHandler`
  --> src/router_runner.rs:11:19
   |
11 |     ipc_handler::{IpcHandler, IpcHandlerError, ResponseMessage, RouteMessage, RouterResult},
   |                   ^^^^^^^^^^

warning: unused imports: `haversine_batch`, `haversine_f32x8`, and `min_distance_to_vertices`
  --> src/simd/mod.rs:12:21
   |
12 | pub use haversine::{haversine_batch, haversine_f32x8, min_distance_to_vertices};
   |                     ^^^^^^^^^^^^^^^  ^^^^^^^^^^^^^^^  ^^^^^^^^^^^^^^^^^^^^^^^^

warning: unused imports: `asin_f32x8`, `cos_f32x8`, and `sin_f32x8`
  --> src/simd/mod.rs:13:23
   |
13 | pub use trig_approx::{asin_f32x8, cos_f32x8, sin_f32x8};
   |                       ^^^^^^^^^^  ^^^^^^^^^  ^^^^^^^^^
```
