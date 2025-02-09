use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions, Name, Stream};
use serde::{Deserialize, Serialize};
use std::io::{self, prelude::*, BufReader};
use tracing::{info, trace, warn};

use crate::{
    router::{route::RouteStats, rules::RouterRules},
    router_runner::RoutingMode,
};

#[derive(Debug, thiserror::Error)]
pub enum IpcHandlerError {
    #[error("Namespace Name cannot be created, cause {error}")]
    NamespaceName { error: io::Error },

    #[error("Failed to create IPC listener: {error}")]
    CreateListener { error: io::Error },

    #[error("Socket address already in use: {error}")]
    SocketAddressInUse { error: io::Error },

    #[error("Failed to read from IPC connection: {error}")]
    ReadLine { error: io::Error },

    #[error("Failed to write line to IPC connection: {error}")]
    WriteLine { error: io::Error },

    #[error("Failed to write data to IPC connection: {error}")]
    WriteAll { error: io::Error },

    #[error("Failed to connect to IPC socket: {error}")]
    Connect { error: io::Error },

    #[error("Failed to extract utf8 from message: {error}")]
    Utf8Message { error: std::str::Utf8Error },

    #[error("Failed to deserialize message: {error}")]
    DeserializeMessage { error: serde_json::Error },

    #[error("Failed to serialize message: {error}")]
    SerializeMessage { error: serde_json::Error },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestMessage {
    pub id: String,
    pub routing_mode: RoutingMode,
    pub rules: RouterRules,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RouteMessage {
    pub coords: Vec<(f32, f32)>,
    pub stats: RouteStats,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum RouterResult {
    Error { message: String },
    Ok { routes: Vec<RouteMessage> },
}
#[derive(Serialize, Deserialize, Debug)]
pub struct ResponseMessage {
    pub id: String,
    pub result: RouterResult,
}

pub struct IpcHandler<'a> {
    socket_print_name: String,
    socket_name: Name<'a>,
}

impl<'a> IpcHandler<'a> {
    pub fn init(socket_name: Option<String>) -> Result<Self, IpcHandlerError> {
        let socket_name = socket_name.map_or("1".to_string(), |v| {
            v.chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .collect::<String>()
        });
        let socket_print_name = if GenericNamespaced::is_supported() {
            format!("ridi-router-{}.socket", socket_name)
        } else {
            format!("/tmp/ridi-router-{}.socket", socket_name)
        };

        let socket_name = socket_print_name
            .clone()
            .to_ns_name::<GenericNamespaced>()
            .map_err(|error| IpcHandlerError::NamespaceName { error })?;

        Ok(Self {
            socket_print_name,
            socket_name,
        })
    }

    pub fn listen<T>(&self, message_handler: T) -> Result<(), IpcHandlerError>
    where
        T: Fn(RequestMessage) -> ResponseMessage + Sync + Send + Copy + 'static,
    {
        let opts = ListenerOptions::new().name(self.socket_name.clone());

        let listener = match opts.create_sync() {
            Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
                return Err(IpcHandlerError::SocketAddressInUse { error: e });
            }
            x => x.map_err(|error| IpcHandlerError::CreateListener { error })?,
        };

        info!(server_name = self.socket_print_name, "Server running");

        println!(";RIDI_ROUTER SERVER READY;"); // this is in stdout so calling processes know the server is ready to accept connections

        for conn in listener.incoming() {
            rayon::spawn(move || match conn {
                Err(e) => {
                    warn!("Incoming connection failed {}", e);
                }
                Ok(conn) => {
                    trace!("received connection");
                    let req = match IpcHandler::process_request(&conn) {
                        Err(err) => {
                            warn!("error from connection {:?}", err);
                            return;
                        }
                        Ok(req) => req,
                    };
                    let resp = message_handler(req);
                    if let Err(error) = IpcHandler::process_response(&conn, &resp) {
                        warn!("error from connection {:?}", error);
                    }
                }
            });
        }

        Ok(())
    }

    fn process_request(conn: &Stream) -> Result<RequestMessage, IpcHandlerError> {
        let mut conn = BufReader::new(conn);

        let mut mes_len_buf = [0u8; 8];
        conn.read_exact(&mut mes_len_buf)
            .map_err(|error| IpcHandlerError::ReadLine { error })?;

        info!(
            message_size = u64::from_ne_bytes(mes_len_buf),
            "Infomcing message"
        );

        let mut buffer = vec![0; u64::from_ne_bytes(mes_len_buf) as usize];
        conn.read_exact(&mut buffer[..])
            .map_err(|error| IpcHandlerError::ReadLine { error })?;

        let string_message =
            std::str::from_utf8(&buffer).map_err(|error| IpcHandlerError::Utf8Message { error })?;

        let request_message: RequestMessage = serde_json::from_str(&string_message)
            .map_err(|error| IpcHandlerError::DeserializeMessage { error })?;

        Ok(request_message)
    }
    fn process_response(
        conn: &Stream,
        response_message: &ResponseMessage,
    ) -> Result<(), IpcHandlerError> {
        let mut conn = BufReader::new(conn);

        let string_message = serde_json::to_string(response_message)
            .map_err(|error| IpcHandlerError::SerializeMessage { error })?;

        let buffer = string_message.as_bytes();

        let mes_len_bytes: u64 = buffer.len() as u64;
        conn.get_mut()
            .write_all(&mes_len_bytes.to_ne_bytes()[..])
            .map_err(|error| IpcHandlerError::WriteAll { error })?;

        conn.get_mut()
            .write_all(buffer)
            .map_err(|error| IpcHandlerError::WriteLine { error })?;

        Ok(())
    }

    pub fn connect(
        &self,
        routing_mode: &RoutingMode,
        rules: RouterRules,
    ) -> Result<ResponseMessage, IpcHandlerError> {
        let conn = Stream::connect(self.socket_name.clone())
            .map_err(|error| IpcHandlerError::Connect { error })?;

        let mut conn = BufReader::new(conn);

        let req_msg = RequestMessage {
            id: "ooo".to_string(),
            routing_mode: routing_mode.clone(),
            rules,
        };
        let string_req = serde_json::to_string(&req_msg)
            .map_err(|error| IpcHandlerError::SerializeMessage { error })?;

        let req_buf = string_req.as_bytes();

        let mes_len_bytes: u64 = req_buf.len() as u64;
        conn.get_mut()
            .write_all(&mes_len_bytes.to_ne_bytes()[..])
            .map_err(|error| IpcHandlerError::WriteAll { error })?;

        conn.get_mut()
            .write_all(req_buf)
            .map_err(|error| IpcHandlerError::WriteAll { error })?;

        let mut mes_len_buf = [0u8; 8];
        conn.read_exact(&mut mes_len_buf)
            .map_err(|error| IpcHandlerError::ReadLine { error })?;

        let mut resp_buf = vec![0; u64::from_ne_bytes(mes_len_buf) as usize];
        conn.read_exact(&mut resp_buf[..])
            .map_err(|error| IpcHandlerError::ReadLine { error })?;

        let string_resp = std::str::from_utf8(&resp_buf)
            .map_err(|error| IpcHandlerError::Utf8Message { error })?;

        let resp_msg: ResponseMessage = serde_json::from_str(string_resp)
            .map_err(|error| IpcHandlerError::DeserializeMessage { error })?;

        Ok(resp_msg)
    }
}
