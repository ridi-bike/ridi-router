use std::{
    io::{self, BufRead},
    process,
    time::Instant,
};

use clap::{arg, value_parser, Command};

use gpx_writer::RoutesWriter;
use map_data_graph::{MapDataGraph, MapDataWay, MapDataWayPoints, OsmNode};
use osm::OsmData;
use osm_data_reader::read_osm_data;
use route::{
    navigator::RouteNavigator,
    weights::{weight_heading, weight_no_loops},
};

mod gps_hash;
mod gpx_writer;
mod map_data_graph;
mod osm;
mod osm_data_reader;
mod route;
#[cfg(test)]
mod test_utils;

fn main() {
    let matches = Command::new("gps-router")
        .arg(
            arg!(
                -f --from_lat <LAT> "From lat"
            )
            .value_parser(value_parser!(f64)),
        )
        .arg(
            arg!(
                -F --from_lon <LON> "From lon"
            )
            .value_parser(value_parser!(f64)),
        )
        .arg(
            arg!(
                -t --to_lat <LAT> "To lat"
            )
            .value_parser(value_parser!(f64)),
        )
        .arg(
            arg!(
                -T --to_lon <LON> "To lon"
            )
            .value_parser(value_parser!(f64)),
        )
        .get_matches();

    let map_data = read_osm_data().unwrap();

    let from_lat = matches.get_one::<f64>("from_lat").unwrap();
    let from_lon = matches.get_one::<f64>("from_lon").unwrap();
    let to_lat = matches.get_one::<f64>("to_lat").unwrap();
    let to_lon = matches.get_one::<f64>("to_lon").unwrap();

    let routes_generation_start = Instant::now();

    let start_point = match map_data.get_closest_to_coords(*from_lat, *from_lon) {
        None => {
            eprintln!("no closest point found");
            process::exit(1);
        }
        Some(p) => p,
    };

    let end_point = match map_data.get_closest_to_coords(*to_lat, *to_lon) {
        None => {
            eprintln!("no closest point found");
            process::exit(1);
        }
        Some(p) => p,
    };

    let mut navigator = RouteNavigator::new(
        &map_data,
        &start_point,
        &end_point,
        vec![weight_heading, weight_no_loops],
    );

    let routes = navigator.generate_routes();

    let routes_generation_duration = routes_generation_start.elapsed();
    eprintln!(
        "Routes generation took {} seconds",
        routes_generation_duration.as_secs()
    );

    let writer = RoutesWriter::new(start_point.clone(), routes, *from_lat, *from_lon, None);

    match writer.write_gpx() {
        Ok(()) => return (),
        Err(e) => eprintln!("Error on write: {:#?}", e),
    }
}
