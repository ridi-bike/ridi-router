use std::{
    io::{self, BufRead},
    process,
    time::Instant,
};

use clap::{arg, value_parser, Command};

use gpx_writer::RoutesWriter;
use map_data_graph::{MapDataGraph, MapDataNode, MapDataWay, MapDataWayNodeIds};
use osm::OsmData;
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

    let mut map_data = MapDataGraph::new();
    let std_read_start = Instant::now();
    {
        let mut input_map_data: String = String::new();
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = line.expect("Could not read line from standard in");
            input_map_data.push_str(line.as_str());
        }

        let osm_data_result = serde_json::from_str::<OsmData>(&input_map_data);

        let osm_data = match osm_data_result {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Problem parsing osm data: {e}");
                process::exit(1);
            }
        };

        let std_read_duration = std_read_start.elapsed();
        eprintln!(
            "stdin read and serde took {} seconds",
            std_read_duration.as_secs()
        );

        let map_data_construct_start = Instant::now();

        for element in osm_data.elements.iter() {
            if element.type_field == "node" {
                if let (Some(lat), Some(lon)) = (element.lat, element.lon) {
                    map_data.insert_node(MapDataNode {
                        id: element.id,
                        lat,
                        lon,
                    });
                } else {
                    eprintln!("Found node with missing coordinates");
                    process::exit(1);
                }
            }
            if element.type_field == "way" {
                map_data
                    .insert_way(MapDataWay {
                        id: element.id,
                        node_ids: MapDataWayNodeIds::from_vec(element.nodes.clone()),
                        one_way: element.tags.as_ref().map_or(false, |tags| {
                            tags.oneway
                                .as_ref()
                                .map_or(false, |one_way| one_way == "yes")
                        }),
                    })
                    .unwrap();
            }
        }
        let map_data_construct_duration = map_data_construct_start.elapsed();
        eprintln!(
            "Map Data Construct took {} seconds",
            map_data_construct_duration.as_secs()
        );
    }

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
