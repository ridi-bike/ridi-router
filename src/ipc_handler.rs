use bincode::ErrorKind;
use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions, Name, Stream};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{self, prelude::*, BufReader},
};
use tracing::{info, trace, warn};

use crate::{
    router::{route::RouteStats, rules::RouterRules},
    router_runner::StartFinish,
};

#[derive(Debug)]
pub enum IpcHandlerError {
    NamespaceName { error: io::Error },
    CreateListener { error: io::Error },
    SocketAddressInUse { error: io::Error },
    ReadLine { error: io::Error },
    WriteLine { error: io::Error },
    WriteAll { error: io::Error },
    Connect { error: io::Error },
    DeserializeMessage { error: Box<ErrorKind> },
    SerializeMessage { error: Box<ErrorKind> },
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CoordsMessage {
    pub lat: f32,
    pub lon: f32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestMessage {
    pub id: String,
    pub start: CoordsMessage,
    pub finish: CoordsMessage,
    pub rules: RouterRules,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RouteMessage {
    pub coords: Vec<CoordsMessage>,
    pub stats: RouteStats,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
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
            String::from(format!("ridi-router-{}.socket", socket_name))
        } else {
            String::from(format!("/tmp/ridi-router-{}.socket", socket_name))
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
        dbg!("listen");
        let opts = ListenerOptions::new().name(self.socket_name.clone());
        dbg!("opts {opts:?}");

        let listener = match opts.create_sync() {
            Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
                return Err(IpcHandlerError::SocketAddressInUse { error: e });
            }
            x => x.map_err(|error| IpcHandlerError::CreateListener { error })?,
        };
        dbg!("listener {listener:?}");

        info!("Server running at {}", self.socket_print_name);

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
                            return ();
                        }
                        Ok(req) => req,
                    };
                    dbg!("calling msg handler");
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
        info!("Incoming connection!");

        let mut mes_len_buf = [0u8; 8];
        conn.read_exact(&mut mes_len_buf)
            .map_err(|error| IpcHandlerError::ReadLine { error })?;

        info!("message size {:?}", u64::from_ne_bytes(mes_len_buf));

        let mut buffer = vec![0; u64::from_ne_bytes(mes_len_buf) as usize];
        conn.read_exact(&mut buffer[..])
            .map_err(|error| IpcHandlerError::ReadLine { error })?;

        info!("message received {}", buffer.len());

        let request_message = bincode::deserialize(&buffer[..])
            .map_err(|error| IpcHandlerError::DeserializeMessage { error })?;

        info!("deserialized {request_message:?}");

        Ok(request_message)
    }
    fn process_response(
        conn: &Stream,
        response_message: &ResponseMessage,
    ) -> Result<(), IpcHandlerError> {
        let mut conn = BufReader::new(conn);

        let buffer = bincode::serialize(response_message)
            .map_err(|error| IpcHandlerError::SerializeMessage { error })?;

        let mes_len_bytes: u64 = buffer.len() as u64;
        conn.get_mut()
            .write_all(&mes_len_bytes.to_ne_bytes()[..])
            .map_err(|error| IpcHandlerError::WriteAll { error })?;
        info!("message size sent {}", mes_len_bytes);

        conn.get_mut()
            .write_all(&buffer[..])
            .map_err(|error| IpcHandlerError::WriteLine { error })?;

        Ok(())
    }

    pub fn connect(
        &self,
        start_finish: &StartFinish,
        rules: RouterRules,
    ) -> Result<ResponseMessage, IpcHandlerError> {
        let conn = Stream::connect(self.socket_name.clone())
            .map_err(|error| IpcHandlerError::Connect { error })?;

        let mut conn = BufReader::new(conn);

        let req_msg = RequestMessage {
            id: "ooo".to_string(),
            start: CoordsMessage {
                lat: start_finish.start_lat,
                lon: start_finish.start_lon,
            },
            finish: CoordsMessage {
                lat: start_finish.finish_lat,
                lon: start_finish.finish_lon,
            },
            rules,
        };
        let req_buf = bincode::serialize(&req_msg)
            .map_err(|error| IpcHandlerError::SerializeMessage { error })?;

        let mes_len_bytes: u64 = req_buf.len() as u64;
        conn.get_mut()
            .write_all(&mes_len_bytes.to_ne_bytes()[..])
            .map_err(|error| IpcHandlerError::WriteAll { error })?;
        info!("message size sent {}", mes_len_bytes);
        conn.get_mut()
            .write_all(&req_buf[..])
            .map_err(|error| IpcHandlerError::WriteAll { error })?;
        info!("message sent {}", req_buf.len());

        let mut mes_len_buf = [0u8; 8];
        conn.read_exact(&mut mes_len_buf)
            .map_err(|error| IpcHandlerError::ReadLine { error })?;

        info!("message size {:?}", u64::from_ne_bytes(mes_len_buf));

        let mut resp_buf = vec![0; u64::from_ne_bytes(mes_len_buf) as usize];
        conn.read_exact(&mut resp_buf[..])
            .map_err(|error| IpcHandlerError::ReadLine { error })?;

        info!("message received {}", resp_buf.len());

        let resp_msg: ResponseMessage = bincode::deserialize(&resp_buf[..])
            .map_err(|error| IpcHandlerError::DeserializeMessage { error })?;

        Ok(resp_msg)
    }
}
