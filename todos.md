# Todos

Deferred issues identified during the `ridi-router-tiles` generation-model planning session.

## Restriction and rules pipeline

These were intentionally left out of the generation refactor scope and should be handled later as a separate end-to-end task.

- Implement generation-time turn restriction materialization from collected OSM restriction relations.
  - Current state: restriction relations are collected upstream, passed into `insert_relation(...)`, and then dropped.
- Define the generation-side restriction data model that should exist once restrictions are actually supported again.
  - The refactor plan removes the fake tiles-side rule model instead of pretending it works.
- Implement RMDF rule serialization.
  - `serialize_rules(...)` is still a stub.
  - Point `rules_offset` / `rules_count` bookkeeping is still incomplete.
- Implement RMDF rule reading on the routing side.
  - Add read helpers for the `RULES` section.
  - Add `TileManager` support for loading rules.
- Hydrate runtime routing points with rule data from tiles instead of hardcoding empty rules.
- Add tile-backed end-to-end tests proving that generated restriction data changes routing behavior.

Related review files:
- `todo-review-rules-serialization.md`
- `todo-review-turn-restrictions-rule-model.md`

## Post-refactor architecture follow-up

These are worth revisiting after the `generation` refactor lands.

- Reassess what overlap still exists between `ridi-router-tiles` generation types and `ridi-router-routing` runtime types.
  - Do this after the tiles crate is generation-shaped.
  - Do not force a shared crate before that cleanup lands.
- Re-evaluate whether any remaining shared concepts should move into a common crate later.
  - Especially low-level non-runtime data types, if any still remain duplicated after the refactor.

## Routing/runtime parity checks

- Verify whether tile-backed routing behavior still differs from in-memory/test-only graph behavior after the generation refactor.
- Identify any routing tests that currently pass only because they rely on test helpers instead of tile-backed RMDF data.


## Test fixture follow-up

- Restore or replace the workspace `map-data/output` RMDF fixture expected by `ridi-router-cli` end-to-end tests.
  - Current state: `cargo test -p ridi-router-cli` fails in this checkout because `map-data/output/manifest.json` is missing.
  - Prefer a deterministic checked-in synthetic fixture or per-test setup over an implicit local generated dataset.

- Document current local bootstrap path for missing fixtures.
  - `./dev.sh pbf montenegro` creates `map-data/pbf/montenegro-latest.osm.pbf`, which unblocks `crates/ridi-router-cli/tests/generate_tiles_cli.rs`.
  - `./dev.sh generate-tiles latvia` creates `map-data/output`, which unblocks the route CLI tests that currently expect `map-data/output/manifest.json`.
  - This path is networked, slow, and mutates shared workspace directories, so it should remain a temporary local bootstrap, not the final stable test-fixture strategy.
- Replace implicit workspace fixture assumptions in tests with deterministic fixtures.
  - Prefer checked-in small synthetic RMDF fixtures or per-test fixture generation helpers over relying on `map-data/output` existing locally.
  - Avoid tests depending on `./dev.sh` side effects or downloaded Geofabrik datasets.