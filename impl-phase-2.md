# Implementation Phase 2: zero-copy rule access and direct rule hydration

## Goal
Remove the largest allocation hotspot by stopping whole-tile rule copies on every point lookup.

## Why this is phase 2
Phase 1 removes wasted work first. Phase 2 then attacks the biggest allocator with a tighter, more invasive refactor.

## Scope

### In scope
1. Change `MappedTile` rule accessors from owned vectors to borrowed section views:
   - `get_rules() -> Result<&[RuleRecord]>`
   - `get_rule_line_refs_payload() -> Result<&[u64]>`
2. Update `TileManager::get_rules_for_point(...)` to slice directly from borrowed rule sections.
3. Remove intermediate whole-section allocation from rule hydration.
4. Prefer exact-capacity allocation for only the selected point rules.
5. If clean, replace `TilePointRule` temporary allocation with a borrowed/lightweight view or direct final hydration path.

### Explicitly out of scope
- read-mostly locking changes
- task-local caches
- call-site API cleanup

## Expected code touch points
- `crates/ridi-router-routing/src/rmdf/io.rs`
- `crates/ridi-router-routing/src/rmdf/tile_manager.rs`
- `crates/ridi-router-routing/src/map_data/graph.rs`
- RMDF IO tests and graph rule hydration tests

## Implementation notes
Preferred order:
1. make mmap-backed getters borrow safely
2. make `get_rules_for_point` slice selected rules only
3. only after that, simplify the final materialization path

If a borrowed-view API becomes awkward, use this fallback:
- keep `get_rules_for_point` returning owned data
- but allocate only for the selected point's rules and line-ref slices
- do not rebuild the entire tile rule section per call

## Deliverables
- no full `Vec<RuleRecord>` allocation per point lookup
- no full `Vec<u64>` rule payload allocation per point lookup
- smaller per-point allocations proportional to that point's actual rule count
- tests covering bounds, empty slices, and equivalent hydrated rules

## Validation
1. Run RMDF IO tests.
2. Run routing crate tests that cover rule hydration.
3. Run `cargo check --workspace`.
4. Re-run perf and compare:
   - `tile_manager::get_rules_for_point`
   - `graph::get_point_from_tiles`
   - cumulative allocation totals for both

## Exit criteria
- rule access is zero-copy from the mmap until the final needed materialization step
- allocation totals for `get_rules_for_point` drop sharply
- point rule behavior is unchanged

## Main risk
Borrowed mmap-backed slices can make lifetimes and helper APIs trickier. Keep the design narrow and local to tile/rule access.

## Rollback plan
Land borrowed getters first. If direct final hydration adds too much complexity, stop at "selected-point-only owned allocation" and profile again.
