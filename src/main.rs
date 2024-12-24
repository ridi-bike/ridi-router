use std::io;

use router_runner::RouterRunner;
use tracing::Level;

mod gpx_writer;
mod ipc_handler;
mod map_data;
mod map_data_cache;
mod osm_data_reader;
mod osm_json_parser;
mod result_writer;
mod router;
mod router_runner;
#[cfg(test)]
mod test_utils;

fn main() {
    let subscriber = tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_ansi(false)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(true)
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    RouterRunner::run().unwrap();
}
