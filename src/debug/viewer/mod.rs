use std::{
    error::Error,
    io::{self, prelude::BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
};
use tiny_http::{Header, Response, Server};

const HTML: &str = include_str!("./viewer.html");
const JS: &str = include_str!("./viewer.js");

#[derive(Debug, thiserror::Error)]
pub enum DebugViewerError {
    #[error("Could not start server: {error}")]
    ServerStart {
        error: Box<dyn Error + Send + Sync + 'static>,
    },
    #[error("Could not start server: {error}")]
    HeaderCreate,
}
pub struct DebugViewer;

impl DebugViewer {
    pub fn run(debug_dir: PathBuf) -> Result<(), DebugViewerError> {
        let server = Server::http("0.0.0.0:8000")
            .map_err(|error| DebugViewerError::ServerStart { error })?;

        for request in server.incoming_requests() {
            println!(
                "received request! method: {:?}, url: {:?}, headers: {:?}",
                request.method(),
                request.url(),
                request.headers()
            );

            let response = match request.url() {
                "/" => Response::from_string(HTML).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..])
                        .map_err(|_| DebugViewerError::HeaderCreate)?,
                ),
                "/viewer.js" => Response::from_string(JS).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"text/javascript"[..])
                        .map_err(|_| DebugViewerError::HeaderCreate)?,
                ),
                _ => Response::from_string("not found"),
            };

            request.respond(response);
        }

        Ok(())
    }
}
