use bincode::ErrorKind;
use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions, Name, Stream};
use serde::{Deserialize, Serialize};
use std::io::{self, prelude::*, BufReader};
use tracing::{info, warn};

use crate::router_runner::{RouterRunnerError, StartFinish};

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

#[derive(Serialize, Deserialize, Debug)]
pub struct CoordsMessage {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestMessage {
    pub id: String,
    pub start: CoordsMessage,
    pub finish: CoordsMessage,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ResponseMessage {
    pub id: String,
}

pub trait MessageHandler {
    fn process(&self, request: RequestMessage) -> ResponseMessage;
}

pub struct IpcHandler<'a> {
    socket_print_name: String,
    socket_name: Name<'a>,
}

impl<'a> IpcHandler<'a> {
    pub fn init() -> Result<Self, IpcHandlerError> {
        // Pick a name.
        let socket_print_name = if GenericNamespaced::is_supported() {
            String::from("ridi-router.socket")
        } else {
            String::from("/tmp/ridi-router.sock")
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

        info!("Server running at {}", self.socket_print_name);

        for conn in listener.incoming() {
            rayon::spawn(move || match conn {
                Err(e) => {
                    warn!("Incoming connection failed {}", e);
                }
                Ok(conn) => {
                    let req = match IpcHandler::process_request(&conn) {
                        Err(err) => {
                            warn!("error from connection {:?}", err);
                            return ();
                        }
                        Ok(req) => req,
                    };
                    let resp = message_handler(req);
                    match IpcHandler::process_response(&conn, &resp) {
                        Err(err) => {
                            warn!("error from connection {:?}", err);
                            return ();
                        }
                        Ok(res) => res,
                    };
                }
            });
        }

        Ok(())
    }

    fn process_request(conn: &Stream) -> Result<RequestMessage, IpcHandlerError> {
        let mut buffer = Vec::new();

        let mut conn = BufReader::new(conn);
        info!("Incoming connection!");

        conn.read_to_end(&mut buffer)
            .map_err(|error| IpcHandlerError::ReadLine { error })?;

        let request_message = bincode::deserialize(&buffer[..])
            .map_err(|error| IpcHandlerError::DeserializeMessage { error })?;

        Ok(request_message)
    }
    fn process_response(
        conn: &Stream,
        response_message: &ResponseMessage,
    ) -> Result<(), IpcHandlerError> {
        let mut conn = BufReader::new(conn);
        let buffer = bincode::serialize(response_message)
            .map_err(|error| IpcHandlerError::SerializeMessage { error })?;
        conn.get_mut()
            .write_all(&buffer[..])
            .map_err(|error| IpcHandlerError::WriteLine { error })?;

        Ok(())
    }

    pub fn connect(&self, start_finish: &StartFinish) -> Result<(), IpcHandlerError> {
        // Preemptively allocate a sizeable buffer for receiving. This size should be enough and
        // should be easy to find for the allocator.
        let mut buffer = String::with_capacity(128);
        eprintln!("buffer {buffer:?}");

        // Create our connection. This will block until the server accepts our connection, but will
        // fail immediately if the server hasn't even started yet; somewhat similar to how happens
        // with TCP, where connecting to a port that's not bound to any server will send a "connection
        // refused" response, but that will take twice the ping, the roundtrip time, to reach the
        // client.
        let conn = Stream::connect(self.socket_name.clone())
            .map_err(|error| IpcHandlerError::Connect { error })?;
        // Wrap it into a buffered reader right away so that we could receive a single line out of it.
        let mut conn = BufReader::new(conn);
        eprintln!("con {conn:?}");

        // Send our message into the stream. This will finish either when the whole message has been
        // sent or if a send operation returns an error. (`.get_mut()` is to get the sender,
        // `BufReader` doesn't implement pass-through `Write`.)
        let message = format!(
            "{},{},{},{}\n",
            start_finish.start_lat,
            start_finish.start_lon,
            start_finish.finish_lat,
            start_finish.finish_lon
        );
        eprintln!("message {message:?}");
        conn.get_mut()
            .write_all(message.as_bytes())
            .map_err(|error| IpcHandlerError::WriteAll { error })?;
        eprintln!("message sent");
        // We now employ the buffer we allocated prior and receive a single line, interpreting a
        // newline character as an end-of-file (because local sockets cannot be portably shut down),
        // verifying validity of UTF-8 on the fly.
        conn.read_line(&mut buffer)
            .map_err(|error| IpcHandlerError::ReadLine { error })?;

        // Print out the result, getting the newline for free!
        print!("Server answered: {buffer}");
        //{
        Ok(())
    }
}
