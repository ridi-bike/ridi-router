use router_runner::RouterRunner;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

mod debug_writer;
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
    // a builder for `FmtSubscriber`.
    let subscriber = FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(Level::TRACE)
        // completes the builder.
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    let router = RouterRunner::init();
    router.run().unwrap();
}
