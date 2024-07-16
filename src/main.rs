use std::{
    collections::HashMap,
    io::{self, BufRead},
    process,
};

use clap::{arg, command, value_parser, ArgAction, Command};

use geo_types::Point;
use gpx::{write, Gpx, GpxVersion, Track, TrackSegment, Waypoint};
use map_data_graph::{MapDataGraph, MapDataNode, MapDataWay};
use osm::OsmData;
use rand::Rng;

mod gps_hash;
mod map_data_graph;
mod osm;
mod route_creator;
mod test_data;

struct Cli {
    from_lat: f64,
    from_lon: f64,
    to_lat: f64,
    to_lon: f64,
}

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

    let mut map_data = MapDataGraph::new();

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
            map_data.insert_way(MapDataWay {
                id: element.id,
                node_ids: element.nodes.clone(),
                one_way: element.tags.as_ref().map_or(false, |tags| {
                    tags.oneway
                        .as_ref()
                        .map_or(false, |one_way| one_way == "yes")
                }),
            });
        }
    }

    let from_lat = matches.get_one::<f64>("from_lat").unwrap();
    let from_lon = matches.get_one::<f64>("from_lon").unwrap();

    let mut track_segment = TrackSegment::new();
    let waypoint = Waypoint::new(Point::new(*from_lon, *from_lat));
    track_segment.points.push(waypoint);

    let start_point = match map_data.get_closest_to_coords(*from_lat, *from_lon) {
        None => {
            eprintln!("no closest point found");
            process::exit(1);
        }
        Some(p) => p,
    };

    let waypoint = Waypoint::new(Point::new(start_point.lon, start_point.lat));
    track_segment.points.push(waypoint);

    let mut prev_point = start_point.clone();
    let mut visited_points = Vec::new();
    for step in 1..100000 {
        let adj_lines_points = map_data.get_adjacent(&prev_point);
        let adj_points = adj_lines_points
            .iter()
            .filter_map(|line_point| {
                let (_, point) = line_point;
                if point.id != start_point.id {
                    return Some(line_point);
                }
                None
            })
            .collect::<Vec<_>>();
        let adj_points = adj_points
            .iter()
            .filter(|(_, point)| {
                !visited_points[if visited_points.len() > 2 {
                    visited_points.len() - 3
                } else {
                    0
                }..if visited_points.len() > 0 {
                    visited_points.len() - 1
                } else {
                    0
                }]
                    .contains(&point.id)
            })
            .collect::<Vec<_>>();
        let idx = if adj_points.len() > 1 {
            rand::thread_rng().gen_range(0..adj_points.len() - 1)
        } else {
            0
        };
        let next_point = adj_points.get(idx);
        if let Some((_, next_point)) = next_point {
            visited_points.push(next_point.id);
            let waypoint = Waypoint::new(Point::new(next_point.lon, next_point.lat));
            track_segment.points.push(waypoint);
            prev_point = next_point.clone();
        }
    }

    let mut track = Track::new();
    track.segments.push(track_segment);

    let mut gpx = Gpx::default();
    gpx.tracks.push(track);

    gpx.version = GpxVersion::Gpx11;

    write(&gpx, std::io::stdout()).unwrap();
}
