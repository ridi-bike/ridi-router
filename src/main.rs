use std::{
    io::{self, BufRead},
    process,
};

use geo_types::Point;
use gpx::{write, Gpx, GpxVersion, Track, TrackSegment, Waypoint};
use osm::OsmData;

mod osm;

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
