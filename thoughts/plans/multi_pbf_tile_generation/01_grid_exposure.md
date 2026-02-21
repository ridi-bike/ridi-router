# Phase 1: Grid Exposure from InMemoryPbf

## Overview

Add a new method to `InMemoryPbf` that returns both the loaded PBF data and the computed `RasterizedProximityGrid`. This enables multi-PBF processing to store the grid for overlap re-evaluation.

This phase comes first because `process_single_pbf()` needs the grid to store it in redb.

## Changes Required

### 1. Add Grid Return Method to InMemoryPbf

**File**: `src/osm_data/in_memory_pbf.rs`

**Changes**: Add new public method after the existing `from_pbf_file_with_flags()` method (around line 345).

**Implementation approach**:
1. Create new method `from_pbf_file_with_grid()`
2. Use existing `compute_proximity_flags_with_grid()` from `src/proximity/flag_computer.rs` instead of `compute_proximity_flags()`
3. Return tuple `(InMemoryPbf, RasterizedProximityGrid)`

**Rationale**: The existing `from_pbf_file_with_flags()` discards the grid after applying flags. We need a variant that returns it. This doesn't modify the existing method, maintaining backwards compatibility for single-PBF mode.

### 2. Ensure Proper Imports

**File**: `src/osm_data/in_memory_pbf.rs`

**Changes**: Verify import of `compute_proximity_flags_with_grid` from `crate::proximity`.

The import likely exists already (from `compute_proximity_flags`), just need to add the `_with_grid` variant.

## Success Criteria

### Automated Verification:
- [ ] New method compiles: `cargo check`
- [ ] Unit test passes: Create PBF, call new method, verify both return values are valid
- [ ] Grid matches what `compute_proximity_flags_with_grid()` returns independently

### Manual Verification:
- [ ] Code review: Method signature is clean and follows existing patterns
- [ ] No changes to existing `from_pbf_file_with_flags()` behavior

## Dependencies

- Depends on: None - can start immediately
- Blocks: Phase 2 (Intermediate Storage)

## Risks & Mitigations

- **Risk**: Changing InMemoryPbf might affect single-PBF mode
  - **Mitigation**: Add new method, don't modify existing one. Run single-PBF tests to verify.

## Notes

The `RasterizedProximityGrid` is ~36 bytes per cell. For a 1-degree tile with 100m cells, that's roughly 3600 cells = ~130KB per grid. For large PBFs, grids could be megabytes in size.
