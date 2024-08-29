use core::panic;
use std::{process, sync::OnceLock, time::Instant};

use clap::{arg, value_parser, Command};

use gpx_writer::RoutesWriter;
use map_data::graph::MapDataGraph;

use crate::{cli::Cli, router::generator::Generator};

mod cli;
mod debug_writer;
mod gps_hash;
mod gpx_writer;
mod map_data;
mod osm_data_reader;
mod osm_json_parser;
mod router;
#[cfg(test)]
mod test_utils;

// riga-cesis-100km
// serde 60 sek (25+35), 13gb
// parse 33 sek, 600mb
// refs 29 sek, 630mb

pub static MAP_DATA_GRAPH: OnceLock<MapDataGraph> = OnceLock::new();

fn main() {
    let from_to = match Cli::get() {
        Cli::Single {
            data_source: _,
            from_to,
        } => from_to,
        cli => panic!("{:#?} not yet implemented", cli),
    };

    let routes_generation_start = Instant::now();

    let from = match MapDataGraph::get().get_closest_to_coords(from_to.from_lat, from_to.from_lon) {
        None => {
            eprintln!("no closest point found");
            process::exit(1);
        }
        Some(p) => p,
    };

    let to = match MapDataGraph::get().get_closest_to_coords(from_to.to_lat, from_to.to_lon) {
        None => {
            eprintln!("no closest point found");
            process::exit(1);
        }
        Some(p) => p,
    };

    let route_generator = Generator::new(from.clone(), to.clone());
    let routes = route_generator.generate_routes();

    let routes_generation_duration = routes_generation_start.elapsed();
    eprintln!(
        "Routes generation took {} seconds",
        routes_generation_duration.as_secs()
    );

    let writer = RoutesWriter::new(
        from.clone(),
        routes,
        from_to.from_lat,
        from_to.from_lon,
        None,
    );

    match writer.write_gpx() {
        Ok(()) => return (),
        Err(e) => eprintln!("Error on write: {:#?}", e),
    }
}
