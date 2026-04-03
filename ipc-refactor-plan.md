# IPC Refactor Plan

## Goal

Simplify routing execution from the old long-lived server/client model to a one-shot CLI model:

`routing request -> start cli -> discover RMDF files -> route -> respond`

This plan focuses on removing IPC-related architecture and making routing request-scoped again, while also planning ahead for:

- GPX output as **multiple files** (one per route)
- JSON output in a still-evolving schema
- NDJSON streaming for progress + final route results

No code changes are proposed in this document.

---

## Executive Summary

The codebase is already halfway out of the old IPC architecture:

- `generate-route --tiles ...` already exists and is the right direction.
- `src/ipc_handler.rs` still exists, but it is effectively **dead code**.
- Some IPC-era types are still reused as generic output types, which keeps transport concerns mixed into CLI routing.
- The routing core still depends on a global `MapDataGraph::get()` singleton, which is a leftover from the old always-running server model.
- `run_generate_route()` creates a request-local `TileManager`, but then uses an unsafe `'static` transmute and does not actually wire that manager into the global routing access path.

So the main refactor is not “remove an active IPC server”. The real work is:

1. remove dead IPC transport code,
2. remove IPC-shaped data types from normal CLI routing,
3. make routing state request-scoped again,
4. separate routing from output formatting and streaming.

---

## What I Found

### 1. IPC transport code is still present but unused

File:
- `src/ipc_handler.rs`

What is in there:
- local socket listener/client via `interprocess`
- request/response framing
- `RequestMessage`, `ResponseMessage`, `RouterResult`, `RouteMessage`
- server-ready stdout marker: `;RIDI_ROUTER SERVER READY;`

Current status:
- `cargo check` reports the whole `IpcHandler` surface as dead code.
- I did not find current CLI paths that call `IpcHandler::listen()` or `IpcHandler::connect()`.

Implication:
- IPC is no longer the real execution path.
- The refactor can remove it without replacing any active runtime dependency inside the current CLI.

### 2. IPC data types still leak into normal routing output

Files:
- `src/router_runner.rs`
- `src/result_writer.rs`
- `src/gpx_writer.rs`

Current coupling:
- `router_runner.rs` converts routes into `ipc_handler::ResponseMessage`
- `result_writer.rs` writes `ResponseMessage`
- `gpx_writer.rs` consumes `ipc_handler::RouteMessage`

Implication:
- Even though transport IPC is dead, the output layer is still shaped like server/client messaging.
- This will get in the way of future JSON schema work and NDJSON streaming.

### 3. The old server-oriented global state is still embedded in the routing core

File:
- `src/map_data/graph.rs`

Current structure:
- global `MAP_DATA_GRAPH: OnceLock<MapDataGraph>`
- routing code calls `MapDataGraph::get()` from many places
- `MapDataGraph` wraps `TileManager` behind an internal `RwLock`

This made sense for:
- a long-lived process that initialized once
- preloaded graph data shared across many requests

It is now awkward because:
- each CLI invocation should own its own routing state
- the new RMDF model makes startup cheap enough
- the singleton is now a leftover, not a requirement

### 4. `run_generate_route()` is in a transition state and likely wrong at runtime

File:
- `src/router_runner.rs`

Current flow:
- creates local `TileManager`
- unsafely transmutes `&mut TileManager` to `&'static mut TileManager`
- passes it into `generate_route_with_tiles()`
- but that parameter is unused
- actual routing still uses `MapDataGraph::get()` internally

Important detail:
- I found no current `MapDataGraph::init(tiles_dir)` call in the route execution path.
- `MapDataGraph::get()` expects prior initialization.

Implication:
- this is exactly the kind of transitional glue that should be removed, not extended.
- the unsafe transmute is a strong signal that the old architecture no longer fits.

### 5. Tile discovery is already mostly aligned with the new architecture

Files:
- `src/router_runner.rs`
- `src/rmdf/tile_manager.rs`
- `src/rmdf/generator/manifest.rs`

Current behavior:
- `generate-route` already takes `--tiles DIR`
- `TileManager::new()` loads `manifest.json`
- manifest generation already scans `.rmdf` files and records metadata

Implication:
- the routing entrypoint should treat the tiles directory + manifest as the only bootstrap input.
- this is a good fit for one-shot CLI routing.

### 6. stdout/stderr separation is already a good foundation for machine output

Files:
- `src/main.rs`
- `src/result_writer.rs`

Current behavior:
- tracing logs go to `stderr`
- JSON result output goes to `stdout` when no file is specified

Implication:
- this is exactly the right basis for future NDJSON streaming.
- machine protocol on `stdout`, diagnostics on `stderr`.

### 7. Some docs and helper commands are stale and still refer to old architecture

Files:
- `README.md`
- `justfile`

Examples found:
- `justfile` still has `start-server` / `start-client` recipes
- several recipes still use removed `--input` / `--cache-dir` routing flow
- README is partly updated, but helper tooling still shows old paths

Implication:
- documentation cleanup should be included in the refactor plan.

---

## Desired End State

A single route request should work like this:

1. Caller starts `ridi-router generate-route ...`
2. CLI validates request arguments
3. CLI discovers tiles via `--tiles DIR` and `manifest.json`
4. CLI builds a request-scoped routing context
5. Router computes routes
6. CLI writes one of:
   - GPX files
   - JSON
   - NDJSON event stream
7. Process exits

There should be:
- no socket setup
- no long-lived server process
- no request/response transport structs in the routing core
- no fake `'static` lifetimes to make request-scoped state look global

---

## Refactor Principles

### A. Keep routing request-scoped

Each CLI invocation should own its own:
- tile discovery
- tile manager
- routing context
- output mode

### B. Separate routing from transport/output

The routing engine should return routing-domain results.
It should not know whether the caller wants:
- a JSON file
- JSON on stdout
- GPX files
- NDJSON streaming

### C. Reserve stdout for protocol output

For future automation and streaming:
- `stdout` = machine-readable output/events only
- `stderr` = logs, tracing, human diagnostics

### D. Treat manifest discovery as the routing bootstrap

The route command should depend on a tiles directory contract, not on hidden global startup behavior.

### E. Plan now for streaming, even if not implemented in this refactor

The refactor should avoid locking the code into a “compute everything first, then serialize one big response” model.

---

## Recommended Refactor Shape

## 1. Remove IPC as an architectural concept, not just as a file

### Remove
- `src/ipc_handler.rs`
- `interprocess` dependency from `Cargo.toml`
- old server/client terminology from docs and recipes
- any stdout server-ready protocol remnants

### Replace with
Transport-neutral types in a new module, for example:
- `src/route_request.rs`
- `src/route_output.rs`
- or `src/application/mod.rs`

Possible shape:
- `RouteRequest`
- `RouteComputationResult`
- `RouteSummary` / `RouteGeometry`
- `RouteOutputEnvelope` only if needed for JSON compatibility

Key point:
- output models should be named after routing/output concerns, not IPC concerns.

---

## 2. Make routing context request-scoped

This is the most important structural change.

### Current anti-pattern
- global `OnceLock<MapDataGraph>`
- request-local `TileManager` created in runner
- unsafe transmute to `'static`
- actual routing reads from global singleton anyway

### Recommended direction
Introduce a request-scoped context shared through the routing stack.

For example:
- `RoutingContext` or `RoutingGraph`
  - owns `TileManager`
  - exposes `get_closest_to_coords`, `get_adjacent`, tag lookup, etc.

Then pass it through as:
- `Arc<RoutingContext>` across generator/navigator/walker
- keep interior locking inside the context if needed for lazy tile loading

This is a better fit than the current global singleton because:
- one CLI invocation = one context
- safe with rayon parallel itinerary generation
- no fake `'static` lifetimes
- much easier to test
- future streaming can report progress from the same request context

### Practical note
Because the current routing code calls `MapDataGraph::get()` in many places, this is the largest code-touching part of the refactor.

A reasonable migration path is:

#### Option A: Minimal transitional step
- keep `MapDataGraph` type
- remove the global `OnceLock`
- instantiate it per request
- pass `Arc<MapDataGraph>` into routing components

#### Option B: Cleaner rename
- rename/refactor `MapDataGraph` into `RoutingContext`
- make the old global model disappear completely

I recommend **Option A first**, then rename later if desired.
It gives most of the architectural benefit without forcing a rename-heavy refactor.

---

## 3. Simplify `RouterRunner` into a plain one-shot application flow

File:
- `src/router_runner.rs`

### New shape
`run_generate_route()` should become the canonical path:

1. read rules
2. validate tiles dir
3. initialize request-scoped routing context from tiles dir
4. resolve start/finish points
5. run generator
6. send results to output writer

### Remove from this path
- IPC-shaped response creation
- unused `generate_route_with_tiles()` parameter hack
- unsafe transmute
- any server/client abstractions

### Likely resulting split
- `RouterRunner` handles CLI parsing and orchestration
- `RouteService` or similar handles request execution
- `ResultWriter` handles formatting/output only

---

## 4. Redesign output around formats, not around old response messages

Current problem:
- output is modeled as `ResponseMessage { id, result }`
- this is a transport envelope from the old IPC protocol

### Recommended model
Keep routing output format-neutral internally.

For example, routing should produce something like:
- `Vec<ComputedRoute>`

Where each route contains:
- geometry
- summary stats
- optional per-segment or per-point metadata
- optional approximated/cluster metadata

Then separate writers can map that into:
- GPX
- JSON
- NDJSON events

### Why this matters
The future JSON schema is still unknown.
That means the internal model should be richer than the current `RouteMessage { coords, stats }` and not locked to the current envelope.

---

## 5. Plan GPX as a real multi-file output mode

Current state:
- `DataDestination::Gpx { file }`
- `GpxWriter` normally writes one GPX file containing multiple routes
- only `debug-split-gpx` writes multiple files

But your future requirement says:
- GPX output = **multiple files, one per route**

### Plan implication
GPX should stop being modeled as a single-file writer.

The output contract is now clarified for this refactor:
- use `--output-dir DIR`
- choose final format with `--format gpx|json`
- write one file per route
- do not support final GPX/JSON route payloads on stdout

Example concept:
- `--output-dir ./routes --format gpx` writes files like `001-354km.gpx`, `002-287km.gpx`, ...
- `--output-dir ./routes --format json` writes files like `001-354km.json`, `002-287km.json`, ...

The CLI cutover is also decided: remove `--output FILE` for `generate-route` and require `--output-dir` plus `--format`.

---

## 6. Plan JSON and NDJSON as separate output modes

These should not be treated as the same thing with different whitespace.

### JSON mode
Good for:
- final full response
- file output
- stdout in non-streaming mode

Likely shape:
- one final document
- request metadata
- route list
- per-route stats
- future structured segment attributes

### NDJSON mode
Good for:
- incremental progress
- long-running route generation
- machine consumption by another process

Likely shape:
- one event per line
- event types like:
  - `started`
  - `progress`
  - `route_candidate`
  - `completed`
  - `error`

### Strong recommendation
Do not model NDJSON as “stream chunks of the final JSON object”.
Instead, define it as an event stream.

That will be much easier to evolve.

---

## 7. Keep logs and progress separate

For NDJSON to work reliably:
- progress events intended for consumers must go to `stdout`
- tracing/logging must stay on `stderr`

The codebase is already close to this.
That is good and should be preserved.

---

## 8. Make tile discovery explicit and boring

Current bootstrap is already close:
- `--tiles DIR`
- `manifest.json`
- `.rmdf` files

### Recommendation
Treat `manifest.json` as required for routing.

Why:
- it gives a stable contract
- avoids expensive directory rescans on each route request
- provides metadata that can later support validation, version checks, and output annotations

### Optional fallback
You could allow manifest-less discovery by scanning `tile_*.rmdf`, but I would recommend **not** doing that unless there is a strong use case.

Manifest-required is simpler.

---

## Proposed Refactor Phases

### Scope clarification for the phases below

This plan now distinguishes between:

- the **immediate IPC simplification refactor**
- a **follow-up routing-core refactor** to remove global `MapDataGraph` usage

That split matters because the output/CLI cleanup is relatively contained, while replacing `MapDataGraph::get()` touches a large part of the routing stack. The immediate refactor should remove IPC concepts from the CLI boundary and land the new output contract without also taking on the full routing-core rewrite.

## Phase 1 - Untangle output from IPC

Primary goal:
- stop using `ipc_handler` types outside the IPC module

Changes to plan:
- introduce transport-neutral route result types
- update `result_writer.rs` to consume those
- update `gpx_writer.rs` to consume those
- update `router_runner.rs` to stop building `ResponseMessage`

Expected benefit:
- dead transport layer can be removed cleanly
- output design becomes ready for GPX/JSON/NDJSON evolution

Risk:
- low to medium

---

## Phase 2 - Adopt the new `generate-route` output contract

Primary goal:
- make the CLI match the locked product decisions for this refactor

Changes to plan:
- replace the current single-path output model with a strict cutover to `--output-dir` + explicit `--format gpx|json`
- remove `--output FILE` compatibility from `generate-route`
- make GPX output write one file per route
- make JSON output write one file per route
- stop using stdout for final GPX/JSON route payloads
- reserve stdout for future structured streaming only

Expected benefit:
- CLI behavior matches the intended product direction
- avoids carrying forward the current single-file/output-to-stdout assumptions
- makes later NDJSON work easier because stdout is no longer shared with final route payloads

Risk:
- medium, mostly around immediate CLI migration for existing users

---

## Phase 3 - Delete dead IPC transport

Primary goal:
- remove legacy server/client implementation completely

Changes to plan:
- delete `src/ipc_handler.rs`
- remove `interprocess` dependency
- remove IPC-specific error variants from `RouterRunnerError`
- remove old server/client docs and helper commands

Expected benefit:
- simpler codebase
- less confusion during future output work

Risk:
- low, once phases 1 and 2 are done

---

## Phase 4 - Clean up docs, scripts, tests

Primary goal:
- make repo tooling and tests reflect the new one-shot CLI model

Changes to plan:
- update `README.md`
- clean `justfile`
- remove `start-server` / `start-client` references
- add route integration tests for RMDF-based one-shot CLI
- add tests for `--output-dir` + `--format` behavior
- add tests that final GPX/JSON route commands do not write route payloads to stdout
- later add NDJSON protocol tests

Expected benefit:
- repo tells the truth again

Risk:
- low

---

## Deferred follow-up - Remove request/global mismatch

Primary goal:
- eliminate unsafe request-local-to-global glue in a dedicated routing-core refactor

Changes to plan:
- replace `MapDataGraph::get()` global access with request-scoped shared context
- remove `OnceLock<MapDataGraph>` from the routing runtime path
- remove `generate_route_with_tiles()` unused `'static mut TileManager` pattern
- thread context through generator/navigator/walker/weight logic

Expected benefit:
- architecture matches one-shot CLI execution
- no unsafe transmute
- no hidden init order dependency

Risk:
- medium to high
- this touches a lot of routing call sites

---

## Specific Files Likely Affected in the Future Refactor

Core orchestration:
- `src/router_runner.rs`
- `src/main.rs`

Legacy removal:
- `src/ipc_handler.rs`
- `Cargo.toml`
- `justfile`
- `README.md`

Routing context/global state:
- `src/map_data/graph.rs`
- `src/router/generator.rs`
- `src/router/navigator.rs`
- `src/router/walker.rs`
- `src/router/weights.rs`
- any route/stat/tag access that currently reaches through `MapDataGraph::get()`

Output:
- `src/result_writer.rs`
- `src/gpx_writer.rs`
- possibly a new `src/output/` module

---

## Architectural Recommendations

## Recommendation 1: do not replace IPC with another transport layer

If another tool wants to call the router, let it:
- spawn the CLI
- pass inputs via args/stdin
- read stdout/stderr

That keeps the router simple.

If a service wrapper is needed later, it can live outside the routing core.

## Recommendation 2: keep one request per process

Given RMDF mmap startup is now cheap enough, this is the simplest model.

Benefits:
- no lifecycle bugs
- no readiness signaling
- no stale loaded state
- no socket naming or server supervision
- easier streaming via stdout

## Recommendation 3: make the route engine return rich internal results

Do not let GPX or old JSON shape define your core route model.
Future JSON will likely need more than today’s:
- coords
- aggregate stats

So the internal model should be richer, even if the first public JSON stays simple.

## Recommendation 4: keep backward compatibility decisions explicit

Current JSON output appears to be an internal/legacy transport shape.
I would treat it as replaceable unless you explicitly need compatibility.

---

## Risks and Watchouts

### 1. Global state removal is the biggest real refactor

Removing IPC itself is easy.
Removing `MapDataGraph::get()` assumptions is the harder part.

### 2. Rayon + lazy tile loading needs a safe shared model

Because route generation uses parallel itinerary processing, the request-scoped context must still support concurrent access safely.
That likely means:
- shared context object
- internal locking around tile loading / lookup

### 3. Output design can accidentally get re-coupled

If the refactor introduces a new generic `Envelope { id, result }`, that would just recreate IPC in a new file.
Avoid that unless there is a real API reason.

### 4. Output-directory migration is a UX risk

The GPX multi-file direction is now decided: use `--output-dir` plus explicit `--format`.
The remaining risk is migration friction from the current `--output FILE` interface, not uncertainty about directory behavior. Existing empty directories are allowed; non-empty directories fail.

### 5. NDJSON progress must not depend on tracing logs

Progress for automation should be explicit structured events, not parsed log lines.

---

## Suggested Test Plan for the Refactor

### Must-have tests
- `generate-route --tiles ... --output-dir ... --format ...` works end-to-end without any IPC code present
- GPX output writes one file per route into a newly created output directory
- JSON output writes one file per route into a newly created output directory
- final GPX/JSON route commands do not emit route payloads to stdout
- missing/invalid tiles dir fails clearly
- manifest missing fails clearly
- non-empty `--output-dir` fails clearly instead of merging or overwriting
- existing empty `--output-dir` is accepted and populated
- a successful run that finds zero valid routes exits with code 0, leaves the output directory empty, and logs that no routes were found

### Important integration tests
- route across multiple tiles
- route when adjacent tile is missing
- route with and without `--rule-file`
- repeated CLI invocations do not depend on process-global state
- until stable route ordering is explicitly introduced later, tests should assert file count/extensions and valid contents rather than exact ordinal filenames or file order

### Future tests for NDJSON
- first event emitted quickly
- progress events are valid NDJSON lines
- final event contains completion status
- error path emits structured error event

---

## Scope Boundaries

## In scope for the IPC simplification refactor
- remove dead IPC transport plumbing
- stop using IPC response types for normal CLI routing
- switch final GPX/JSON outputs to `--output-dir` + explicit `--format`
- align docs/tooling/tests with the new architecture

## Explicitly out of scope for this refactor
- removing global `MapDataGraph` / `OnceLock` runtime access
- actual NDJSON streaming implementation
- final JSON schema design
- full output product design for road/surface/etc. metadata
- broader routing algorithm changes

But those out-of-scope items should still influence the internal design now.

---

## Decisions Locked So Far

These decisions are now settled for this refactor direction:

1. `--tiles DIR` remains the routing bootstrap, and `manifest.json` is required.
2. Current JSON output shape is disposable; no backward compatibility is needed for the new major version.
3. Output should move to a unified `--output-dir` approach for both GPX and JSON.
4. The CLI should choose final output format with an explicit `--format gpx|json`.
5. GPX output should write one file per route into `--output-dir`, with meaningful names such as `001-354km.gpx`.
6. JSON output should, for now, mirror the GPX file layout with one file per route, using names such as `001-345km.json`.
7. If `--output-dir` already exists and contains files, the CLI should fail rather than overwrite or merge.
8. If `--output-dir` does not exist, the CLI should create it.
9. Final GPX/JSON route outputs should stop supporting stdout in this refactor. Stdout should be reserved for future streaming-oriented use later.
10. Filename distance components like `001-345km.gpx` / `.json` should use the total route length rounded to the nearest whole kilometer.
11. For now, `--output-dir` should contain only per-route files. No aggregate file, index file, or manifest should be produced by the route command.
12. NDJSON is planned as an event stream, but the feature itself and its CLI parameter are out of scope for this refactor. The groundwork should allow future event destinations like stdout or an append-only file.
13. Request IDs are no longer needed and can be removed.
14. Removing global `MapDataGraph` / `OnceLock` state is explicitly split into a follow-up refactor, not this IPC simplification refactor.
15. The future library use case is important, but library work itself is out of scope for this refactor. This refactor should still be library-friendly.
16. The future library should expose pure in-memory Rust data for final routes, streamed events, and structured errors. It should not write files or stdout/stderr itself.
17. Future streaming will likely use a generic event sink abstraction, but that abstraction is out of scope for this refactor.
18. For now, CLI errors can stay human-readable on `stderr`, but the design should leave room for future structured library errors and future machine-readable CLI error modes.
19. Future route/event data should be rich enough to support more than final geometries and aggregate stats, including things like considered paths, fork weights, dead ends, and backtracking details.
20. This refactor may temporarily keep `MapDataGraph::get()` inside the routing core, as long as IPC transport/types are removed from the CLI boundary. The unsafe transmute/global-state cleanup belongs to the follow-up refactor.
21. Per-route file ordering is out of scope for this refactor. Files may be emitted in arbitrary generator order for now, and stable ordering can be introduced later.
22. Tests for this refactor should assume rule files come from disk paths. Rule-file-via-stdin support is not part of the planned CLI contract right now.
23. There will be no compatibility bridge from `--output FILE` to the new `--output-dir` + `--format` CLI. The cutover should be strict.
24. If route computation succeeds but yields zero valid routes, the command should still exit with code 0, leave `--output-dir` empty, and log that no routes were found.
25. An existing empty `--output-dir` is valid input and may be reused.

## Remaining Open Questions

At this point there are no remaining open questions for the IPC simplification refactor. The important CLI, output, and scope decisions have either been decided or explicitly deferred.
## My Recommendation

If the goal is simplicity and a clean long-term base, I would do this:

1. **Remove IPC-shaped output types first**
2. **Delete `ipc_handler.rs` completely**
3. **Keep `generate-route --tiles DIR` with required manifest as the only routing bootstrap**
4. **Redesign outputs around format writers and future event sinks**
5. **Keep this refactor compatible with a later library extraction**
6. **Handle global `MapDataGraph` removal in the dedicated follow-up refactor**

That gets the codebase aligned with the new RMDF reality instead of preserving server-era structure that no longer buys you anything.
