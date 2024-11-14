use clap::error;
use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions, Name, Stream};
use std::{
    io::{self, prelude::*, BufReader},
    num::ParseFloatError,
    thread,
    time::Duration,
};

use crate::router_mode::{RouterMode, StartFinish};

#[derive(Debug)]
pub enum IpcHandlerError {
    NamespaceName {
        error: io::Error,
    },
    CreateListener {
        error: io::Error,
    },
    SocketAddressInUse {
        error: io::Error,
    },
    ReadLine {
        error: io::Error,
    },
    WriteLine {
        error: io::Error,
    },
    WriteAll {
        error: io::Error,
    },
    Connect {
        error: io::Error,
    },
    ParseMessageCoords {
        error: ParseFloatError,
        value: String,
        pos: usize,
    },
    ParseMessageUnexpected {
        pos: usize,
        value: String,
    },
    ParseMessageMissingValue,
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

    pub fn listen(&self) -> Result<(), IpcHandlerError> {
        // Define a function that checks for errors in incoming connections. We'll use this to filter
        // through connections that fail on initialization for one reason or another.
        fn handle_error(conn: io::Result<Stream>) -> Option<Stream> {
            match conn {
                Ok(c) => Some(c),
                Err(e) => {
                    eprintln!("Incoming connection failed: {e}");
                    None
                }
            }
        }

        // Configure our listener...
        let opts = ListenerOptions::new().name(self.socket_name.clone());

        // ...then create it.
        let listener = match opts.create_sync() {
            Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
                // When a program that uses a file-type socket name terminates its socket server
                // without deleting the file, a "corpse socket" remains, which can neither be
                // connected to nor reused by a new listener. Normally, Interprocess takes care of
                // this on affected platforms by deleting the socket file when the listener is
                // dropped. (This is vulnerable to all sorts of races and thus can be disabled.)
                //
                // There are multiple ways this error can be handled, if it occurs, but when the
                // listener only comes from Interprocess, it can be assumed that its previous instance
                // either has crashed or simply hasn't exited yet. In this example, we leave cleanup
                // up to the user, but in a real application, you usually don't want to do that.
                eprintln!(
                    "Error: could not start server because the socket file is occupied. Please check if
                    {} is in use by another process and try again.", self.socket_print_name
                  );
                return Err(IpcHandlerError::SocketAddressInUse { error: e });
            }
            x => x.map_err(|error| IpcHandlerError::CreateListener { error })?,
        };

        // The synchronization between the server and client, if any is used, goes here.
        eprintln!("Server running at {}", self.socket_print_name);

        for conn in listener.incoming().filter_map(handle_error) {
            rayon::spawn(move || {
                let res = IpcHandler::process_connection(&conn);
                if let Err(err) = res {
                    eprintln!("error from connection {:?}", err)
                }
            });
        }

        Ok(())
    }

    fn process_connection(conn: &Stream) -> Result<(), IpcHandlerError> {
        let mut buffer = String::with_capacity(128);

        // Wrap the connection into a buffered receiver right away
        // so that we could receive a single line from it.
        let mut conn = BufReader::new(conn);
        println!("Incoming connection!");

        // Since our client example sends first, the server should receive a line and only then
        // send a response. Otherwise, because receiving from and sending to a connection cannot
        // be simultaneous without threads or async, we can deadlock the two processes by having
        // both sides wait for the send buffer to be emptied by the other.
        conn.read_line(&mut buffer)
            .map_err(|error| IpcHandlerError::ReadLine { error })?;

        let mut start_lat: Option<f64> = None;
        let mut start_lon: Option<f64> = None;
        let mut finish_lat: Option<f64> = None;
        let mut finish_lon: Option<f64> = None;

        buffer.pop(); // remove newline char
        let message_parts = buffer.split(",");
        eprintln!("message_parts {:?}", message_parts);
        for (idx, part) in message_parts.enumerate() {
            eprintln!("{}:{}", idx, part);
            let coord = part
                .parse()
                .map_err(|error| IpcHandlerError::ParseMessageCoords {
                    error,
                    value: part.to_string(),
                    pos: idx,
                })?;
            if idx == 0 {
                start_lat = Some(coord);
            }
            if idx == 1 {
                start_lon = Some(coord);
            }
            if idx == 2 {
                finish_lat = Some(coord);
            }
            if idx == 3 {
                finish_lon = Some(coord);
            }
            if idx > 3 {
                return Err(IpcHandlerError::ParseMessageUnexpected {
                    pos: idx,
                    value: part.to_string(),
                });
            }
        }

        let start_lat = start_lat.ok_or(IpcHandlerError::ParseMessageMissingValue)?;
        let start_lon = start_lon.ok_or(IpcHandlerError::ParseMessageMissingValue)?;
        let finish_lat = finish_lat.ok_or(IpcHandlerError::ParseMessageMissingValue)?;
        let finish_lon = finish_lon.ok_or(IpcHandlerError::ParseMessageMissingValue)?;

        let arr = [1f64; 5];
        eprintln!("arr {:?}", arr);
        arr.iter().for_each(|v| {
            let sleep_sec = (v * start_lat) as u64;
            eprintln!("sleeping for {}", sleep_sec);
            thread::sleep(Duration::from_secs(sleep_sec));
        });

        // Now that the receive has come through and the client is waiting on the server's send, do
        // it. (`.get_mut()` is to get the sender, `BufReader` doesn't implement a pass-through
        // `Write`.)
        conn.get_mut()
            .write_all(b"Hello from server!\n")
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
