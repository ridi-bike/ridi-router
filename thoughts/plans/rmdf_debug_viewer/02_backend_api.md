# Phase 2: Backend API

## Overview

Implement the API endpoints for serving manifest data and tile contents. The `/api/manifest` endpoint returns the manifest.json contents, and `/api/tiles/:filename` loads an RMDF tile via memory mapping and returns all points, lines, and tag data as JSON.

## Changes Required

### 1. Create API Response Types

**File**: `src/debug/rmdf_viewer/api.rs`

**Changes**: Define response structures with serde serialization and TypeScript export

```rust
use anyhow::{Context, Result};
use serde::Serialize;
use std::path::PathBuf;
use ts_rs::TS;

use crate::rmdf::generator::manifest::{TileManifest, TileMetadata};
use crate::rmdf::MappedTile;

// Configure TypeScript export path for all types in this module
const TS_EXPORT_DIR: &str = "../ui/src/types/generated";

/// Response for GET /api/manifest
#[derive(Serialize, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct ManifestResponse {
    pub version: String,
    pub tile_size_degrees: f32,
    pub format_version: u32,
    pub generated_at: String,
    pub tiles: Vec<TileSummary>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct TileSummary {
    pub filename: String,
    pub col: u16,
    pub row: u16,
    pub bounds: TileBoundsResponse,
    pub size_bytes: u64,
    pub point_count: u64,
    pub line_count: u64,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct TileBoundsResponse {
    pub lat_min: f32,
    pub lat_max: f32,
    pub lon_min: f32,
    pub lon_max: f32,
}

/// Response for GET /api/tiles/:filename
#[derive(Serialize, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct TileResponse {
    pub filename: String,
    pub header: TileHeader,
    pub points: Vec<PointResponse>,
    pub lines: Vec<LineResponse>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct TileHeader {
    pub point_count: u64,
    pub line_count: u64,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct PointResponse {
    pub osm_id: u64,
    pub lat: f32,
    pub lon: f32,
    pub flags: Vec<String>,
    pub connected_lines_count: u32,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct LineResponse {
    pub point_a_osm_id: u64,
    pub point_b_osm_id: u64,
    pub point_a: [f32; 2],  // [lat, lon]
    pub point_b: [f32; 2],  // [lat, lon]
    pub direction: String,
    pub tags: TagResponse,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = TS_EXPORT_DIR)]
pub struct TagResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub highway: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub surface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub smoothness: Option<String>,
}

// Implementation functions...
```

**Rationale**: These types define the JSON contract between backend and frontend. Tag values are pre-resolved on the backend (per research decision #1).

### 2. Implement Manifest Endpoint

**File**: `src/debug/rmdf_viewer/api.rs`

**Changes**: Add `get_manifest()` function

```rust
pub fn get_manifest(input_dir: &PathBuf) -> Result<ManifestResponse> {
    let manifest_path = input_dir.join("manifest.json");
    
    let file = std::fs::File::open(&manifest_path)
        .with_context(|| format!("Failed to open manifest at {:?}", manifest_path))?;
    
    let manifest: TileManifest = serde_json::from_reader(file)
        .context("Failed to parse manifest.json")?;
    
    let response = ManifestResponse {
        version: manifest.version,
        tile_size_degrees: manifest.tile_size_degrees,
        format_version: manifest.format_version,
        generated_at: manifest.generated_at,
        tiles: manifest.tiles.into_iter().map(|t| TileSummary {
            filename: t.filename,
            col: t.col,
            row: t.row,
            bounds: TileBoundsResponse {
                lat_min: t.bounds.lat_min,
                lat_max: t.bounds.lat_max,
                lon_min: t.bounds.lon_min,
                lon_max: t.bounds.lon_max,
            },
            size_bytes: t.size_bytes,
            point_count: t.point_count,
            line_count: t.line_count,
        }).collect(),
    };
    
    Ok(response)
}
```

**Rationale**: Uses the existing `TileManifest` struct from `src/rmdf/generator/manifest.rs`. Transforms to response type to control the API contract independently.

### 3. Implement Tile Endpoint

**File**: `src/debug/rmdf_viewer/api.rs`

**Changes**: Add `get_tile()` function

```rust
pub fn get_tile(input_dir: &PathBuf, filename: &str) -> Result<TileResponse> {
    // Security: Validate filename doesn't contain path traversal
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        anyhow::bail!("Invalid filename");
    }
    
    // Security: Must be .rmdf file
    if !filename.ends_with(".rmdf") {
        anyhow::bail!("Filename must end with .rmdf");
    }
    
    let tile_path = input_dir.join(filename);
    let mapped_tile = MappedTile::load(&tile_path)
        .with_context(|| format!("Failed to load tile: {:?}", tile_path))?;
    
    let header = mapped_tile.header;
    
    // Build points response
    let points: Vec<PointResponse> = mapped_tile
        .get_points()?
        .iter()
        .map(|p| {
            let mut flags = Vec::new();
            if p.residential_in_proximity() {
                flags.push("residential_in_proximity".to_string());
            }
            if p.nogo_area() {
                flags.push("nogo_area".to_string());
            }
            PointResponse {
                osm_id: p.osm_id,
                lat: p.lat,
                lon: p.lon,
                flags,
                connected_lines_count: p.lines_count,
            }
        })
        .collect();
    
    // Build lines response
    let lines: Vec<LineResponse> = mapped_tile
        .get_lines()?
        .iter()
        .map(|l| {
            let direction = match l.direction {
                0 => "BothWays",
                1 => "OneWay",
                2 => "Roundabout",
                _ => "Unknown",
            };
            
            // Resolve tags
            let tags = resolve_tags(&mapped_tile, l.tag_set_index).unwrap_or_default();
            
            LineResponse {
                point_a_osm_id: l.point_a_osm_id,
                point_b_osm_id: l.point_b_osm_id,
                point_a: [l.point_a_lat, l.point_a_lon],
                point_b: [l.point_b_lat, l.point_b_lon],
                direction: direction.to_string(),
                tags,
            }
        })
        .collect();
    
    Ok(TileResponse {
        filename: filename.to_string(),
        header: TileHeader {
            point_count: header.point_count,
            line_count: header.line_count,
        },
        points,
        lines,
    })
}

fn resolve_tags(tile: &MappedTile, tag_set_index: u32) -> Result<TagResponse> {
    let tag_set = tile.get_tag_set(tag_set_index)?;
    
    let resolve = |idx: u32| -> Option<String> {
        if idx == crate::rmdf::format::TagSetRecord::NONE {
            return None;
        }
        tile.get_tag_value(idx).ok()
    };
    
    Ok(TagResponse {
        name: resolve(tag_set.name_idx),
        highway: resolve(tag_set.highway_idx),
        surface: resolve(tag_set.surface_idx),
        smoothness: resolve(tag_set.smoothness_idx),
    })
}
```

**Rationale**: 
- Memory-maps the tile on demand (no pre-loading)
- Pre-resolves tag values on backend (simplifies frontend)
- Includes all fields needed for visualization and popups
- Security: validates filename to prevent path traversal

### 4. Update mod.rs to Use API Functions

**File**: `src/debug/rmdf_viewer/mod.rs`

**Changes**: Replace placeholder responses with actual implementations

```rust
fn handle_api_request(
    request: &Request, 
    input_dir: &PathBuf
) -> Result<Response<std::io::Cursor<Vec<u8>>>, RmdfViewerError> {
    let url = request.url();

    if url == "/api/manifest" {
        match api::get_manifest(input_dir) {
            Ok(manifest) => {
                let json = serde_json::to_string(&manifest)
                    .map_err(|e| RmdfViewerError::Serialize(e))?;
                Ok(Response::from_string(json)
                    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                        .map_err(|_| RmdfViewerError::HeaderCreate)?))
            }
            Err(e) => {
                let error_json = format!("{{\"error\": \"{}\"}}", e);
                Ok(Response::from_string(error_json)
                    .with_status_code(500)
                    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                        .map_err(|_| RmdfViewerError::HeaderCreate)?))
            }
        }
    } else if let Some(filename) = url.strip_prefix("/api/tiles/") {
        match api::get_tile(input_dir, filename) {
            Ok(tile) => {
                let json = serde_json::to_string(&tile)
                    .map_err(|e| RmdfViewerError::Serialize(e))?;
                Ok(Response::from_string(json)
                    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                        .map_err(|_| RmdfViewerError::HeaderCreate)?))
            }
            Err(e) => {
                // Check if file not found vs other errors
                let status = if e.to_string().contains("Failed to open") {
                    404
                } else {
                    500
                };
                let error_json = format!("{{\"error\": \"{}\"}}", e);
                Ok(Response::from_string(error_json)
                    .with_status_code(status)
                    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                        .map_err(|_| RmdfViewerError::HeaderCreate)?))
            }
        }
    } else {
        let response = Response::from_string("{\"error\": \"Not found\"}")
            .with_status_code(404)
            .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                .map_err(|_| RmdfViewerError::HeaderCreate)?);
        Ok(response)
    }
}
```

**Rationale**: Proper error handling with appropriate HTTP status codes:
- 200: Success
- 404: Tile file not found
- 500: Parse errors, corrupt files

### 5. Add Serialize Error Variant

**File**: `src/debug/rmdf_viewer/mod.rs`

**Changes**: Add error variant for serialization

```rust
#[derive(Debug, thiserror::Error)]
pub enum RmdfViewerError {
    // ... existing variants ...
    
    #[error("Serialization error: {0}")]
    Serialize(#[source] serde_json::Error),
}
```

## Success Criteria

### Automated Verification:
- [ ] `cargo build --features rmdf-viewer` compiles without errors
- [ ] `cargo test --features rmdf-viewer` passes all tests

### Manual Verification:
- [ ] `curl http://127.0.0.1:1337/api/manifest` returns valid JSON with tiles array
- [ ] Manifest response includes: version, tiles[*].filename, tiles[*].bounds, tiles[*].point_count
- [ ] `curl http://127.0.0.1:1337/api/tiles/tile_0_0.rmdf` returns valid JSON
- [ ] Tile response includes: header.point_count, points array, lines array
- [ ] Points have: osm_id, lat, lon, flags, connected_lines_count
- [ ] Lines have: point_a_osm_id, point_b_osm_id, point_a, point_b, direction, tags
- [ ] Tags are resolved to strings (not indices)
- [ ] Missing tile returns 404 with JSON error
- [ ] Invalid filename (path traversal) returns error

## Dependencies

- Depends on: Phase 1 (Backend Foundation)
- Blocks: Phase 3 (Frontend Scaffolding)

## Risks & Mitigations

- **Risk**: Large tiles may produce large JSON responses
  - **Mitigation**: Acceptable for debug tool; typical tiles are <50MB
  
- **Risk**: Memory mapping may fail on corrupt files
  - **Mitigation**: Return 500 error with descriptive message

## Out of Scope for This Phase

**CRITICAL**: The following items are explicitly NOT part of this phase:
- Static file serving (SPA) - Phase 7
- React frontend - Phases 3-6
- Pagination or streaming for large tiles
- Caching of loaded tiles
- WebSocket for real-time updates

## Phase Boundary Rules

**IMPORTANT**: When executing this phase:
1. **Focus on API endpoints only** - Do not add static file serving yet
2. **Use existing MappedTile** - Do not modify src/rmdf/io.rs
3. **Pre-resolve all tags** - Backend returns strings, not indices

## Notes

- The `resolve_tags()` function handles the case where tag indices are `0xFFFFFFFF` (NONE) by returning `None`.
- Direction is encoded as: 0=BothWays, 1=OneWay, 2=Roundabout (from src/rmdf/format.rs:121)
- Points include `connected_lines_count` derived from `lines_count` field (for popup display)
