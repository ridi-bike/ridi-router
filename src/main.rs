use std::{
    collections::HashMap,
    io::{self, BufRead},
    process,
    rc::Rc,
};

use geo_types::Point;
use gpx::{write, Gpx, GpxVersion, Track, TrackSegment, Waypoint};
use osm::OsmData;

mod gps_hash;
mod osm;

type MapId = i64;

struct MapNode {
    id: MapId,
    map_ways: Vec<MapId>,
    lat: f64,
    lon: f64,
}

struct MapWay {
    id: MapId,
    map_nodes: Vec<MapId>,
}

fn main() {
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

    let mut ways = HashMap::new();
    let mut nodes = HashMap::new();
    // let mut coordinates =

    for element in osm_data.elements.iter() {
        if element.type_field == "node" {
            if let (Some(lat), Some(lon)) = (element.lat, element.lon) {
                nodes.insert(
                    element.id,
                    MapNode {
                        id: element.id,
                        map_ways: Vec::new(),
                        lat,
                        lon,
                    },
                );
            } else {
                eprintln!("Found node with missing coordinates");
                process::exit(1);
            }
        }
        if element.type_field == "way" {
            for node_id in element.nodes.iter() {
                let node = match nodes.get_mut(node_id) {
                    Some(d) => d,
                    None => {
                        eprintln!("Missing node - nodes aren't specified first or ways reference nodes that are not in the query");
                        process::exit(1);
                    }
                };
                node.map_ways.push(element.id);
            }
            ways.insert(
                element.id,
                MapWay {
                    id: element.id,
                    map_nodes: element.nodes.clone(),
                },
            );
        }
    }

    let mut track_segment = TrackSegment::new();

    for p in 1..100 {
        let node = osm_data.elements.get(p).unwrap();
        let waypoint = Waypoint::new(Point::new(node.lon.unwrap(), node.lat.unwrap()));
        track_segment.points.push(waypoint);
    }

    let mut track = Track::new();
    track.segments.push(track_segment);

    let mut gpx = Gpx::default();
    gpx.tracks.push(track);

    gpx.version = GpxVersion::Gpx11;

    write(&gpx, std::io::stdout()).unwrap();
}
