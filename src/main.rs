use std::{
    io::{self, IsTerminal},
    process,
};

use router_runner::RouterRunner;
use tracing::{error_span, Level};

mod debug;
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
    let subscriber = if std::io::stdin().is_terminal() {
        let subscriber = tracing_subscriber::fmt()
            .with_writer(io::stderr)
            .with_file(true)
            .with_line_number(true)
            .with_thread_names(true)
            .with_max_level(Level::INFO)
            .finish();

        tracing::subscriber::set_global_default(subscriber)
    } else {
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_writer(io::stderr)
            .with_file(true)
            .with_line_number(true)
            .with_thread_names(true)
            .with_max_level(Level::INFO)
            .finish();

        tracing::subscriber::set_global_default(subscriber)
    };

    if let Err(subscriber) = subscriber {
        tracing::error!(error = ?subscriber, "Subscriber setup failed");
        process::exit(1);
    }

    let span = error_span!("Service started", service = "ridi-router");
    let _entered = span.enter();
    let runner = RouterRunner::run();
    if let Err(runner) = runner {
        tracing::error!(error = ?runner, "Router startup failed");
        process::exit(1);
    }
}
