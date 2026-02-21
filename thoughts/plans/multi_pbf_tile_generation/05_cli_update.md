# Phase 5: CLI Update

## Overview

Update the CLI to support directory input for multi-PBF tile generation. Add `--input-dir` option (mutually exclusive with existing `--input`), add `--db-path` for intermediate storage, and dispatch to `MultiPbfGenerator` when directory input is used.

This phase comes last because it needs the full multi-PBF pipeline to be complete.

## Changes Required

### 1. Update GenerateTiles Command Structure

**File**: `src/router_runner.rs`

**Changes**: Update `GenerateTiles` variant (lines 214-226).

Current:
```rust
GenerateTiles {
    #[arg(short, long, value_name = "FILE")]
    /// Input OSM PBF file
    input: PathBuf,

    #[arg(short, long, value_name = "DIR")]
    /// Output directory for tiles and manifest
    output: PathBuf,

    #[arg(long, default_value = "1.0")]
    /// Tile size in degrees (e.g., 0.1, 1.0)
    tile_size: f32,
},
```

New:
```rust
GenerateTiles {
    #[arg(short, long, value_name = "FILE", conflicts_with = "input_dir")]
    /// Input OSM PBF file (single file mode)
    input: Option<PathBuf>,

    #[arg(long, value_name = "DIR", conflicts_with = "input")]
    /// Input directory containing OSM PBF files (multi-file mode)
    input_dir: Option<PathBuf>,

    #[arg(short, long, value_name = "DIR")]
    /// Output directory for tiles and manifest
    output: PathBuf,

    #[arg(long, default_value = "1.0")]
    /// Tile size in degrees (e.g., 0.1, 1.0)
    tile_size: f32,

    #[arg(long, value_name = "FILE")]
    /// Path for intermediate database (multi-file mode only)
    /// Default: temp file in output directory
    db_path: Option<PathBuf>,
},
```

### 2. Add Validation for Input Options

**File**: `src/router_runner.rs`

**Changes**: Add validation in the match arm:

```rust
CliMode::GenerateTiles {
    input,
    input_dir,
    output,
    tile_size,
    db_path,
} => {
    // Validate exactly one input option is provided
    match (input, input_dir) {
        (Some(_), Some(_)) => {
            anyhow::bail!("Cannot use both --input and --input-dir");
        }
        (None, None) => {
            anyhow::bail!("Must provide either --input or --input-dir");
        }
        _ => {}
    }
    
    // ... rest of dispatch logic
}
```

### 3. Update Dispatch Logic

**File**: `src/router_runner.rs`

**Changes**: Update the dispatch logic (lines 449-466):

```rust
CliMode::GenerateTiles {
    input,
    input_dir,
    output,
    tile_size,
    db_path,
} => {
    // Validate input options
    match (input, input_dir) {
        (Some(_), Some(_)) => {
            anyhow::bail!("Cannot use both --input and --input-dir");
        }
        (None, None) => {
            anyhow::bail!("Must provide either --input or --input-dir");
        }
        _ => {}
    }

    if let Some(input_file) = input {
        // Single-PBF mode (existing behavior)
        use crate::rmdf::generator::TileGenerator;

        info!(
            "Generating tiles from {:?} to {:?} (tile_size={}°)",
            input_file, output, tile_size
        );

        let generator = TileGenerator::new(input_file.clone(), output.clone(), *tile_size)?;
        generator.generate()?;

        info!("Tile generation complete");
    } else if let Some(input_dir) = input_dir {
        // Multi-PBF mode (new behavior)
        use crate::rmdf::generator::MultiPbfGenerator;

        info!(
            "Generating tiles from directory {:?} to {:?} (tile_size={}°)",
            input_dir, output, tile_size
        );

        // Use provided db_path or default to temp location
        let db_path = db_path.unwrap_or_else(|| output.join(".intermediate.redb"));

        let generator = MultiPbfGenerator::new(
            input_dir.clone(),
            output.clone(),
            *tile_size,
            db_path,
        );
        generator.generate()?;

        info!("Multi-PBF tile generation complete");
    }

    Ok(())
}
```

### 4. Update MultiPbfGenerator Constructor

**File**: `src/rmdf/generator/mod.rs`

**Changes**: Ensure `MultiPbfGenerator::new()` accepts db_path parameter:

```rust
impl MultiPbfGenerator {
    pub fn new(
        input_directory: PathBuf,
        output_dir: PathBuf,
        tile_size_degrees: f32,
        db_path: PathBuf,
    ) -> Self {
        Self {
            input_directory,
            output_dir,
            tile_size_degrees,
            db_path,
        }
    }
}
```

### 5. Update Help Text

The help text should clearly distinguish the two modes:

```rust
/// Generate RMDF tiles from OSM PBF data
/// 
/// Single file mode: --input FILE
/// Multi-file mode: --input-dir DIR
GenerateTiles {
    // ...
}
```

## Success Criteria

### Automated Verification:
 [x] `--input file.pbf` works (existing single-PBF mode unchanged)
 [x] `--input-dir ./dir` works (new multi-PBF mode)
 [x] Using both `--input` and `--input-dir` returns error
 [x] Using neither returns error
 [x] `--help` shows new options correctly

### Manual Verification:
 [x] Single PBF mode produces same output as before
 [x] Multi-PBF mode processes all PBF files in directory
 [x] Error messages are clear and helpful
 [x] Help text is accurate

## Dependencies

- Depends on: Phase 4 (Final Tile Writing)
- Blocks: None - final phase

## Risks & Mitigations

- **Risk**: Breaking existing single-PBF workflows
  - **Mitigation**: Extensive testing with single-PBF mode, keep existing `TileGenerator` path unchanged

- **Risk**: Confusion about which mode to use
  - **Mitigation**: Clear help text, error messages, and documentation

## Notes

The `--db-path` option is optional. If not provided, the intermediate database is stored in the output directory with a hidden filename (`.intermediate.redb`). This file is cleaned up after successful completion.

For debugging purposes, users can specify `--db-path ./debug.redb` to keep the intermediate database for inspection.

## Example Usage

Single-PBF (existing):
```bash
ridi-router generate-tiles --input germany.osm.pbf --output ./tiles
```

Multi-PBF (new):
```bash
ridi-router generate-tiles --input-dir ./map-data/input --output ./map-data/output
```

Multi-PBF with custom db path:
```bash
ridi-router generate-tiles --input-dir ./input --output ./output --db-path ./debug.redb
```
