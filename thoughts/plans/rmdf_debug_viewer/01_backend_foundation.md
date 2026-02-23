# Phase 1: Backend Foundation

## Overview

Set up the HTTP server infrastructure and CLI integration for the RMDF debug viewer. This phase creates the module structure, adds the feature flag, implements the basic HTTP server with `tiny_http`, and adds the CLI subcommand.

## Changes Required

### 1. Cargo.toml - Add Feature and Dependency

**File**: `Cargo.toml`

**Changes**: Add `rmdf-viewer` feature and `tiny_http` optional dependency

```toml
[features]
default = []
rule-schema-writer = []
debug-split-gpx = []
rmdf-viewer = ["dep:tiny_http", "dep:ts-rs"]

[dependencies]
# ... existing dependencies ...
tiny_http = { version = "0.12", optional = true }
ts-rs = { version = "10", optional = true }
```

**Rationale**: Feature-gating ensures the viewer is only compiled when needed. `tiny_http` is a lightweight HTTP server. `ts-rs` generates TypeScript types from Rust structs for type-safe API communication.

### 2. Create Module Structure

**File**: `src/debug/rmdf_viewer/mod.rs`

**Changes**: Create new module with HTTP server implementation

```rust
use anyhow::Result;
use std::path::PathBuf;
use tiny_http::{Header, Method, Request, Response, Server};
use tracing::info;

#[derive(Debug, thiserror::Error)]
pub enum RmdfViewerError {
    #[error("Could not start server: {0}")]
    ServerStart(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),

    #[error("Could not create header")]
    HeaderCreate,

    #[error("Could not respond: {0}")]
    Respond(#[source] std::io::Error),

    #[error("File not found: {0}")]
    FileNotFound(String),
}

pub fn run(input_dir: PathBuf) -> Result<()> {
    let addr = "127.0.0.1:1337";
    let server = Server::http(addr).map_err(|e| RmdfViewerError::ServerStart(Box::new(e)))?;
    info!(addr, "Running RMDF Debug Viewer on http://{addr}");
    info!(input_dir = ?input_dir, "Serving tiles from");

    for request in server.incoming_requests() {
        if let Err(e) = handle_request(&request, &input_dir) {
            info!(error = ?e, "Request handling error");
        }
    }

    Ok(())
}

fn handle_request(request: &Request, input_dir: &PathBuf) -> Result<(), RmdfViewerError> {
    // Only allow GET requests
    if request.method() != &Method::Get {
        request
            .respond(Response::from_string("Method not allowed").with_status_code(405))
            .map_err(RmdfViewerError::Respond)?;
        return Ok(());
    }

    let url = request.url();

    // API routes
    if url.starts_with("/api/") {
        let response = handle_api_request(request, input_dir)?;
        request.respond(response).map_err(RmdfViewerError::Respond)?;
        return Ok(());
    }

    // Static files / SPA fallback (placeholder for now)
    let response = Response::from_string("RMDF Debug Viewer - API ready, UI not built yet")
        .with_status_code(503);
    request.respond(response).map_err(RmdfViewerError::Respond)?;

    Ok(())
}

fn handle_api_request(request: &Request, input_dir: &PathBuf) -> Result<Response<std::io::Cursor<Vec<u8>>>, RmdfViewerError> {
    let url = request.url();

    if url == "/api/manifest" {
        // TODO: Implement in Phase 2
        let response = Response::from_string("{\"error\": \"Not implemented yet\"}")
            .with_status_code(501)
            .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                .map_err(|_| RmdfViewerError::HeaderCreate)?);
        return Ok(response);
    }

    if let Some(filename) = url.strip_prefix("/api/tiles/") {
        // TODO: Implement in Phase 2
        let response = Response::from_string("{\"error\": \"Not implemented yet\"}")
            .with_status_code(501)
            .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                .map_err(|_| RmdfViewerError::HeaderCreate)?);
        return Ok(response);
    }

    let response = Response::from_string("{\"error\": \"Not found\"}")
        .with_status_code(404)
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
            .map_err(|_| RmdfViewerError::HeaderCreate)?);
    Ok(response)
}
```

**Rationale**: This follows the pattern from the previous debug-viewer (commit 313bc45^). The server is simple and handles GET requests only. API endpoints return 501 (Not Implemented) as placeholders for Phase 2.

### 3. Create API Module Stub

**File**: `src/debug/rmdf_viewer/api.rs`

**Changes**: Create empty module for Phase 2

```rust
// API handlers will be implemented in Phase 2
// This file is reserved for:
// - get_manifest() - GET /api/manifest
// - get_tile() - GET /api/tiles/:filename
```

**Rationale**: Separating API logic into its own module keeps the code organized and makes each file focused.

### 4. Update debug Module

**File**: `src/debug/mod.rs`

**Changes**: Add `rmdf_viewer` module with feature gate

```rust
// Existing modules...

#[cfg(feature = "rmdf-viewer")]
pub mod rmdf_viewer;
```

**Rationale**: Feature-gated module ensures it's only compiled when the feature is enabled.

### 5. Add CLI Subcommand

**File**: `src/router_runner.rs`

**Changes**: Add `RmdfViewer` variant to `CliMode` enum (around line 217)

```rust
#[derive(Subcommand)]
enum CliMode {
    // ... existing variants ...

    /// Start RMDF debug viewer web server
    #[cfg(feature = "rmdf-viewer")]
    RmdfViewer {
        #[arg(long, value_name = "DIR")]
        /// Directory containing manifest.json and RMDF tile files
        input_dir: PathBuf,
    },
}
```

**Changes**: Add match arm in `RouterRunner::run()` (around line 488)

```rust
match &cli.mode {
    // ... existing match arms ...

    #[cfg(feature = "rmdf-viewer")]
    CliMode::RmdfViewer { input_dir } => {
        crate::debug::rmdf_viewer::run(input_dir.clone())
    }
}
```

**Rationale**: Follows the existing CLI pattern with feature-gated subcommands.

## Success Criteria

### Automated Verification:
- [x] `cargo build --features rmdf-viewer` compiles without errors
- [x] `cargo build` (without feature) compiles without errors
- [x] `ridi-router --help` shows `rmdf-viewer` subcommand when feature enabled

### Manual Verification:
- [ ] `ridi-router rmdf-viewer --input-dir /tmp/test` starts without panic
- [ ] Server logs: "Running RMDF Debug Viewer on http://127.0.0.1:1337"
- [ ] `curl http://127.0.0.1:1337/api/manifest` returns 501 with JSON
- [ ] `curl http://127.0.0.1:1337/` returns 503 with placeholder message
- [ ] POST request to any endpoint returns 405
- [ ] Ctrl+C shuts down server cleanly

## Dependencies

- Depends on: None - can start immediately
- Blocks: Phase 2 (Backend API)

## Risks & Mitigations

- **Risk**: `tiny_http` API may have changed since last use
  - **Mitigation**: Check tiny_http 0.12 documentation, adapt as needed

## Out of Scope for This Phase

**CRITICAL**: The following items are explicitly NOT part of this phase. Do NOT implement these now:
- Actual API implementations (/api/manifest, /api/tiles) - Phase 2
- Static file serving with include_directory! - Phase 7
- React frontend - Phases 3-6
- JSON serialization of tile data - Phase 2
- Error handling for missing files - Phase 2

## Phase Boundary Rules

**IMPORTANT**: When executing this phase:
1. **Stay within scope** - Only implement what is listed in "Changes Required" above
2. **Do NOT rush ahead** - Even if you see an obvious next step, stop at this phase's boundary
3. **Respect dependencies** - Phase 2 depends on this phase being complete and verified
4. **501 responses are intentional** - Placeholder responses are correct for this phase

## Notes

- The HTTP server runs in a blocking loop. For graceful shutdown, the user presses Ctrl+C which interrupts the process. This is acceptable for a debug tool.
- No threading or async is needed - `tiny_http` handles requests sequentially.
- The input_dir validation (exists, contains manifest.json) will be added in Phase 2.

## Deviations from Plan

### Phase 1: Backend Foundation
- **Original Plan**: The plan specified exact code snippets for the HTTP server implementation
- **Actual Implementation**: Made several adjustments to match the actual tiny_http 0.12 API:
  1. Changed `request.method() != &Method::Get` to `*request.method() != Method::Get` because `method()` returns `&Method`, not `Method`
  2. Changed `handle_request` to take `Request` by value instead of `&Request` because `respond()` takes ownership of `self`
  3. Changed `Server::http(addr).map_err(|e| RmdfViewerError::ServerStart(Box::new(e)))` to `.map_err(RmdfViewerError::ServerStart)` because the error is already a `Box<dyn Error>`, avoiding double-boxing
  4. Refactored `handle_api_request` to take `&str` URL instead of `&Request` to work with the ownership model
  5. Added `#[cfg(feature = "rmdf-viewer")] mod debug;` to `src/main.rs` since the debug module didn't exist before
- **Reason for Deviation**: The tiny_http 0.12 API has different ownership semantics than the plan assumed
- **Impact Assessment**: No functional impact - the server works correctly with these adjustments. The deviations are implementation details that don't affect the API or behavior.
- **Date/Time**: 2026-02-23
