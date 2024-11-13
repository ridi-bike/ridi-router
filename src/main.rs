use core::panic;

use gpx_writer::RoutesWriter;
use map_data::graph::MapDataGraph;
use router_mode::RouterMode;

use crate::router::generator::Generator;

mod debug_writer;
mod gps_hash;
mod gpx_writer;
mod map_data;
mod osm_data_reader;
mod osm_json_parser;
mod result_writer;
mod router;
mod router_mode;
#[cfg(test)]
mod test_utils;

fn main() {
    match RouterMode::get() {
        RouterMode::Dual { start_finish, .. } => {
            let start = match MapDataGraph::get()
                .get_closest_to_coords(start_finish.start_lat, start_finish.start_lon)
            {
                Some(p) => p,
                None => panic!("no closest point found"),
            };
            let finish = match MapDataGraph::get()
                .get_closest_to_coords(start_finish.finish_lat, start_finish.finish_lon)
            {
                Some(p) => p,
                None => panic!("no closest point found"),
            };
            let route_generator = Generator::new(start.clone(), finish.clone());
            let routes = route_generator.generate_routes();
            let writer = RoutesWriter::new(
                start.clone(),
                routes,
                start_finish.start_lat,
                start_finish.start_lon,
                None,
            );

            match writer.write_gpx() {
                Ok(()) => return (),
                Err(e) => panic!("Error on write: {:#?}", e),
            }
        }
        RouterMode::Server { .. } => {}
        RouterMode::Client { .. } => {}
    }
}
