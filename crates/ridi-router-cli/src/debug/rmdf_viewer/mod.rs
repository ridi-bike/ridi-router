mod api;
use anyhow::Result;
use include_directory::{include_directory, Dir};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use tiny_http::{Header, Method, Request, Response, Server};
use tracing::info;

// Embed built UI assets at compile time
static UI_DIST: Dir = include_directory!("$CARGO_MANIFEST_DIR/src/debug/rmdf_viewer/ui/dist");
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

    #[error("Serialization error: {0}")]
    Serialize(#[source] serde_json::Error),
}

pub fn run(input_dir: PathBuf) -> Result<()> {
    let addr = "127.0.0.1:1337";
    let server = Server::http(addr).map_err(RmdfViewerError::ServerStart)?;
    info!(addr, "Running RMDF Debug Viewer on http://{addr}");
    info!(input_dir = ?input_dir, "Serving tiles from");

    for request in server.incoming_requests() {
        if let Err(e) = handle_request(request, &input_dir) {
            info!(error = ?e, "Request handling error");
        }
    }

    Ok(())
}

fn handle_request(request: Request, input_dir: &Path) -> Result<(), RmdfViewerError> {
    // Only allow GET requests
    if *request.method() != Method::Get {
        request
            .respond(Response::from_string("Method not allowed").with_status_code(405))
            .map_err(RmdfViewerError::Respond)?;
        return Ok(());
    }

    let url = request.url().to_string();

    // API routes
    if url.starts_with("/api/") {
        let response = handle_api_request(&url, input_dir)?;
        request
            .respond(response)
            .map_err(RmdfViewerError::Respond)?;
        return Ok(());
    }

    // Static files / SPA fallback
    match handle_file_request(&url) {
        Ok(response) => {
            request
                .respond(response)
                .map_err(RmdfViewerError::Respond)?;
        }
        Err(RmdfViewerError::FileNotFound(_)) => {
            // SPA fallback: serve index.html for client-side routing
            if let Ok(index_response) = handle_file_request_for_path("index.html") {
                request
                    .respond(index_response)
                    .map_err(RmdfViewerError::Respond)?;
            } else {
                request
                    .respond(Response::from_string("Not found").with_status_code(404))
                    .map_err(RmdfViewerError::Respond)?;
            }
        }
        Err(e) => {
            request
                .respond(Response::from_string(format!("Error: {e}")).with_status_code(500))
                .map_err(RmdfViewerError::Respond)?;
        }
    }

    Ok(())
}

fn handle_api_request(
    url: &str,
    input_dir: &Path,
) -> Result<Response<std::io::Cursor<Vec<u8>>>, RmdfViewerError> {
    if url == "/api/manifest" {
        match api::get_manifest(input_dir) {
            Ok(manifest) => {
                let json = serde_json::to_string(&manifest).map_err(RmdfViewerError::Serialize)?;
                Ok(Response::from_string(json).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                        .map_err(|_| RmdfViewerError::HeaderCreate)?,
                ))
            }
            Err(e) => {
                let error_json = format!("{{\"error\": \"{e}\"}}");
                Ok(Response::from_string(error_json)
                    .with_status_code(500)
                    .with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                            .map_err(|_| RmdfViewerError::HeaderCreate)?,
                    ))
            }
        }
    } else if let Some(filename) = url.strip_prefix("/api/tiles/") {
        match api::get_tile(input_dir, filename) {
            Ok(tile) => {
                let json = serde_json::to_string(&tile).map_err(RmdfViewerError::Serialize)?;
                Ok(Response::from_string(json).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                        .map_err(|_| RmdfViewerError::HeaderCreate)?,
                ))
            }
            Err(e) => {
                // Check if file not found vs other errors
                let status = if e.to_string().contains("Failed to open")
                    || e.to_string().contains("Failed to load")
                {
                    404
                } else {
                    500
                };
                let error_json = format!("{{\"error\": \"{e}\"}}");
                Ok(Response::from_string(error_json)
                    .with_status_code(status)
                    .with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                            .map_err(|_| RmdfViewerError::HeaderCreate)?,
                    ))
            }
        }
    } else if let Some(filename) = url.strip_prefix("/api/geojson/") {
        match api::get_geojson(input_dir, filename) {
            Ok(geojson) => Ok(Response::from_string(geojson).with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/geo+json"[..])
                    .map_err(|_| RmdfViewerError::HeaderCreate)?,
            )),
            Err(e) => {
                let status = if e.to_string().contains("Failed to load") {
                    404
                } else {
                    500
                };
                let error_json = format!("{{\"error\": \"{e}\"}}");
                Ok(Response::from_string(error_json)
                    .with_status_code(status)
                    .with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                            .map_err(|_| RmdfViewerError::HeaderCreate)?,
                    ))
            }
        }
    } else {
        let response = Response::from_string("{\"error\": \"Not found\"}")
            .with_status_code(404)
            .with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .map_err(|_| RmdfViewerError::HeaderCreate)?,
            );
        Ok(response)
    }
}

/// Handle static file requests from embedded UI assets
fn handle_file_request(url: &str) -> Result<Response<Cursor<Vec<u8>>>, RmdfViewerError> {
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
    let file_name = if let Some(file_name) = file_name.strip_prefix('/') {
        file_name
    } else {
        &file_name
    };

    // SPA fallback: serve index.html for root
    let file_name = if file_name.is_empty() {
        "index.html"
    } else {
        file_name
    };

    handle_file_request_for_path(file_name)
}

/// Handle file request for a specific path in the embedded assets
fn handle_file_request_for_path(
    file_name: &str,
) -> Result<Response<Cursor<Vec<u8>>>, RmdfViewerError> {
    // Try to get file from embedded dist
    let file = UI_DIST
        .get_file(file_name)
        .ok_or_else(|| RmdfViewerError::FileNotFound(file_name.to_string()))?;

    let mime_type = file.mimetype().to_string();
    let contents = file.contents();

    Ok(Response::from_data(contents).with_header(
        Header::from_bytes(&b"Content-Type"[..], mime_type.as_bytes())
            .map_err(|_| RmdfViewerError::HeaderCreate)?,
    ))
}
