mod cli;
#[cfg(feature = "rmdf-viewer")]
mod debug;
mod file_naming;
mod gpx_writer;
mod json_writer;
mod progress_writer;
mod result_writer;
#[cfg(feature = "rmdf-viewer")]
mod rmdf;
mod router_runner;

use std::{
    io::{self, IsTerminal},
    process,
};

use cli::rendering::render_user_error;
use router_runner::RouterRunner;
use tracing::{error_span, Level};

#[hotpath::main(
    percentiles = [50, 95, 99],
    limit = 0,
    format = "none",
    report = "functions-timing,functions-alloc"
)]
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

    if let Err(error) = subscriber {
        eprintln!("Failed to initialize tracing subscriber: {error}");
        process::exit(1);
    }

    let span = error_span!("Process", service = "ridi-router-cli");
    let _entered = span.enter();

    if let Err(error) = RouterRunner::run() {
        eprintln!("{}", render_user_error(&error));
        process::exit(1);
    }
}
