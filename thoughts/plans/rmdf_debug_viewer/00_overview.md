# RMDF Debug Viewer Implementation Plan

## Overview

Build a new `ridi-router rmdf-viewer --input-dir <DIR>` command that starts a local HTTP server at localhost:1337 serving a React single-page application for visualizing RMDF tile files on a Leaflet map. The viewer allows users to browse available tiles from a `manifest.json`, load tiles on-demand via memory mapping, and visualize all elements (points, lines) with interactive popups showing element details.

## Current State Analysis

The ridi-router project already has the core infrastructure needed for this feature:

### Existing Components:
- **RMDF Format** (`src/rmdf/format.rs:43-238`): Binary format with `PointRecord`, `LineRecord`, `TagSetRecord`, `RmdfHeader`
- **Memory-Mapped Loading** (`src/rmdf/io.rs:9-160`): `MappedTile` with `get_points()`, `get_lines()`, `get_tag_value()` methods
- **Manifest Structure** (`src/rmdf/generator/manifest.rs:10-51`): `TileManifest`, `TileMetadata`, `TileBounds` with serde support
- **CLI Pattern** (`src/router_runner.rs:157-217`): `CliMode` enum with feature-gated subcommands
- **Dependencies**: `include_directory`, `serde_json`, `clap`, `memmap2` already in Cargo.toml

### Previous Implementation (removed at commit 313bc45):
- Used `tiny_http` for HTTP server
- Used `include_directory!` macro for embedding built assets
- SPA fallback pattern for non-API routes
- Feature-gated in Cargo.toml

### What's Missing:
1. `tiny_http` dependency (needs to be added as optional)
2. `ts-rs` dependency for TypeScript type generation (needs to be added as optional)
3. `rmdf-viewer` feature flag
4. `src/debug/rmdf_viewer/` module with HTTP server and API handlers
5. `CliMode::RmdfViewer` variant
6. React frontend with Leaflet visualization

## Desired End State

A fully functional debug viewer allowing:
1. Run `ridi-router rmdf-viewer --input-dir <DIR>` to start server at http://127.0.0.1:1337
2. Browse available RMDF tiles from manifest.json
3. Load tiles on-demand to inspect contents
4. Visualize points and lines on an interactive Leaflet map
5. Click elements to see detailed information in popups
6. Toggle visibility of multiple loaded tiles

### Verification:
```bash
# Backend verification
cargo build --features rmdf-viewer
ridi-router rmdf-viewer --input-dir ./test-tiles &
curl http://127.0.0.1:1337/api/manifest | jq .
curl http://127.0.0.1:1337/api/tiles/tile_0_0.rmdf | jq '.points | length'
curl http://127.0.0.1:1337/  # Returns HTML

# Frontend verification (manual)
# Open http://127.0.0.1:1337 in browser
# Verify tile list appears
# Click a tile to load it
# Verify points/lines appear on map
# Click a point/line to see popup
```

## What We're NOT Doing

- Mobile responsive design (desktop-only)
- Authentication (localhost only)
- CORS handling (same-origin)
- URL state persistence (no shareable URLs)
- Data export functionality
- Tile unloading/eviction (frontend keeps all loaded tiles in memory)
- LRU caching on backend (on-demand loading only)

## Implementation Approach

**Backend-first with incremental frontend:**
1. Build HTTP server and CLI integration
2. Implement API endpoints
3. Scaffold frontend with Vite
4. Build tile management UI
5. Implement map visualization
6. Add interactivity (popups, highlighting)
7. Bundle assets into binary

This approach allows:
- Backend can be tested independently via curl
- Frontend development against working API
- Incremental feature delivery

## Implementation Phases

1. **Phase 1: Backend Foundation** - HTTP server, CLI integration, basic routing
   - See: `01_backend_foundation.md`

2. **Phase 2: Backend API** - /api/manifest and /api/tiles endpoints
   - See: `02_backend_api.md`

3. **Phase 3: Frontend Scaffolding** - Vite project, basic structure, API client
   - See: `03_frontend_scaffolding.md`

4. **Phase 4: Frontend Tile Management** - Tile list, loading, borders
   - See: `04_frontend_tile_management.md`

5. **Phase 5: Frontend Map Visualization** - Points, lines, color coding
   - See: `05_frontend_map_visualization.md`

6. **Phase 6: Frontend Interactivity** - Popups, highlighting, legend
   - See: `06_frontend_interactivity.md`

7. **Phase 7: Asset Bundling & Integration** - include_directory!, final build
   - See: `07_asset_bundling.md`

## Testing Strategy

### Unit Tests:
- API response serialization
- Tile data transformation
- Error handling cases

### Integration Tests:
- HTTP server request/response cycle
- API endpoint responses with real tiles
- Frontend API client calls

### Manual Testing Steps:
1. Start server: `ridi-router rmdf-viewer --input-dir <test-dir>`
2. Open http://127.0.0.1:1337 in browser
3. Verify tile list shows all tiles from manifest
4. Click tile border to load it
5. Verify points appear as circle markers
6. Verify lines appear as polylines
7. Click point → verify popup shows OSM ID, coords, flags
8. Click line → verify popup shows endpoints, direction, tags
9. Verify color coding: default (blue), residential_in_proximity (orange), nogo_area (red)
10. Verify one-way lines show direction arrows
11. Toggle tile visibility → verify elements hide/show
12. Load multiple tiles → verify all render
13. Ctrl+C → verify server shuts down cleanly

## Performance Considerations

- **Memory-mapped tiles**: Zero-copy file access, only load when requested
- **Frontend state**: All loaded tiles kept in memory (acceptable for debug tool)
- **No pagination**: Return full tile contents (tiles are typically <50MB)
- **Leaflet performance**: Use `L.LayerGroup` for efficient batch operations

## Migration Notes

No migration needed - this is a new feature. The previous debug-viewer was fully removed in commit 313bc45.

## References

- Original ticket: `thoughts/tickets/feature_rmdf_debug_viewer.md`
- Research document: `thoughts/research/2026-02-22_rmdf_debug_viewer.md`
- RMDF format: `src/rmdf/format.rs`
- Memory mapping: `src/rmdf/io.rs`
- Manifest structure: `src/rmdf/generator/manifest.rs`
- CLI pattern: `src/router_runner.rs:157-217`
- Previous implementation: git commit `313bc45^:src/debug/viewer/mod.rs`
- ts-rs crate: https://crates.io/crates/ts-rs (TypeScript type generation from Rust)
