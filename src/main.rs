use router_runner::RouterRunner;

mod debug_writer;
mod gps_hash;
mod gpx_writer;
mod ipc_handler;
mod map_data;
mod osm_data_reader;
mod osm_json_parser;
mod result_writer;
mod router;
mod router_runner;
#[cfg(test)]
mod test_utils;

fn main() {
    let router = RouterRunner::init();
    router.run();
}
