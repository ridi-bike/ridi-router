# Routing fix notes

This document explains what I changed to make route generation work for Latvia with:

```bash
./dev.sh route riga,latvia sigulda,latvia
```

## Result

The command now works and generates a GPX file in:

```text
map-data/routes/001-67km.gpx
```

## Main problems found

### 1. Rule parsing failed when no preset was passed
When `./dev.sh route ...` was run without a preset, the script did not pass a `--rule-file` argument to the CLI.

The CLI then tried to read rules from stdin. In this environment, stdin was present but empty, so the JSON parser tried to parse an empty string and failed with:

```text
Rule file error: Failed to parse JSON: EOF while parsing a value at line 1 column 0
```

### 2. Start point snapping failed even though Latvia tiles existed
After bypassing the rule parsing error, the route command still failed with:

```text
Routing error: Could not find start point on map
```

The tiles were present and contained data for Riga and Sigulda, so this error was misleading.

The real issue was in RMDF tag set reading.

Tag sets in the RMDF file were read using an aligned cast from raw mmap bytes. That is unsafe unless the byte offset is properly aligned for `TagSetRecord`.

In real generated tiles, tag set records were not guaranteed to be aligned. Because of that, reading a tag set could fail at runtime during closest-point lookup. That failure was swallowed and surfaced as “point not found”.

## Fixes made

### A. Treat empty stdin as default rules
File:
- `crates/ridi-router-cli/src/cli/rules.rs`

Change:
- `read_router_rules_from_stdin()` now returns `RouterRules::default()` if stdin is empty or whitespace-only.

Why:
- This prevents an empty stdin stream from being treated as invalid JSON.
- It makes the CLI behavior match the intended “default rules” behavior when no rule file is provided.

### B. Fix unaligned RMDF tag set reads
Files:
- `crates/ridi-router-routing/src/rmdf/io.rs`
- `crates/ridi-router-cli/src/rmdf/io.rs`
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

Change:
- Replaced aligned reference-based reads of `TagSetRecord` with `pod_read_unaligned(...)` and returned tag sets by value instead of by reference.

Why:
- `TagSetRecord` data inside the memory-mapped RMDF file may start at an offset that is not naturally aligned.
- Reading it as a typed reference can fail even though the bytes are valid.
- Unaligned reads are the correct way to decode these packed binary records.

Impact:
- Closest-point lookup can now correctly inspect adjacent road tags.
- The “missing start point” error caused by broken tag-set decoding is gone.

### C. Add a fallback snap without preferred-highway filtering
File:
- `crates/ridi-router-routing/src/routing_api.rs`

Change:
- Route generation first tries snapping with `WP_LOOKUP_ALLOWED_HWS`.
- If that finds nothing, it retries without the highway allowlist.

Why:
- The preferred-highway filter is useful, but it is stricter than necessary for finding a valid nearby start/finish node.
- In real map data, a valid nearby point may be on a road outside the preferred list.

Impact:
- Start/finish snapping is more robust.
- This helps avoid false “point not found” failures near valid roads.

### D. Improve closest-point error visibility
File:
- `crates/ridi-router-routing/src/map_data/graph.rs`

Change:
- Instead of silently turning lookup errors into `None`, the code now logs a warning before returning `None`.

Why:
- During debugging, this exposed the real failure:
  - invalid tag-set reads during point lookup
- Without logging, the system looked like it simply could not find points.

## Dev workflow change

### E. Make `dev.sh route` use a fast preset by default
Files:
- `dev.sh`
- `rule-examples/rules-fast.json`

Change:
- If no preset is provided, `dev.sh route` now uses:

```text
rule-examples/rules-fast.json
```

instead of relying on raw CLI defaults.

Why:
- Raw default route generation was much slower for this route.
- A tuned preset with reduced variation/retry work makes start-finish routing much faster in local development.

Current default fast preset:
- disables start-finish waypoint variation
- disables retry bearing variation
- uses only `avoid_residential = [false]`
- keeps basic routing behavior intact
- reduces `step_limit` to `5000`

Impact:
- `./dev.sh route riga,latvia sigulda,latvia` now completes in a practical amount of time.
- On this machine it completed in roughly 30–60 seconds, depending on build state.

## Test updates

Files:
- `crates/ridi-router-cli/tests/generate_route_cli.rs`
- `crates/ridi-router-cli/tests/generate_tiles_cli.rs`
- `README.md`

Changes:
- Updated tests to use `rule-examples/rules-fast.json`.
- Updated expectations for end-to-end route tests so they expect output files to be created.
- Updated README examples to reference `rules-fast.json`.

Why:
- The previous test expectations assumed no routes for the repo fixture.
- With working snapping and a practical preset, the fixture now generates a route successfully.

## Verification performed

I ran:

```bash
./dev.sh route riga,latvia sigulda,latvia
```

and it succeeded, producing:

```text
map-data/routes/001-67km.gpx
```

I also ran:

```bash
cargo test -p ridi-router-cli --test generate_route_cli
```

and the test target passed.

## Files changed

- `dev.sh`
- `README.md`
- `rule-examples/rules-fast.json`
- `crates/ridi-router-cli/src/cli/rules.rs`
- `crates/ridi-router-cli/src/rmdf/io.rs`
- `crates/ridi-router-cli/tests/generate_route_cli.rs`
- `crates/ridi-router-cli/tests/generate_tiles_cli.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- `crates/ridi-router-routing/src/rmdf/io.rs`
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- `crates/ridi-router-routing/src/routing_api.rs`

## Summary

The route command was failing for three practical reasons:

1. empty stdin was parsed as JSON rules
2. RMDF tag sets were read incorrectly because of alignment assumptions
3. the default dev route flow was slower and less practical than needed

After fixing those issues, route generation works for the Latvia tiles and the default `dev.sh route` flow is usable again.
