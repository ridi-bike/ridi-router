# Phase 8: Cleanup & Consolidation

## Overview

Remove all old code and systems: bincode cache, JSON import, deprecated CLI parameters. This is the breaking change that finalizes the RMDF transition.

**Goals:**
- Delete map_data_cache.rs entirely
- Delete json_reader.rs and json_parser.rs
- Remove pack() and unpack() methods from graph.rs
- Remove bincode from Cargo.toml
- Remove deprecated CLI parameters (--cache-dir, --input for routing)
- Update README and documentation

## Changes Required

### 1. Delete Cache System

**Files to delete**:
- `src/map_data_cache.rs`

**File**: `src/lib.rs`

**Changes**: Remove cache module

```rust
// DELETE THIS LINE:
// pub mod map_data_cache;

pub mod rmdf;
pub mod map_data;
pub mod osm_data;
pub mod router;
// ...
```

**Rationale**: Cache system no longer needed with RMDF.

### 2. Delete JSON Import

**Files to delete**:
- `src/osm_data/json_reader.rs`
- `src/osm_data/json_parser.rs`

**File**: `src/osm_data/mod.rs`

**Changes**: Remove JSON modules

```rust
pub mod data_reader;
pub mod pbf_reader;
pub mod pbf_area_reader;

// DELETE THESE:
// pub mod json_reader;
// pub mod json_parser;
```

**File**: `src/osm_data/data_reader.rs`

**Changes**: Remove JsonFile variant

```rust
// DELETE JsonFile variant from DataSource enum
#[derive(Debug, Clone)]
pub enum DataSource {
    PbfFile { file: PathBuf },
    // DELETE: JsonFile { file: PathBuf },
}

// DELETE json parsing logic from OsmDataReader
impl OsmDataReader {
    pub fn read_data(self) -> Result<MapDataGraph, OsmDataReaderError> {
        match self.source {
            DataSource::PbfFile { file } => {
                PbfReader::new(&mut self.map_data, &file).read()?;
            }
            // DELETE:
            // DataSource::JsonFile { file } => {
            //     JsonReader::new(&mut self.map_data, &file).read()?;
            // }
        }
        Ok(self.map_data)
    }
}
```

**Rationale**: JSON no longer supported (PBF only).

### 3. Remove Pack/Unpack from MapDataGraph

**File**: `src/map_data/graph.rs`

**Changes**: Delete serialization methods

```rust
// DELETE struct MapDataGraphPacked (lines 272-278)
// DELETE pub fn pack(&self) -> anyhow::Result<MapDataGraphPacked> (lines 292-342)
// DELETE pub fn unpack(packed: MapDataGraphPacked) -> anyhow::Result<&'static MapDataGraph> (lines 768-829)
```

**Rationale**: MapDataGraph only used for in-memory building during tile generation now.

### 4. Update Dependencies

**File**: `Cargo.toml`

**Changes**: Remove bincode, keep memmap2 and bytemuck

```toml
[dependencies]
anyhow = "1.0.95"
# DELETE: bincode = "1.3.3"
clap = { version = "4.5.9", features = ["derive"] }
# ... existing dependencies
memmap2 = "0.9"      # Keep (added in Phase 1)
bytemuck = "1.14"    # Keep (added in Phase 1)
hex = "0.4"          # Keep (added in Phase 4)
chrono = "0.4"       # Keep (added in Phase 4)
```

**Rationale**: bincode no longer used anywhere (tile generation uses RMDF writer).

### 5. Remove Deprecated CLI Parameters

**File**: `src/router_runner.rs`

**Changes**: Remove --cache-dir and --input from GenerateRoute

```rust
RouterCommand::GenerateRoute {
    // DELETE: input: Option<PathBuf>,
    // DELETE: cache_dir: Option<PathBuf>,

    /// Tiles directory (required)
    #[arg(long)]
    tiles: PathBuf,  // No longer Option

    #[arg(short, long)]
    output: Option<PathBuf>,

    #[arg(long)]
    rule_file: Option<PathBuf>,

    #[arg(long)]
    debug_dir: Option<PathBuf>,

    #[command(subcommand)]
    routing_mode: RoutingMode,
}
```

**File**: `src/router_runner.rs`

**Changes**: Remove PrepCache command entirely

```rust
// DELETE entire PrepCache variant:
// PrepCache {
//     input: PathBuf,
//     cache_dir: PathBuf,
// },

// DELETE PrepCache handler in run() method
```

**Rationale**: No caching system to prepare.

### 6. Update Documentation

**File**: `README.md`

**Changes**: Update usage examples

```markdown
## Usage

### Generate Tiles

```bash
ridi-router generate-tiles \
    --input montenegro.osm.pbf \
    --output ./tiles \
    --tile-size 0.1
```

### Generate Route

```bash
ridi-router generate-route \
    --tiles ./tiles \
    --routing-mode start-finish \
    --start 42.5,18.5 \
    --finish 42.6,18.6 \
    --output route.gpx
```

## Breaking Changes from v0.x

- **Removed**: JSON input support
- **Removed**: Bincode cache system
- **Removed**: `--cache-dir` and `--input` (for routing)
- **Added**: `generate-tiles` command (required before routing)
- **Added**: `--tiles` directory (required for routing)
```

**Rationale**: Clear migration guide for users.

## Success Criteria

### Automated Verification

- [ ] All deleted files removed from git
- [ ] No references to bincode remain: `rg bincode`
- [ ] No references to json_reader remain: `rg json_reader`
- [ ] No references to map_data_cache remain: `rg map_data_cache`
- [ ] Cargo check passes: `cargo check`
- [ ] All tests pass: `cargo test`
- [ ] Clippy clean: `cargo clippy -- -D warnings`

### Manual Verification

- [ ] CLI help shows only new commands
- [ ] Old commands rejected: `ridi-router prep-cache` (error)
- [ ] Old parameters rejected: `ridi-router generate-route --cache-dir ./cache` (error)
- [ ] README accurate
- [ ] CHANGELOG.md updated with breaking changes

## Dependencies

- **Depends on**: Phase 7 (routing must work before removing old code)
- **Blocks**: Phase 9 (E2E tests verify final clean system)

## Risks & Mitigations

**Risk**: Accidentally break tile generation
- **Mitigation**: MapDataGraph still used internally for building, just not for routing

**Risk**: Users confused by breaking changes
- **Mitigation**: Clear CHANGELOG, README migration guide, major version bump

## Notes

- This is the breaking change release (v1.0.0 or v0.9.0 → v1.0.0)
- No backward compatibility with old cache system
- Users must regenerate all data from PBF
- MapDataGraph remains for tile generation (in-memory building)
- bincode completely removed from codebase
