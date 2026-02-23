mod api;
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

fn handle_request(request: Request, input_dir: &PathBuf) -> Result<(), RmdfViewerError> {
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
        request.respond(response).map_err(RmdfViewerError::Respond)?;
        return Ok(());
    }

    // Static files / SPA fallback (placeholder for now)
    let response = Response::from_string("RMDF Debug Viewer - API ready, UI not built yet")
        .with_status_code(503);
    request.respond(response).map_err(RmdfViewerError::Respond)?;

    Ok(())
}

fn handle_api_request(url: &str, input_dir: &PathBuf) -> Result<Response<std::io::Cursor<Vec<u8>>>, RmdfViewerError> {
    if url == "/api/manifest" {
        match api::get_manifest(input_dir) {
            Ok(manifest) => {
                let json = serde_json::to_string(&manifest)
                    .map_err(RmdfViewerError::Serialize)?;
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
                    .map_err(RmdfViewerError::Serialize)?;
                Ok(Response::from_string(json)
                    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                        .map_err(|_| RmdfViewerError::HeaderCreate)?))
            }
            Err(e) => {
                // Check if file not found vs other errors
                let status = if e.to_string().contains("Failed to open") || e.to_string().contains("Failed to load") {
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

