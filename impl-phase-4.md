# Phase 4: Routing-level proof and final verification

## Goal
Prove the start/finish snapping path honors the highway allowlist, then run the final validation pass.

## Files
- `crates/ridi-router-routing/src/routing_api.rs`
- possibly shared fixture usage from `crates/ridi-router-test-support/src/lib.rs`
- optional minor cleanup in `crates/ridi-router-routing/src/rmdf/tile_manager.rs`

## Planned changes
1. Add one focused routing-level test in `routing_api.rs` proving `WP_LOOKUP_ALLOWED_HWS` is honored during start/finish lookup.
   - nearest candidate attached only to disallowed highway like `track`
   - next candidate attached to allowed highway like `secondary`
   - expected result: routing snaps to the allowed candidate
2. Reuse the upgraded synthetic helper if practical.
   - If expressing the fixture through the helper becomes awkward, use a tiny purpose-built fixture generator instead.
3. Run targeted verification first:
   - `ridi-router-test-support` tests
   - `ridi-router-routing` tile-manager tests
   - `ridi-router-routing` routing-api tests
4. Then run broader crate validation and fix any fallout.

## Suggested validation commands
- `cargo test -p ridi-router-test-support`
- `cargo test -p ridi-router-routing tile_manager`
- `cargo test -p ridi-router-routing routing_api`
- `cargo test -p ridi-router-routing`

## Final checklist
- `limit_to_hw_tags` is honored
- `_rules` is honored
- filtering semantics match stable `main`
- same-tile search uses spatial index first
- full-tile fallback prevents false negatives within the tile
- final candidate choice uses Haversine distance
- routing API start/finish snapping honors `WP_LOOKUP_ALLOWED_HWS`

## Exit criteria
- The full todo is covered by unit + routing-level tests.
- Remaining non-goals stay out of scope:
  - no cross-tile nearest lookup expansion
  - no border-aware neighbor search
  - no rule-semantic redesign
