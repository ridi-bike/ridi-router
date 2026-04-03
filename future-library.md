# Future Library Notes

This document captures the library-related direction that should influence the refactor, even though the actual library work is out of scope for now.

---

## Scope Decision

Library work is **out of scope** for the current IPC simplification refactor.

However, the refactor should be deliberately **library-friendly**.
That means the refactor should avoid baking CLI-only or stdout/file-only assumptions into the routing core.

---

## Target Direction

The project is expected to become a Rust library that can be consumed by other Rust projects.

The future library should:
- expose pure in-memory final route results
- expose structured Rust errors
- expose streamed in-memory routing events while routing runs
- not write files directly
- not write to stdout/stderr directly

The consumer of the library will decide what to do with the data:
- render to UI
- write files
- print to stdout
- store in a database
- stream to another system

---

## Library-Friendly Design Constraints for This Refactor

The current refactor should aim for these properties:

1. **Separate routing from transport/output**
   - routing should produce in-memory results/events
   - file/stdout concerns should live in CLI/output layers

2. **Avoid IPC-shaped domain types**
   - request/response envelopes from the old socket model should not define the core data model

3. **Keep CLI orchestration thin**
   - future `src/lib.rs` extraction should be straightforward
   - the CLI should ideally become a wrapper over reusable application/core APIs

4. **Avoid stdout/file assumptions in core types**
   - NDJSON is a CLI/file serialization concern
   - the library should work with Rust event/result data, not serialized NDJSON strings

5. **Prepare for richer route/event models**
   - future data may include more than final route geometry and aggregate stats
   - it may include considered paths, fork weights, dead ends, backtracked paths, and similar internal routing details

---

## Future Output Model Split

A useful long-term split is:

### Core library layer
Owns:
- route requests
- route computation
- final route results
- streamed routing events
- structured errors

Does not own:
- file writing
- stdout/stderr writing
- CLI parsing
- NDJSON/JSON/GPX serialization policy

### CLI / adapter layer
Owns:
- clap argument parsing
- mapping CLI flags to core requests
- choosing destination format/mode
- writing GPX/JSON/NDJSON to files or stdout
- rendering human-readable errors for now

---

## Event Streaming Direction

For the future library:
- streamed routing progress should be exposed as in-memory Rust event structures
- not as NDJSON

For CLI/file consumers:
- NDJSON is only a serialization/output format for stdout or files

So there are two different concepts:

1. **Library event model**
   - Rust enums/structs
   - consumed directly by Rust callers

2. **CLI/file event format**
   - NDJSON text lines
   - derived from the library event model

This is an important distinction to preserve.

---

## Event Sink Direction

A future generic event sink abstraction is expected.
Examples could include:
- stdout sink
- append-to-file sink
- in-memory callback sink for library consumers
- channel-based sink

That abstraction is out of scope for the current refactor.

But the current refactor should avoid designs that assume:
- only stdout is possible
- only files are possible
- only final results matter

---

## Error Direction

Future intended layering:
- library returns structured Rust errors
- CLI converts them to human-readable stderr output for now
- future CLI work may add machine-readable error output modes

So this refactor should avoid hard-coding human-readable stderr strings as the only meaningful error representation.

---

## Final Route Result Direction

The future library should expose final route results as pure in-memory Rust data structures.

Those results should be able to grow over time to include:
- final route geometry
- route statistics
- road type / surface / smoothness details
- clustering/ranking metadata
- possibly debug/exploration metadata if needed later

The output writers in CLI land can then decide how much of that data to serialize for:
- GPX
- JSON
- NDJSON

---

## Implications for This Refactor

The current refactor should therefore prefer:
- transport-neutral result types
- output-format writers/adapters outside the routing core
- request execution APIs that could later be called from both CLI and library code

And it should avoid:
- encoding old IPC request/response envelopes into the new core design
- making stdout/file serialization the primary representation of route results
- mixing file naming or directory layout rules into the routing engine itself

---

## Explicitly Deferred

The following library decisions are intentionally deferred to later work:
- whether `src/lib.rs` is introduced immediately after this refactor or in a later one
- exact public Rust API surface
- exact event sink trait/interface
- exact event enum/struct definitions
- exact final route result struct layout
- how much routing-internal detail becomes public API vs optional/debug API

---

## Bottom Line

The library should eventually expose:
- in-memory route results
- in-memory streamed events
- structured Rust errors

The CLI should be only one consumer of that core.

This IPC simplification refactor should not implement the library, but it should make that future direction easier, not harder.
