# Library Refactor Plan

## Purpose

Refactor the project from a single mixed CLI/application crate into a workspace with reusable libraries and a thin CLI consumer.

This refactor is intended to make the route-generation and tile-generation logic reusable from Rust without forcing CLI, file-output, stdout/stderr, or JSON/GPX concerns into the core APIs.

This document captures the target architecture, locked decisions, design guidelines, API direction, and module ownership. Implementation phases will be documented separately.

---

## Locked decisions

### High-level
- This refactor **will introduce libraries now**.
- The repo should become a **Cargo workspace with a virtual root**.
- The main structure should be **three primary crates**:
  - `ridi-router-cli`
  - `ridi-router-tiles`
  - `ridi-router-routing`
- A fourth crate such as `ridi-router-common` is allowed **if and when it clearly helps with shared pure data types or format types**.
- This is a **breaking major-version refactor**. Backward compatibility is not required.
- The migration style should be **incremental with cleanup**, not a big-bang rewrite.

### Binary and package naming
- The CLI crate should be named `ridi-router-cli`.
- The produced binary should also be named `ridi-router-cli`.

### Library split
- `ridi-router-tiles`
  - reads PBF input
  - processes map data
  - writes RMDF tiles and manifest to disk
- `ridi-router-routing`
  - reads tiles from disk
  - performs route navigation
  - produces pure Rust route results
- `ridi-router-cli`
  - parses CLI arguments
  - parses rule files
  - maps CLI input into Rust request types
  - adapts Rust results into JSON and GPX
  - owns stdout/stderr rendering and file output policy

### Rules and request ownership
- Rules should be parsed by the CLI and passed to the routing library as Rust data structures.
- The routing library should not own CLI rule-file parsing.
- The routing library may own the canonical rule types if that is the cleanest ownership boundary.

### Routing graph transition plan
- The current process-global graph may remain temporarily as an **internal implementation detail** during the library split.
- The routing library should move graph initialization behind `RoutingExecutor::open(...)` rather than requiring CLI-owned global setup.
- During this interim phase, `RoutingExecutor::open(...)` must reject attempts to open a different `tiles_dir` after the process-global graph has already been initialized.
- This means the short-term contract is still effectively **one tiles dataset per process**, even though the public API becomes executor/session-shaped.
- A later dedicated refactor should remove the process-global graph from routing internals entirely and make the routing context instance-scoped.

### API direction
- The routing library should use an **executor/session-style API**.
- Streaming/event APIs should **not** be defined in this refactor.
- The design should leave room to add streaming later without redoing the public shape.

### Result and serialization boundary
- Keep the routing result shape close to the current small model for now.
- Do **not** expand public route results yet into large debug/exploration models.
- CLI JSON and GPX are allowed to diverge from internal Rust data structures.
- Do not force divergence for its own sake either; choose the best shape per boundary.

### Error direction
- Libraries should use typed error enums with meaningful variants.
- `anyhow` should be removed from library boundaries.
- CLI error rendering should sit on top of library errors.

### Request/process model
- One request per process remains the intended operational model.
- The routing executor may be kept alive and reused for multiple route requests **sequentially**.
- It should not be designed around concurrent multi-request handling.
- In the interim singleton-backed implementation, only one `tiles_dir` may be opened per process.
- Opening another executor for the same `tiles_dir` may reuse the already-opened internal graph.
- Opening another executor for a different `tiles_dir` in the same process should fail with a typed initialization error.
- Tile generation is single-request, single-process, one input source at a time, one output dir at a time.

---

## Target workspace shape

```text
/Cargo.toml                     # virtual workspace root
/crates/ridi-router-cli
/crates/ridi-router-routing
/crates/ridi-router-tiles
/crates/ridi-router-common      # optional, only if justified
```

### Workspace intent
- The root becomes a virtual workspace and no longer contains the application crate directly.
- Each primary responsibility gets its own crate boundary.
- The CLI depends on both libraries.
- The two libraries should remain independent unless a clean shared-data crate is needed.

### Dependency direction
Preferred dependency direction:

```text
ridi-router-cli
  ├── ridi-router-routing
  ├── ridi-router-tiles
  └── ridi-router-common (optional)

ridi-router-routing
  └── ridi-router-common (optional)

ridi-router-tiles
  └── ridi-router-common (optional)
```

Avoid:
- `ridi-router-routing -> ridi-router-tiles` just to borrow manifest/data types if a tiny shared crate is cleaner.
- `ridi-router-tiles -> ridi-router-routing`
- a large catch-all common crate that becomes a dumping ground.

---

## Crate responsibilities

## 1. `ridi-router-routing`

### Purpose
Own reusable route computation over already-generated tiles.

### Owns
- route request types
- route mode types
- rule types if they are routing-specific
- route computation
- tile opening/reading for routing
- request-scoped routing executor/session
- pure route result types
- typed routing errors

### Does not own
- clap parsing
- JSON serialization policy
- GPX serialization policy
- stdout/stderr output
- output directory policy
- route file naming
- rule-file parsing from disk

### Caller contract
The caller should provide:
- a tiles directory path at executor construction time
- a parsed Rust request for each route run
- parsed rules as Rust values

The routing library should provide:
- typed initialization/open failures
- typed route execution failures
- pure in-memory route results

---

## 2. `ridi-router-tiles`

### Purpose
Own conversion from OSM PBF input into on-disk RMDF tile output.

### Owns
- reading input PBF data
- intermediate processing needed to build tiles
- writing RMDF tile files
- writing manifest data
- typed tile-generation errors
- request/config types for tile generation
- optional typed generation summary result

### Does not own
- clap parsing
- stdout/stderr rendering
- generic CLI UX
- route generation
- JSON/GPX route output

### File I/O policy
Unlike the future routing library direction, the tile-generation library **does own disk writing**. That is the correct responsibility boundary for this crate.

This is not a contradiction. The routing library is meant to be pure at the result/output boundary, while the tile-generation library is fundamentally a producer of on-disk tile artifacts.

---

## 3. `ridi-router-cli`

### Purpose
Be a thin consumer of the two libraries.

### Owns
- `clap` argument parsing
- CLI subcommand definitions
- parsing rule files from disk into Rust values
- converting CLI inputs into library request/config types
- JSON output generation
- GPX output generation
- file naming
- output directory validation
- stdout/stderr policy
- human-readable error rendering

### Design rule
The CLI should orchestrate. It should not own the routing algorithm, tile-generation internals, or domain modeling.

---

## API direction

## Routing library API

The routing library should target an explicit executor/session shape.

### Recommended public shape

```rust
pub struct RoutingExecutor { /* private fields */ }

pub struct RoutingExecutorConfig {
    pub tiles_dir: std::path::PathBuf,
    // future non-CLI-specific options can live here
}

pub struct RouteRequest {
    pub mode: RouteMode,
    pub rules: RouterRules,
}

pub enum RouteMode {
    StartFinish { start: Coords, finish: Coords },
    RoundTrip { start_finish: Coords, bearing: f32, distance: u32 },
}

impl RoutingExecutor {
    pub fn open(config: RoutingExecutorConfig) -> Result<Self, RoutingError>;
    pub fn generate(&mut self, request: RouteRequest) -> Result<RouteComputation, RoutingError>;
}
```

> Interim implementation note: the first library version may still back `RoutingExecutor` with a hidden process-global `MapDataGraph` initialized from `open(config)`. That is acceptable for this refactor as long as conflicting `tiles_dir` openings are rejected explicitly instead of silently reusing the wrong dataset.

### Why executor/session is the right fit
Compared with a single free function, the executor shape gives a stable place for:
- one-time tile opening/bootstrap
- request execution methods
- future execution options
- future progress/event hooks
- future validation and policy knobs
- test setup around a reusable opened routing context

Compared with a simpler `Router::open(...).generate(...)` service object, the explicit executor/config naming better matches the direction that the object represents execution state and future extension points.

### Sequential reuse rule
The executor may be reused for multiple route requests in one process, but only **sequentially**.

That should be reflected in the API design:
- prefer `&mut self` on request execution methods
- do not promise concurrent request execution
- do not shape the API around multi-client server semantics

### Streaming preparation without streaming API now
Do not introduce public event enums, sink traits, or callback interfaces in this refactor.

Instead, keep the public shape such that later additions can fit naturally, for example:
- a future `generate_with_events(...)`
- a future optional observer/callback argument
- a future executor config field for event behavior

The key requirement is to avoid a public API that would force future streaming to be retrofitted awkwardly.

---

## Tile-generation library API

Tile generation should stay simpler than routing.

### Recommended public shape

```rust
pub enum TileInputSource {
    File(std::path::PathBuf),
    Directory(std::path::PathBuf),
}

pub struct TileGenerationRequest {
    pub input: TileInputSource,
    pub output_dir: std::path::PathBuf,
    pub tile_size_deg: f32,
    pub db_path: Option<std::path::PathBuf>,
}

pub struct TileGenerationSummary {
    // exact fields to be decided pragmatically
}

pub fn generate_tiles(request: TileGenerationRequest)
    -> Result<TileGenerationSummary, TileGenerationError>;
```

### Why not an executor here
The tile-generation workflow is naturally single-run and output-oriented:
- one input source at a time
- one output dir at a time
- no meaningful long-lived request session needed

So a single request-shaped API is preferred over inventing a reusable executor that adds little value.

---

## Data model guidance

## Routing result model
Keep the routing library result model intentionally small for this refactor.

A shape close to the current one is acceptable:
- route coordinates/geometry
- route stats

Do not expand public results yet with:
- considered paths
- dead ends
- backtracking traces
- fork-weight details
- internal exploration state

Those can come later if and when the library API actually needs them.

### Important boundary rule
The routing library result type is the canonical Rust domain result.

The CLI JSON and GPX output models are adapters over that result, not the other way around.

That means:
- do not let JSON shape define the library structs
- do not let GPX limitations define the library structs
- do allow CLI-specific representation choices where useful

### Pragmatic divergence rule
CLI output and library structs may differ where their needs differ:
- library boundary: idiomatic Rust, correctness, safety, future evolution
- CLI JSON boundary: ease of use and sensible machine-readable output
- GPX boundary: format-specific representation and metadata

Do not create a second model unless it improves one of those boundaries.

---

## Error model guidance

## Library errors
Both libraries should expose explicit error enums.

Examples of the intended style:
- `RoutingError`
- `TileGenerationError`

Those enums should have variants for actual domain/runtime failure categories, not generic catch-all wrappers.

Examples of good categories:
- tiles directory missing
- manifest invalid
- tile read failure
- start point not found
- finish point not found
- invalid route request
- route generation failure
- output directory invalid
- PBF read failure
- manifest write failure
- tile write failure

### Error rules
- Avoid `anyhow::Result` in library public APIs.
- Avoid stringly typed error states where the caller could benefit from a real variant.
- Preserve underlying causes where useful.
- Keep errors suitable for both Rust callers and CLI rendering.

## CLI errors
The CLI should compose library errors into CLI-facing presentation.

That can mean:
- a CLI-specific error enum that wraps library errors
- formatting into human-readable stderr messages
- process exit handling in the CLI binary only

The CLI should not force human-readable strings to be the only meaningful representation of failure.

---

## Shared-type guidance

A shared crate is allowed, but should remain small and data-oriented.

### Best candidates for a future `ridi-router-common`
- RMDF manifest schema/types
- RMDF format identifiers/version constants
- small pure shared structs/enums needed by both libraries

### What should not go into common
- route generation behavior
- tile-generation behavior
- CLI helpers
- broad utility code with weak ownership

### Manifest ownership guidance
The manifest format is primarily dictated by the tile format, so its conceptual home starts near tile generation.

However, because both libraries will likely need to understand the manifest contract, the manifest schema is a strong candidate for extraction into `ridi-router-common` if that avoids awkward cross-library dependencies.

Pragmatic rule:
- if manifest types can stay cleanly in one library without distorting dependency direction, that is fine
- if both libraries need the schema directly, extract the pure manifest types into common

---

## CLI boundary rules

## Route generation command
The CLI should be the only layer that owns:
- JSON serialization
- GPX serialization
- route file naming
- output directory validation
- stdout/stderr presentation
- final user-facing error text

The routing library should return Rust values only.

## Tile generation command
The CLI should also stay thin for tile generation:
- parse args
- build a `TileGenerationRequest`
- call the tile-generation library
- render typed errors for users

The tile-generation library should perform the real work.

---

## Ownership of current code

The current codebase is still arranged as one application crate. The likely destination of existing modules is approximately:

### Likely to move toward `ridi-router-cli`
- current `src/main.rs`
- current CLI parsing/orchestration from `src/router_runner.rs`
- current `src/json_writer.rs`
- current `src/gpx_writer.rs`
- current `src/result_writer.rs`
- current `src/file_naming.rs`

### Likely to move toward `ridi-router-routing`
- current `src/router/*`
- current `src/route_output.rs` or its replacement
- current `src/rmdf/tile_manager.rs`
- current routing-side tile read/bootstrap logic
- current non-global graph-backed route execution logic
- routing-specific rule types and validation, if they remain routing-owned

### Likely to move toward `ridi-router-tiles`
- current `src/rmdf/generator/*`
- current `src/osm_data/*`
- current map-processing pieces used only for tile building
- current writer logic for generated tile artifacts

### Candidate for `ridi-router-common` if needed
- current `src/rmdf/format.rs`
- current manifest schema/types from `src/rmdf/generator/manifest.rs`
- small pure RMDF schema/data types used from both sides

This is directionally correct, not a rigid move list. Ownership should be decided by responsibility, not by preserving current folder names.

---

## Refactor principles

### 1. Separate domain logic from adapters
The libraries should expose domain-level Rust APIs.
The CLI should adapt those APIs to files, flags, JSON, GPX, and stderr.

### 2. Keep public library APIs small and stable-looking
Do not expose internal helper types just because they are convenient during extraction.

### 3. Prefer explicit request/config structs over long parameter lists
This makes later growth easier without churn.

### 4. Keep IO ownership obvious
- routing lib: reads tiles, returns Rust results, no JSON/GPX/stdout/file-output ownership
- tile lib: reads PBF, writes tiles/manifest
- CLI: reads CLI/rule files, writes final route outputs, prints

### 5. Do not design for server semantics
No request IDs, no envelope types, no transport-shaped core models, no multi-client assumptions.

### 6. Keep concurrency claims modest
Sequential request handling is enough.
Do not complicate public APIs for unsupported concurrency scenarios.

### 7. Extract common only when it buys clarity
A small pure-data common crate is acceptable.
A vague utilities crate is not.

### 8. Remove `anyhow` from library surfaces
Typed enums are part of the goal, not cleanup polish.

### 9. Move parsing out of reusable libraries where appropriate
Especially:
- clap parsing
- rule-file parsing from disk
- JSON/GPX output shaping

### 10. Keep the CLI thin enough that it is obviously only one consumer
A future Rust integration should feel like it is calling the libraries directly, not reusing CLI-shaped code.

---

## Migration approach guidelines

This document does not define implementation phases, but the migration style should follow these principles:

### Incremental extraction
- first clean boundaries inside the current codebase
- then move modules into workspace crates
- keep behavior stable while responsibilities are being separated

### Clean before move
Before relocating code, strip out mismatched ownership such as:
- CLI parsing inside reusable modules
- output-writing logic inside routing logic
- broad application error handling inside libraries

### Minimize temporary compatibility shims
Because this is a breaking major-version refactor, prefer clean boundaries over compatibility wrappers that preserve old structure.

### Preserve test intent while moving ownership
Tests should move with the responsibility they validate:
- domain behavior tests with the library
- output/CLI behavior tests with the CLI crate

---

## Non-goals for this document

This document intentionally does not lock:
- exact implementation phases
- exact final module names inside each crate
- exact public field layout of all request/result structs
- future streaming event enums/traits
- future richer route exploration/debug API

Those should be decided in dedicated implementation-phase documents once the workspace and ownership boundaries are agreed.

---

## Desired end state

When this refactor is complete:
- the repo is a virtual-root Cargo workspace
- `ridi-router-routing` can be used from Rust without pulling in CLI/output concerns
- `ridi-router-tiles` can be used from Rust to build on-disk tiles with typed errors
- `ridi-router-cli` is a thin adapter over both libraries
- route JSON and GPX output live only in the CLI crate
- libraries expose typed errors instead of `anyhow`
- the routing API is executor/session-shaped and ready for future streaming extension
- no public library API is shaped around old IPC or CLI transport ideas

---

## Bottom line

The refactor should split the project into:
- a reusable tile-generation library that owns PBF-to-tile conversion and tile writing
- a reusable routing library that owns tile-backed route computation and returns Rust data
- a thin CLI crate that parses user input and converts Rust values into JSON/GPX and human-readable terminal behavior

The key architectural choice is:
- **executor/session API for routing**
- **single-request API for tile generation**
- **all CLI/file/serialization policy kept out of the reusable libraries**

That gives the project a clean long-term base for both direct Rust reuse and future features like streamed routing events, without prematurely defining those event APIs now.
