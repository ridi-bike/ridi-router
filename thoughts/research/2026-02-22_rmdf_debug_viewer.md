---
date: 2026-02-22T13:30:00+02:00
git_commit: a9824f1efcae1bf6afbe2af82a43e14bd80e15ef
branch: feat-ridi-map-format
repository: git@github.com:tomsjansons/ridi-router.git
topic: "RMDF Debug Viewer Implementation Research"
tags: [research, codebase, rmdf, debug-viewer, http-server, react, leaflet, vite]
last_updated: 2026-02-22T13:30:00+02:00
---

## Ticket Synopsis

Build a new `ridi-router rmdf-viewer --input-dir <DIR>` command that starts a local HTTP server at localhost:1337 serving a React single-page application for visualizing RMDF tile files on a Leaflet map. The viewer allows users to browse available tiles from a `manifest.json`, load tiles on-demand via memory mapping, and visualize all elements (points, lines) with interactive popups showing element details.

## Summary

This research analyzes the codebase to determine how to implement the RMDF Debug Viewer feature. Key findings:

1. **RMDF Format**: Well-defined binary format in `src/rmdf/format.rs` with memory-mapped access via `MappedTile` in `src/rmdf/io.rs`
2. **Manifest Loading**: `TileManifest` structure in `src/rmdf/generator/manifest.rs`, loaded via `serde_json` in `TileManager`
3. **Previous Implementation**: Removed debug-viewer at commit 313bc45 used `tiny_http`, `include_directory!`, Vite+VanJS+MapLibre - provides a template pattern
4. **CLI Pattern**: Feature-gated subcommands using clap derive with `#[cfg(feature = "...")]`
5. **API Design**: New viewer needs `/api/manifest` and `/api/tiles/<filename>` endpoints returning JSON

## Detailed Findings

### RMDF Format Definitions

Located in `src/rmdf/format.rs`:

- **RmdfHeader** (lines 43-77): Main file header with magic, version, bounds, counts, section offsets
- **PointRecord** (lines 85-113): Point data with OSM ID, coordinates, line/rule references, flags (56 bytes)
- **LineRecord** (lines 114-138): Line segment with endpoints, direction, tag set index (48 bytes)
- **TagSetRecord** (lines 139-153): Tag set with name, highway, surface, smoothness indices (20 bytes)
- **TileId** (lines 17-31): Tile coordinates (col, row) with `to_filename()` helper
- **TileBounds** (lines 33-41): Geographic bounds (lat/lon min/max)

**Flag Encodings**:
- `PointRecord.flags` (u16): bit 0 = `residential_in_proximity`, bit 1 = `nogo_area`
- `LineRecord.direction` (u8): 0 = BothWays, 1 = OneWay, 2 = Roundabout

### Memory-Mapped Tile Loading

Located in `src/rmdf/io.rs`:

```rust
pub struct MappedTile {
    _file: File,
    mmap: Mmap,
    pub header: &'static RmdfHeader,
    pub tile_id: TileId,
}
```

**Key Methods**:
- `load(path)` - Opens file, creates memmap2::Mmap, casts header with bytemuck (lines 16-55)
- `get_points()` - Returns `&[PointRecord]` using section_offsets (lines 87-96)
- `get_lines()` - Returns `&[LineRecord]` using section_offsets (lines 99-108)
- `get_tag_set(index)` - Returns single `&TagSetRecord` by index (lines 126-137)
- `get_tag_value(idx)` - Resolves string index to actual value (lines 140-165)

### Manifest Structure

Located in `src/rmdf/generator/manifest.rs`:

```rust
pub struct TileManifest {
    pub version: String,
    pub tile_size_degrees: f32,
    pub format_version: u32,
    pub generated_at: String,
    pub source_files: Vec<String>,
    pub tiles: Vec<TileMetadata>,
}

pub struct TileMetadata {
    pub filename: String,
    pub col: u16,
    pub row: u16,
    pub bounds: TileBounds,
    pub neighbors: TileNeighbors,
    pub size_bytes: u64,
    pub point_count: u64,
    pub line_count: u64,
    pub checksum: String,
}
```

**Loading Pattern** (from `src/rmdf/tile_manager.rs:23-41`):
```rust
let manifest_path = tile_dir.join("manifest.json");
let manifest_file = std::fs::File::open(&manifest_path)?;
let manifest: TileManifest = serde_json::from_reader(manifest_file)?;
```

### Previous Debug-Viewer Implementation

Removed at commit 313bc45, available in git history:

**Dependencies** (feature-gated in Cargo.toml):
```toml
debug-viewer = ["dep:duckdb", "dep:qstring", "dep:sql-builder", "dep:tiny_http"]
tiny_http = { version = "0.12.0", optional = true }
```

**HTTP Server Pattern** (from deleted `src/debug/viewer/mod.rs`):
```rust
use tiny_http::{Header, Method, Request, Response, Server};
use include_directory::{include_directory, Dir};

static DIST_DIR: Dir = include_directory!("$CARGO_MANIFEST_DIR/src/debug/viewer/ui/dist");

impl DebugViewer {
    pub fn run(debug_dir: PathBuf) -> Result<()> {
        let addr = "127.0.0.1:1337";
        let server = Server::http(addr)?;
        info!(addr, "Running Debug Viewer on http://{addr}");

        for request in server.incoming_requests() {
            if request.method() != &Method::Get {
                request.respond(Response::from_string("not allowed").with_status_code(405))?;
                continue;
            }

            // API routes
            if request.url().starts_with("/api/") {
                let response = Self::handle_api_request(&request)?;
                request.respond(response)?;
                continue;
            }

            // SPA fallback - serve static files
            let response = Self::handle_file_request(&request)?;
            request.respond(response)?;
        }
    }
}
```

**Static File Serving with SPA Fallback**:
```rust
fn handle_file_request(request: &Request) -> Result<Response<Cursor<Vec<u8>>>> {
    let mut file_name = request.url().to_string();
    
    // Security: strip path traversal
    loop {
        let len = file_name.len();
        file_name = file_name.replace("../", "").replace("./", "");
        if file_name.len() == len { break; }
    }
    
    // SPA fallback: serve index.html for root
    let file_name = if file_name == "" { "index.html" } else { &file_name[1..] };

    let file = DIST_DIR.get_file(file_name)?;
    let mime_type = file.mimetype().to_string();

    Ok(Response::from_string(file.contents_utf8().unwrap())
        .with_header(Header::from_bytes(&b"Content-Type"[..], mime_type.as_bytes())?))
}
```

### CLI Subcommand Pattern

Located in `src/router_runner.rs`:

**Step 1: Add feature in Cargo.toml**:
```toml
[features]
default = []
rmdf-viewer = ["dep:tiny_http", "dep:include-directory"]
```

**Step 2: Add variant in CliMode enum** (lines 173-216):
```rust
#[derive(Subcommand)]
enum CliMode {
    GenerateRoute { ... },
    GenerateTiles { ... },

    #[cfg(feature = "rmdf-viewer")]
    RmdfViewer {
        #[arg(long, value_name = "DIR")]
        input_dir: PathBuf,
    },
}
```

**Step 3: Add match arm in run()** (lines 458-492):
```rust
match &cli.mode {
    CliMode::GenerateRoute { ... } => { ... },
    CliMode::GenerateTiles { ... } => { ... },

    #[cfg(feature = "rmdf-viewer")]
    CliMode::RmdfViewer { input_dir } => {
        crate::debug::rmdf_viewer::run(input_dir)
    }
}
```

### API Response Design

**GET /api/manifest**:
```json
{
  "version": "0.1.0",
  "tile_size_degrees": 1.0,
  "format_version": 1,
  "generated_at": "2025-01-15T10:30:00Z",
  "tiles": [
    {
      "filename": "tile_204_146.rmdf",
      "col": 204,
      "row": 146,
      "bounds": { "lat_min": 56.0, "lat_max": 57.0, "lon_min": 24.0, "lon_max": 25.0 },
      "size_bytes": 12345678,
      "point_count": 1000,
      "line_count": 500
    }
  ]
}
```

**GET /api/tiles/tile_204_146.rmdf**:
```json
{
  "filename": "tile_204_146.rmdf",
  "header": {
    "point_count": 1000,
    "line_count": 500
  },
  "points": [
    {
      "osm_id": 123456,
      "lat": 56.5,
      "lon": 24.5,
      "flags": ["residential_in_proximity"],
      "connected_lines": [0, 5, 12]
    }
  ],
  "lines": [
    {
      "point_a_osm_id": 123456,
      "point_b_osm_id": 789012,
      "point_a": [56.5, 24.5],
      "point_b": [56.4, 24.6],
      "direction": "OneWay",
      "tags": {
        "highway": "residential",
        "surface": "asphalt"
      }
    }
  ]
}
```

## Code References

- `src/rmdf/format.rs:43-77` - RmdfHeader definition
- `src/rmdf/format.rs:85-113` - PointRecord definition with flags
- `src/rmdf/format.rs:114-138` - LineRecord definition with direction
- `src/rmdf/format.rs:139-153` - TagSetRecord definition
- `src/rmdf/io.rs:9-13` - MappedTile struct
- `src/rmdf/io.rs:16-55` - MappedTile::load() implementation
- `src/rmdf/io.rs:87-96` - get_points() method
- `src/rmdf/io.rs:99-108` - get_lines() method
- `src/rmdf/io.rs:126-137` - get_tag_set() method
- `src/rmdf/io.rs:140-165` - get_tag_value() method
- `src/rmdf/generator/manifest.rs:10-48` - TileManifest, TileMetadata, TileBounds structs
- `src/rmdf/tile_manager.rs:23-41` - Manifest loading pattern
- `src/router_runner.rs:55-62` - Cli struct definition
- `src/router_runner.rs:173-216` - CliMode enum with subcommands
- `src/router_runner.rs:458-492` - RouterRunner::run() dispatch
- `Cargo.toml:10-14` - Feature flags section

## Architecture Insights

### Zero-Copy Memory Mapping Pattern
The RMDF format uses `#[repr(C)]` structs with `bytemuck::Pod` and `bytemuck::Zeroable` derives for direct memory mapping. The `MappedTile` holds both `File` and `Mmap` to keep the mapping valid, with header cast to `'static` lifetime.

### Section-Based Layout
RMDF files use a section-based layout with offsets stored in `RmdfHeader.section_offsets[7]`. This allows:
- Direct access to any section via offset + count
- Zero-copy slice casting with `bytemuck::cast_slice`
- No deserialization overhead

### String Pool Resolution
Tag values are stored in a string pool section. The `TagSetRecord` contains indices (u32) into this pool. Resolution requires:
1. Get `TagSetRecord` via `get_tag_set(tag_set_index)`
2. Each `*_idx` field points to a `StringEntry` in the tag values section
3. `StringEntry` contains offset and length for the actual string

### Tile Naming Convention
Tiles are named `tile_{col}_{row}.rmdf` where:
- `col = floor(lon + 180)` (0-359)
- `row = floor(lat + 90)` (0-179)

Example: Riga (56.9°N, 24.1°E) → `tile_204_146.rmdf`

### Border Point Duplication
Points on tile borders are duplicated in adjacent tiles with the same OSM ID. This enables:
- Self-contained tiles (no cross-tile lookups for geometry)
- Border detection via coordinate comparison
- Adjacent tile loading on-demand for routing

## Historical Context

### Previous Debug-Viewer Architecture
The removed debug-viewer (commit 313bc45) used:
- **DuckDB** for in-memory SQL queries on debug data
- **VanJS** for reactive UI (now replaced with React per ticket spec)
- **MapLibre GL** for map rendering (now Leaflet per ticket spec)
- **Vite** for SPA bundling (same pattern to be used)

### RMDF Design Decisions
From `thoughts/plans/rmdf_memory_mapped_tiles/00_overview.md`:
- Replaced bincode 4-file cache with single RMDF file per tile
- Zero-copy access eliminates deserialization overhead
- Border point duplication enables self-contained tiles
- MAX_LOADED_TILES = 100 (conservative FD limit)
- LRU eviction is stub-only (TODO exists at `tile_manager.rs:258`)

### Multi-PBF Tile Generation
From `thoughts/research/2026-02-21_multi_pbf_tile_generation.md`:
- 500m buffer zone for overlap handling
- `residential_in_proximity` uses MAX aggregation
- `nogo_area` uses OR aggregation

## Related Research

- `thoughts/research/2026-01-14_rmdf_memory_mapped_tiles.md` - RMDF format research
- `thoughts/research/2026-01-16_tile_based_proximity_calculation.md` - Tile-based proximity
- `thoughts/research/2026-02-21_multi_pbf_tile_generation.md` - Multi-PBF handling
- `thoughts/plans/rmdf_memory_mapped_tiles/00_overview.md` - RMDF architecture overview
- `thoughts/plans/rmdf_memory_mapped_tiles/01_rmdf_format.md` - Format specification

## Implementation Recommendations

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

### Backend Implementation Steps
1. Add `rmdf-viewer` feature to Cargo.toml with dependencies
2. Create `src/debug/rmdf_viewer/mod.rs` with `run()` function
3. Implement HTTP server with tiny_http following previous pattern
4. Add `/api/manifest` handler - read and return manifest.json
5. Add `/api/tiles/:filename` handler - load MappedTile, return JSON
6. Implement static file serving with include_directory!
7. Add SPA fallback (serve index.html for non-API routes)
8. Add CliMode::RmdfViewer variant with #[cfg(feature = "rmdf-viewer")]

### Frontend Implementation Steps
1. Initialize Vite React project in `src/debug/rmdf_viewer/ui/`
2. Install dependencies: react, react-dom, leaflet, react-leaflet
3. Create API client for /api/manifest and /api/tiles endpoints
4. Implement TileList component with load/visibility toggles
5. Implement MapView component with Leaflet integration
6. Implement element rendering with color coding by flags
7. Implement direction arrows for one-way lines
8. Implement ElementPopup components for point/line details
9. Implement Legend component

### Key Dependencies
**Backend (Cargo.toml)**:
```toml
[features]
rmdf-viewer = ["dep:tiny_http", "dep:include-directory", "dep:serde_json", "dep:memmap2"]

[dependencies]
tiny_http = { version = "0.12", optional = true }
include-directory = { version = "0.1", optional = true }
```

**Frontend (package.json)**:
```json
{
  "dependencies": {
    "react": "^18",
    "react-dom": "^18",
    "leaflet": "^1.9",
    "react-leaflet": "^4"
  },
  "devDependencies": {
    "vite": "^6",
    "@vitejs/plugin-react": "^4",
    "typescript": "^5"
  }
}
```

## Design Decisions (Resolved)

1. **Tag Value Resolution**: ✅ Pre-resolve tag values on backend. The API will return fully resolved tag strings (highway, surface, smoothness) rather than indices, simplifying frontend implementation.

2. **Connected Lines in Points**: ✅ Include line references in point response. Since the whole tile is loaded, line references can be rebuilt and included in the point JSON. This enables the frontend to show "connected lines count" and link to related lines.

3. **Tile State Management**: ✅ Pure on-demand, no unloading. Frontend does not "unload" tiles - once loaded, they remain available. Backend serves tiles on-demand without state tracking. If users load too many files, they restart the server (user responsibility).

4. **Error Handling**: ✅ Return appropriate HTTP status codes with visible user-facing errors:
   - 404 for missing tiles (file not found)
   - 500 for corrupt/invalid files (parse errors)
   - Error responses include clear message for frontend display