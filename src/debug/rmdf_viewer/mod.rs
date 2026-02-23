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

fn handle_api_request(url: &str, _input_dir: &PathBuf) -> Result<Response<std::io::Cursor<Vec<u8>>>, RmdfViewerError> {
    if url == "/api/manifest" {
        // TODO: Implement in Phase 2
        let response = Response::from_string("{\"error\": \"Not implemented yet\"}")
            .with_status_code(501)
            .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                .map_err(|_| RmdfViewerError::HeaderCreate)?);
        return Ok(response);
    }

    if let Some(_filename) = url.strip_prefix("/api/tiles/") {
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
