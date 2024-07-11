use std::{
    collections::HashMap,
    io::{self, BufRead},
    process,
};

use clap::{Parser, Subcommand};
use geo_types::Point;
use gps_coords_hash_map::{MapDataGraph, MapDataNode, MapDataWay};
use gpx::{write, Gpx, GpxVersion, Track, TrackSegment, Waypoint};
use osm::OsmData;

mod gps_coords_hash_map;
mod gps_hash;
mod osm;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    from_lat: f64,
    from_lon: f64,
    to_lat: f64,
    to_lon: f64,
}

fn main() {
    let cli = Cli::parse();

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
                eprintln!("writing {:?} {:?}", lat, lon);
            } else {
                eprintln!("Found node with missing coordinates");
                process::exit(1);
            }
        }
        if element.type_field == "way" {
            map_data.insert_way(MapDataWay {
                id: element.id,
                node_ids: element.nodes.clone(),
            });
        }
    }

    let mut track_segment = TrackSegment::new();

    let from_lat = cli.from_lat;
    let from_lon = cli.from_lon;

    let waypoint = Waypoint::new(Point::new(from_lat, from_lon));
    track_segment.points.push(waypoint);

    let closes_point = map_data.get_closest_to_coords(from_lat, from_lon);

    if let Some(point) = closes_point {
        let waypoint = Waypoint::new(Point::new(point.lat, point.lon));
        track_segment.points.push(waypoint);
    } else {
        eprintln!("no closest point found");
        process::exit(1);
    }

    // for p in 1..100 {
    //     let node = osm_data.elements.get(p).unwrap();
    //     let waypoint = Waypoint::new(Point::new(node.lon.unwrap(), node.lat.unwrap()));
    //     track_segment.points.push(waypoint);
    // }

    let mut track = Track::new();
    track.segments.push(track_segment);

    let mut gpx = Gpx::default();
    gpx.tracks.push(track);

    gpx.version = GpxVersion::Gpx11;

    write(&gpx, std::io::stdout()).unwrap();
}
