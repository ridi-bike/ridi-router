use std::{
    error::Error,
    fs::File,
    io::{Cursor, Read},
    path::PathBuf,
};
use tiny_http::{Header, Method, Request, Response, Server};
use tracing::info;

// const HTML: &str = include_str!("./viewer.html");
// const JS: &str = include_str!("./viewer.js");

#[derive(Debug, thiserror::Error)]
pub enum DebugViewerError {
    #[error("Could not start server: {error}")]
    ServerStart {
        error: Box<dyn Error + Send + Sync + 'static>,
    },

    #[error("Could not start server")]
    HeaderCreate,

    #[error("Could not respond: {error}")]
    Respond { error: io::Error },

    #[error("Could not open file: {error}")]
    FileOpen { error: io::Error },
}
pub struct DebugViewer;

impl DebugViewer {
    pub fn run(debug_dir: PathBuf) -> Result<(), DebugViewerError> {
        let addr = "0.0.0.0:1337";
        let server = Server::http(addr).map_err(|error| DebugViewerError::ServerStart { error })?;
        info!(addr, "Running Debug Viewer on http://{addr}");

        for request in server.incoming_requests() {
            let response = DebugViewer::handle_request(&request)?;
            request
                .respond(response)
                .map_err(|error| DebugViewerError::Respond { error })?;
        }

        Ok(())
    }

    fn handle_request(request: &Request) -> Result<Response<Cursor<Vec<u8>>>, DebugViewerError> {
        println!(
            "received request! method: {:?}, url: {:?}",
            request.method(),
            request.url(),
        );

        if request.method() != &Method::Get {
            return Ok(Response::from_string("not allowed").with_status_code(405));
        }

        // Ok(match request.url() {
        //     "/" => Response::from_string(HTML).with_header(
        //         Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..])
        //             .map_err(|_| DebugViewerError::HeaderCreate)?,
        //     ),
        //     "/viewer.js" => Response::from_string(JS).with_header(
        //         Header::from_bytes(&b"Content-Type"[..], &b"text/javascript"[..])
        //             .map_err(|_| DebugViewerError::HeaderCreate)?,
        //     ),
        //     _ => Response::from_string("not found").with_status_code(404),
        // })
        Ok(match request.url() {
            "/" => {
                let mut contents = String::new();
                File::open("./src/debug/viewer/viewer.html")
                    .map_err(|error| DebugViewerError::FileOpen { error })?
                    .read_to_string(&mut contents)
                    .map_err(|error| DebugViewerError::FileOpen { error })?;

                Response::from_string(contents).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..])
                        .map_err(|_| DebugViewerError::HeaderCreate)?,
                )
            }
            "/viewer.js" => {
                let mut contents = String::new();
                File::open("./src/debug/viewer/viewer.js")
                    .map_err(|error| DebugViewerError::FileOpen { error })?
                    .read_to_string(&mut contents)
                    .map_err(|error| DebugViewerError::FileOpen { error })?;

                Response::from_string(contents).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"text/javascript"[..])
                        .map_err(|_| DebugViewerError::HeaderCreate)?,
                )
            }
            _ => Response::from_string("not found").with_status_code(404),
        })
    }
}
