# Phase 7: Asset Bundling & Integration

## Overview

Build the frontend assets with Vite and embed them into the Rust binary using the `include_directory!` macro. Wire up static file serving in the HTTP server and verify the complete integration works end-to-end.

## Changes Required

### 1. Build Frontend Assets

**Command**: Run from `src/debug/rmdf_viewer/ui/`

```bash
bun run build
```

**Result**: Creates `src/debug/rmdf_viewer/ui/dist/` with:
- `index.html`
- `assets/index-[hash].js`
- `assets/index-[hash].css`

**Rationale**: Vite's build output is optimized and hashed for cache busting. The dist directory is the target for embedding.

### 2. Add .gitignore for dist

**File**: `.gitignore`

**Changes**: Add dist directory to gitignore

```gitignore
# RMDF Viewer UI build output
src/debug/rmdf_viewer/ui/dist/
```

**Rationale**: Built assets shouldn't be committed. They're rebuilt during the Cargo build process (or manually before building the binary).

### 3. Update mod.rs to Embed Assets

**File**: `src/debug/rmdf_viewer/mod.rs`

**Changes**: Add include_directory! macro and static

```rust
use anyhow::Result;
use include_directory::{include_directory, Dir};
use std::path::PathBuf;
use tiny_http::{Header, Method, Request, Response, Server};
use tracing::info;

// Embed built UI assets at compile time
static UI_DIST: Dir = include_directory!("$CARGO_MANIFEST_DIR/src/debug/rmdf_viewer/ui/dist");

// ... existing code ...
```

**Rationale**: The `include_directory!` macro embeds all files in the dist directory at compile time. The `$CARGO_MANIFEST_DIR` ensures the path works regardless of where cargo is run from.

### 4. Add include-directory as Feature Dependency

**File**: `Cargo.toml`

**Changes**: Add include-directory to rmdf-viewer feature and make it optional

```toml
[features]
rmdf-viewer = ["dep:tiny_http", "dep:include-directory"]

[dependencies]
# Change from:
# include_directory = "0.1.1"
# To:
include-directory = { version = "0.1.1", optional = true }
```

**Rationale**: Making `include_directory` optional reduces compile time and binary size when the viewer isn't needed.

### 5. Implement Static File Serving

**File**: `src/debug/rmdf_viewer/mod.rs`

**Changes**: Implement `handle_file_request()` function

```rust
use std::io::Cursor;

fn handle_file_request(request: &Request) -> Result<Response<Cursor<Vec<u8>>>, RmdfViewerError> {
    let url = request.url();
    
    // Security: sanitize filename to prevent path traversal
    let mut file_name = url.to_string();
    loop {
        let len = file_name.len();
        file_name = file_name.replace("../", "");
        file_name = file_name.replace("./", "");
        if file_name.len() == len {
            break;
        }
    }
    
    // Remove leading slash
    let file_name = if file_name.starts_with('/') {
        &file_name[1..]
    } else {
        &file_name
    };
    
    // SPA fallback: serve index.html for root
    let file_name = if file_name.is_empty() {
        "index.html"
    } else {
        file_name
    };
    
    // Try to get file from embedded dist
    let file = UI_DIST
        .get_file(file_name)
        .ok_or_else(|| RmdfViewerError::FileNotFound(file_name.to_string()))?;
    
    let mime_type = file.mimetype().to_string();
    let contents = file.contents();
    
    Ok(Response::from_data(contents)
        .with_header(
            Header::from_bytes(&b"Content-Type"[..], mime_type.as_bytes())
                .map_err(|_| RmdfViewerError::HeaderCreate)?,
        ))
}
```

**Rationale**: 
- Path traversal protection strips `../` and `./`
- SPA fallback serves `index.html` for root path
- MIME type detection from file extension
- Returns file contents directly from embedded bytes

### 6. Update Request Handler

**File**: `src/debug/rmdf_viewer/mod.rs`

**Changes**: Update `handle_request()` to use file serving

```rust
fn handle_request(request: &Request, input_dir: &PathBuf) -> Result<(), RmdfViewerError> {
    // Only allow GET requests
    if request.method() != &Method::Get {
        request
            .respond(Response::from_string("Method not allowed").with_status_code(405))
            .map_err(RmdfViewerError::Respond)?;
        return Ok(());
    }

    let url = request.url();

    // API routes (highest priority)
    if url.starts_with("/api/") {
        let response = handle_api_request(request, input_dir)?;
        request.respond(response).map_err(RmdfViewerError::Respond)?;
        return Ok(());
    }

    // Static files / SPA fallback
    match handle_file_request(request) {
        Ok(response) => {
            request.respond(response).map_err(RmdfViewerError::Respond)?;
        }
        Err(RmdfViewerError::FileNotFound(_)) => {
            // SPA fallback: serve index.html for client-side routing
            if let Ok(index_response) = handle_file_request_for_path("index.html") {
                request.respond(index_response).map_err(RmdfViewerError::Respond)?;
            } else {
                request
                    .respond(Response::from_string("Not found").with_status_code(404))
                    .map_err(RmdfViewerError::Respond)?;
            }
        }
        Err(e) => {
            request
                .respond(Response::from_string(format!("Error: {}", e)).with_status_code(500))
                .map_err(RmdfViewerError::Respond)?;
        }
    }

    Ok(())
}
```

**Rationale**: 
- API routes take priority
- Not found - serve index.html for SPA client-side routing
- Error responses with appropriate status codes

### 7. Update Vite Config for Dev Proxy

**File**: `src/debug/rmdf_viewer/ui/vite.config.ts`

**Changes**: Add dev server proxy for development

```typescript
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: false,
    minify: 'esbuild',
  },
  base: '/',
  server: {
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:1337',
        changeOrigin: true,
      },
    },
  },
})
```

**Rationale**: During development, the Vite dev server runs on a different port. The proxy forwards API requests to the backend.

## Success Criteria

### Automated Verification:
- [ ] `bun run build` in ui/ creates dist/ directory
- [ ] dist/ contains index.html
- [ ] dist/ contains assets/ with .js and .css files
- [ ] `cargo build --features rmdf-viewer` compiles without errors
- [ ] Binary size is reasonable (<50MB for debug build)

### Manual Verification:
- [ ] Build UI: `cd src/debug/rmdf_viewer/ui && bun run build`
- [ ] Build binary: `cargo build --features rmdf-viewer`
- [ ] Run: `./target/debug/ridi-router rmdf-viewer --input-dir <test-dir>`
- [ ] Server logs: "Running RMDF Debug Viewer on http://127.0.0.1:1337"
- [ ] Open http://127.0.0.1:1337 in browser
- [ ] Page loads with RMDF Debug Viewer title
- [ ] Map appears with OpenStreetMap tiles
- [ ] Tile list shows in sidebar
- [ ] API calls work: Network tab shows /api/manifest returning 200
- [ ] Click Load on a tile - points and lines appear
- [ ] All features from Phases 4-6 work correctly
- [ ] Refresh page - still works (SPA routing)
- [ ] Ctrl+C - server shuts down cleanly

## Dependencies

- Depends on: Phase 6 (Frontend Interactivity)
- Blocks: None - this is the final phase

## Out of Scope for This Phase

- New features - all features complete
- Performance optimizations beyond standard Vite minification
- Custom build scripts (just document manual build)
- CI/CD integration (future work)

## Notes

### Build Workflow

**Production build:**
```bash
# 1. Build frontend
cd src/debug/rmdf_viewer/ui
bun install
bun run build

# 2. Build binary (embeds dist/)
cd ../../..
cargo build --features rmdf-viewer --release

# 3. Run
./target/release/ridi-router rmdf-viewer --input-dir /path/to/tiles
```

**Development:**
```bash
# Terminal 1: Backend
cargo run --features rmdf-viewer -- rmdf-viewer --input-dir /path/to/tiles

# Terminal 2: Frontend (hot reload)
cd src/debug/rmdf_viewer/ui
bun run dev
```

### File Size Expectations

- UI dist/ after build: ~100-500KB (minified)
- Binary with rmdf-viewer: +1-2MB over base
- Total release binary: ~15-20MB

### Previous Implementation Reference

The previous debug-viewer (commit 313bc45^) used the same pattern:
```rust
static DIST_DIR: Dir = include_directory!("$CARGO_MANIFEST_DIR/src/debug/viewer/ui/dist");
```

This approach is well-tested and reliable.
