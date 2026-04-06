# ridi-router

`ridi-router` is now a Cargo workspace with reusable libraries and a thin CLI.

## Workspace layout

- `crates/ridi-router-cli` - CLI adapter, file IO policy, JSON/GPX output, human-readable errors
- `crates/ridi-router-routing` - routing library over generated RMDF tiles
- `crates/ridi-router-tiles` - tile-generation library from OSM PBF input
- `crates/ridi-router-common` - small shared RMDF data types

## Binary name

The CLI package is `ridi-router-cli` and the produced binary is also `ridi-router-cli`.

## Build

```bash
cargo build --workspace
```

Run the CLI from the workspace root:

```bash
cargo run -p ridi-router-cli -- --help
```

Or run the built binary directly:

```bash
./target/debug/ridi-router-cli --help
```

## Workflow

### 1. Generate RMDF tiles

```bash
ridi-router-cli generate-tiles \
  --input ./map-data/pbf/latvia-latest.osm.pbf \
  --output ./tiles \
  --tile-size-deg 1.0
```

You can also generate from a directory of `.pbf` files:

```bash
ridi-router-cli generate-tiles \
  --input-dir ./map-data/pbf \
  --output ./tiles \
  --tile-size-deg 1.0
```

### 2. Generate routes from tiles

Start-finish mode:

```bash
ridi-router-cli generate-route \
  --tiles ./tiles \
  --output-dir ./routes \
  --format gpx \
  --rule-file ./rule-examples/rules-fast.json \
  start-finish \
  --start 56.951861,24.113821 \
  --finish 57.313103,25.281460
```

Round-trip mode:

```bash
ridi-router-cli generate-route \
  --tiles ./tiles \
  --output-dir ./routes \
  --format json \
  --rule-file ./rule-examples/rules-fast.json \
  round-trip \
  --start-finish 56.951861,24.113821 \
  --bearing 35 \
  --distance 100000
  --distance 100000
```

## Output behavior

- `generate-route` writes one file per route into `--output-dir`
- final route payloads go to files, not stdout
- if `--output-dir` does not exist, the CLI creates it
- if `--output-dir` exists and is empty, it is reused
- if `--output-dir` exists and is not empty, the command fails
- if routing succeeds but finds no valid routes, the command still succeeds and leaves the output directory empty

## Rule files

Rule files are parsed by the CLI and passed into the routing library as Rust values.

Examples live in `./rule-examples/`.

A schema can be written with the optional feature:

```bash
cargo run -p ridi-router-cli --features rule-schema-writer -- \
  rule-schema-write --destination ./rule-examples/schema.json
```

## Library usage

### Routing library

```rust
use std::path::PathBuf;
use ridi_router_routing::{
    Coords, RouteMode, RouteRequest, RouterRules, RoutingExecutor, RoutingExecutorConfig,
};

let mut executor = RoutingExecutor::open(RoutingExecutorConfig {
    tiles_dir: PathBuf::from("./tiles"),
})?;

let result = executor.generate(RouteRequest {
    mode: RouteMode::StartFinish {
        start: Coords { lat: 56.95, lon: 24.11 },
        finish: Coords { lat: 57.31, lon: 25.28 },
    },
    rules: RouterRules::default(),
})?;
```

### Tile-generation library

```rust
use std::path::PathBuf;
use ridi_router_tiles::{generate_tiles, TileGenerationRequest, TileInputSource};

let summary = generate_tiles(TileGenerationRequest {
    input: TileInputSource::File(PathBuf::from("./region.osm.pbf")),
    output_dir: PathBuf::from("./tiles"),
    tile_size_deg: 1.0,
    db_path: None,
})?;
```

## Tests

```bash
cargo check --workspace
cargo test --workspace
cargo tree --workspace
```

## Optional RMDF viewer

The workspace still includes the RMDF viewer behind a feature flag:

```bash
cargo run -p ridi-router-cli --features rmdf-viewer -- \
  rmdf-viewer --input-dir ./tiles
```
