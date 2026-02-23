---
type: feature
priority: medium
created: 2026-02-22T12:00:00Z
status: implemented
tags: [rmdf, debug, viewer, web, leaflet, react, vite]
keywords: [rmdf-viewer, manifest.json, MappedTile, tiny_http, include_directory, Leaflet, React, Vite]
patterns: [memory mapping, http server, spa bundling, clap subcommand, geo visualization]
---

# FEATURE-001: RMDF Debug Viewer

## Description

Build a new `ridi-router rmdf-viewer --input-dir <DIR>` command that starts a local HTTP server at localhost:1337 serving a React single-page application for visualizing RMDF tile files on a Leaflet map.

The viewer allows users to browse available tiles from a `manifest.json`, load tiles on-demand via memory mapping, and visualize all elements (points, lines) with interactive popups showing element details.

## Context

The ridi-router project generates RMDF (Ridgeline Map Data Format) tiles containing routing data. Developers need a visual debugging tool to inspect tile contents, verify data integrity, and understand geographic coverage.

Previous debug-viewer implementation (removed in commit 313bc45) used:
- `tiny_http` for HTTP server
- `include_directory!` macro for embedding Vite-built assets
- Vite 6, Tailwind 4, maplibre-gl, vanjs-core

This new implementation will use React (per spec) with Leaflet instead of maplibre-gl.

## Requirements

### Functional Requirements

#### CLI Command
- [ ] New subcommand `ridi-router rmdf-viewer --input-dir <DIR>`
- [ ] HTTP server listening on `127.0.0.1:1337`
- [ ] Server logs startup message with URL
- [ ] Graceful shutdown on SIGINT/SIGTERM

#### Backend API
- [ ] `GET /api/manifest` - Returns manifest.json contents from input-dir
- [ ] `GET /api/tiles/<filename>` - Returns tile data (points, lines, tag sets) for specified RMDF file
  - Memory-map the file on demand (do not pre-load all files)
  - Return JSON with all elements for frontend rendering
- [ ] Static file serving for SPA assets (bundled in binary)
- [ ] SPA fallback: serve index.html for non-API routes

#### Frontend - Tile Selection
- [ ] Read `manifest.json` on startup
- [ ] Display list of available tiles from manifest
- [ ] Draw tile borders on map using `TileBounds` from manifest
- [ ] Allow loading/unloading tiles on demand
- [ ] Support loading multiple tiles simultaneously
- [ ] Toggle visibility checkbox per loaded tile
- [ ] Display tile header stats (point_count, line_count, size_bytes)

#### Frontend - Map Visualization
- [ ] Leaflet map with OpenStreetMap base layer
- [ ] Display all points from loaded tile(s) as markers/circles
- [ ] Display all lines from loaded tile(s) as polylines
- [ ] Color coding by element flags:
  - Default: neutral color
  - `residential_in_proximity` flag: yellow/orange
  - `nogo_area` flag: red
- [ ] Direction arrows on one-way lines (direction=1)
- [ ] Highlight elements on hover
- [ ] Click handler with Leaflet popup showing element details

#### Frontend - Element Popups
- [ ] Point popup content:
  - OSM ID
  - Type: "Point"
  - Coordinates (lat, lon)
  - Flags: residential_in_proximity, nogo_area (if set)
  - Connected lines count
- [ ] Line popup content:
  - Point A/B OSM IDs
  - Type: "Line"
  - Direction: BothWays / OneWay / Roundabout
  - Tag set info (highway, surface, smoothness, name if available)

#### Frontend - UI Components
- [ ] Simple legend explaining color coding (flag meanings)
- [ ] Tile list panel with load/visibility toggles
- [ ] Stats panel showing loaded tile metadata

### Non-Functional Requirements

- [ ] Assets bundled into binary using `include_directory!` pattern
- [ ] Vite build for React SPA
- [ ] Memory-efficient: only load tiles on demand
- [ ] Desktop-only (no mobile responsive required)
- [ ] No authentication required (localhost only)
- [ ] No CORS needed (same-origin)

## Current State

- RMDF format defined in `src/rmdf/format.rs`
- Memory-mapped tile loading exists in `src/rmdf/io.rs` (`MappedTile` struct)
- Manifest generation exists in `src/rmdf/generator/manifest.rs`
- Previous debug-viewer code available in git history (commit 313bc45 removed it)

## Desired State

A fully functional debug viewer allowing:
1. Browse available RMDF tiles from a directory
2. Load tiles on-demand to inspect contents
3. Visualize points and lines on an interactive map
4. Click elements to see detailed information
5. Toggle visibility of multiple loaded tiles

## Research Context

### Keywords to Search

- `manifest.json` - Tile manifest file containing tile list and bounds
- `MappedTile` - Memory-mapped tile loader in `src/rmdf/io.rs`
- `RmdfHeader` - Binary format header with section offsets
- `PointRecord` - Point element struct with flags
- `LineRecord` - Line element struct with direction
- `TagSetRecord` - Tag data (highway, surface, etc.)
- `tiny_http` - Lightweight HTTP server library (previous pattern)
- `include_directory!` - Macro for embedding assets in binary
- `TileManifest` / `TileMetadata` / `TileBounds` - Manifest structs

### Patterns to Investigate

1. **Vite + React Setup**
   - Look at git commit `7ac1529` for previous Vite UI implementation
   - Check `package.json` for frontend dependencies
   - Build output goes to `src/debug/viewer/ui/dist`

2. **Asset Bundling**
   - Use `include_directory!` macro from `include-directory` crate
   - Embed built assets at compile time
   - Pattern: `static DIST_DIR: Dir = include_directory!("$CARGO_MANIFEST_DIR/src/debug/viewer/ui/dist");`

3. **HTTP Server**
   - Use `tiny_http` crate (previous pattern)
   - SPA fallback: serve index.html for non-API, non-static routes
   - Handle API routes with JSON responses

4. **Memory Mapping**
   - Use `memmap2::Mmap` for zero-copy file access
   - `MappedTile::load()` pattern from `src/rmdf/io.rs`
   - Transmute bytes to structs using `bytemuck`

5. **RMDF Reading**
   - `MappedTile::get_points()` - Returns `&[PointRecord]`
   - `MappedTile::get_lines()` - Returns `&[LineRecord]`
   - `MappedTile::get_tag_sets()` - Returns `&[TagSetRecord]`

### Key Decisions Made

1. **React over VanJS** - User specified React despite previous implementation using vanjs-core
2. **Leaflet over MapLibre** - User specified Leaflet for simpler integration
3. **Memory mapping on demand** - Load files only when user selects them
4. **Multiple tiles allowed** - Can load and overlay multiple tiles
5. **Color by flags** - Elements colored based on `residential_in_proximity` and `nogo_area` flags
6. **Desktop only** - No mobile responsive design required
7. **No export** - Viewer only, no data export functionality
8. **No URL state** - No shareable URLs with lat/lng/zoom

### Manifest JSON Format

Located at `<input-dir>/manifest.json`:

```json
{
  "version": "x.x.x",
  "tile_size_degrees": 1.0,
  "format_version": 1,
  "generated_at": "2025-01-15T10:30:00Z",
  "source_files": ["source.osm.pbf"],
  "tiles": [
    {
      "filename": "tile_123_456.rmdf",
      "col": 123,
      "row": 456,
      "bounds": {
        "lat_min": -33.0,
        "lat_max": -32.0,
        "lon_min": 150.0,
        "lon_max": 151.0
      },
      "neighbors": {
        "north": "tile_123_457.rmdf",
        "south": "tile_123_455.rmdf",
        "east": "tile_124_456.rmdf",
        "west": "tile_122_456.rmdf",
        "northeast": "tile_124_457.rmdf",
        "northwest": "tile_122_457.rmdf",
        "southeast": "tile_124_455.rmdf",
        "southwest": "tile_122_455.rmdf"
      },
      "size_bytes": 12345678,
      "point_count": 1000,
      "line_count": 500,
      "checksum": "sha256:..."
    }
  ]
}
```

### RMDF Data Structures

```rust
// From src/rmdf/format.rs
pub struct PointRecord {
    pub osm_id: u64,
    pub lat: f32,
    pub lon: f32,
    pub lines_offset: u64,
    pub lines_count: u32,
    pub rules_offset: u64,
    pub rules_count: u32,
    pub flags: u16,  // bit 0: residential_in_proximity, bit 1: nogo_area
}

pub struct LineRecord {
    pub point_a_osm_id: u64,
    pub point_a_lat: f32,
    pub point_a_lon: f32,
    pub point_b_osm_id: u64,
    pub point_b_lat: f32,
    pub point_b_lon: f32,
    pub direction: u8,  // 0=BothWays, 1=OneWay, 2=Roundabout
    pub tag_set_index: u32,
}

pub struct TagSetRecord {
    pub name_idx: u32,
    pub hw_ref_idx: u32,
    pub highway_idx: u32,
    pub surface_idx: u32,
    pub smoothness_idx: u32,
}
```

## Success Criteria

### Automated Verification

- [ ] `cargo build --features rmdf-viewer` compiles without errors
- [ ] `ridi-router rmdf-viewer --input-dir <test-dir>` starts server without panic
- [ ] `curl http://127.0.0.1:1337/api/manifest` returns valid JSON
- [ ] `curl http://127.0.0.1:1337/api/tiles/tile_0_0.rmdf` returns valid JSON with points/lines
- [ ] `curl http://127.0.0.1:1337/` returns HTML (SPA entry point)
- [ ] Frontend unit tests pass

### Manual Verification

- [ ] Open http://127.0.0.1:1337 in browser
- [ ] Tile list shows all tiles from manifest
- [ ] Tile borders are drawn on map
- [ ] Clicking a tile loads it and displays points/lines
- [ ] Multiple tiles can be loaded
- [ ] Toggle visibility works for each tile
- [ ] Tile stats (point/line counts) displayed correctly
- [ ] Elements colored by flags correctly
- [ ] One-way lines show direction arrows
- [ ] Hover highlights elements
- [ ] Click on point shows popup with correct info
- [ ] Click on line shows popup with correct info
- [ ] Legend visible and accurate
- [ ] Server shuts down cleanly on Ctrl+C

## Related Information

- Git commit `313bc45` - Removed previous debug-viewer
- Git commit `7ac1529` - Added Vite UI implementation
- `src/rmdf/format.rs` - RMDF binary format definitions
- `src/rmdf/io.rs` - Memory-mapped tile loading
- `src/rmdf/generator/manifest.rs` - Manifest generation
- `src/rmdf/tile_manager.rs` - Tile management with LRU eviction

## Implementation Notes

### File Structure

```
src/
  debug/
    rmdf_viewer/
      mod.rs           # CLI command + HTTP server
      api.rs           # API handlers (/api/manifest, /api/tiles/:filename)
      ui/              # Vite React app
        package.json
        vite.config.ts
        index.html
        src/
          main.tsx
          App.tsx
          components/
            TileList.tsx
            MapView.tsx
            Legend.tsx
            ElementPopup.tsx
          api/
            client.ts
          types/
            manifest.ts
            tile.ts
```

### Dependencies

Backend (Cargo.toml):
- `tiny_http` - HTTP server
- `include-directory` - Asset bundling
- `memmap2` - Memory mapping
- `serde_json` - JSON serialization
- `clap` - CLI (already present)

Frontend (package.json):
- `react` + `react-dom`
- `leaflet` + `react-leaflet`
- `vite` + `@vitejs/plugin-react`
- `typescript`

### API Response Format

`GET /api/manifest`:
```json
{
  "version": "0.1.0",
  "tiles": [
    {
      "filename": "tile_0_0.rmdf",
      "bounds": { "lat_min": -90, "lat_max": -89, "lon_min": 0, "lon_max": 1 },
      "point_count": 1000,
      "line_count": 500,
      "size_bytes": 12345
    }
  ]
}
```

`GET /api/tiles/tile_0_0.rmdf`:
```json
{
  "filename": "tile_0_0.rmdf",
  "header": {
    "point_count": 1000,
    "line_count": 500
  },
  "points": [
    {
      "osm_id": 123456,
      "lat": -89.5,
      "lon": 0.5,
      "flags": ["residential_in_proximity"]
    }
  ],
  "lines": [
    {
      "point_a_osm_id": 123456,
      "point_b_osm_id": 789012,
      "point_a": [-89.5, 0.5],
      "point_b": [-89.4, 0.6],
      "direction": "OneWay",
      "tags": {
        "highway": "residential",
        "surface": "asphalt"
      }
    }
  ]
}
```

## Open Questions

None - scope fully defined through exploration.
