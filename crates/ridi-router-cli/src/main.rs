mod cli;
#[cfg(feature = "rmdf-viewer")]
mod debug;
mod file_naming;
mod gpx_writer;
mod json_writer;
mod map_data;
mod osm_data;
mod proximity;
mod result_writer;
mod rmdf;
mod router;
mod router_runner;
mod simd;
#[cfg(test)]
mod test_utils;

use std::{
    io::{self, IsTerminal},
    process,
};

use router_runner::RouterRunner;
use tracing::{error_span, Level};

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

    let span = error_span!("Process", service = "ridi-router-cli");
    let _entered = span.enter();
    let runner = RouterRunner::run();
    if let Err(runner) = runner {
        tracing::error!(error = ?runner, "Router startup failed");
        process::exit(1);
    }
}
