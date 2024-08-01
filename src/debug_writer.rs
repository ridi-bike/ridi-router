use core::panic;
use std::{
    collections::HashMap,
    fs::{create_dir_all, File},
};

use geo::Point;
use gpx::{write, Gpx, GpxVersion, Track, TrackSegment, Waypoint};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    map_data_graph::MapDataPointRef,
    route::walker::{Route, RouteWalkerMoveResult},
};

pub struct DebugWriter {
    id: u64,
    step_id: u64,
    walker_id: u16,
    start_point: MapDataPointRef,
    dead_end_tracks: Vec<Track>,
    forks: HashMap<u64, Waypoint>,
    route: Route,
}

impl DebugWriter {
    pub fn new(walker_id: u16, start_point: MapDataPointRef) -> Self {
        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");

        Self {
            id: since_the_epoch.as_secs(),
            step_id: 0,
            walker_id,
            start_point,
            dead_end_tracks: Vec::new(),
            route: Route::new(),
            forks: HashMap::new(),
        }
    }

    pub fn log_move(&mut self, move_result: &RouteWalkerMoveResult, route: &Route) -> () {
        self.step_id += 1;
        let last_segment = route.get_segment_last();
        if let Some(last_segment) = last_segment {
            let point = last_segment.get_end_point();
            let mut waypoint = if let Some(wp) = self.forks.get(&point.borrow().id) {
                wp.clone()
            } else {
                Waypoint::new(Point::new(point.borrow().lon, point.borrow().lat))
            };
            let move_result_type = match move_result {
                RouteWalkerMoveResult::Finish => "Finish",
                RouteWalkerMoveResult::DeadEnd => "DeadEnd",
                RouteWalkerMoveResult::Fork(_) => "Fork",
            };
            let comment_data = format!("step: {}\nMoveResult: {}", self.step_id, move_result_type);
            waypoint.comment = if let Some(comment) = waypoint.comment {
                Some(format!("{}\n{}", comment, comment_data))
            } else {
                Some(format!("{}", comment_data))
            };
            self.forks.insert(point.borrow().id, waypoint);
        }
        let dead_track = self.dead_end_tracks.pop();
        if let Some(dead_track) = dead_track {
            if self.route.get_segment_count() > route.get_segment_count() {
                let mut track_segment = TrackSegment::new();
                let mut dead_segment_idx = route.get_segment_count() - 1;
                loop {
                    let dead_segment = self.route.get_segment_by_index(dead_segment_idx);
                    dead_segment_idx += 1;
                    if dead_segment_idx == self.route.get_segment_count() {
                        break;
                    }
                    if let Some(dead_segment) = dead_segment {
                        track_segment.points.push(Waypoint::new(Point::new(
                            dead_segment.get_end_point().borrow().lon,
                            dead_segment.get_end_point().borrow().lat,
                        )));
                    }
                }
                let mut dead_track = dead_track.clone();
                dead_track.segments.push(track_segment);
                self.dead_end_tracks.push(dead_track);
            } else if dead_track.segments.len() != 0 {
                self.dead_end_tracks.push(Track::new());
            }
        } else {
            self.dead_end_tracks.push(Track::new());
        }

        self.route = route.clone();
    }

    pub fn write_gpx(&self) -> () {
        let mut gpx = Gpx::default();
        gpx.version = GpxVersion::Gpx11;

        let mut track_segment = TrackSegment::new();

        let waypoint = Waypoint::new(Point::new(
            self.start_point.borrow().lon,
            self.start_point.borrow().lat,
        ));
        track_segment.points.push(waypoint);

        for segment in self.route.clone() {
            let waypoint = Waypoint::new(Point::new(
                segment.get_end_point().borrow().lon,
                segment.get_end_point().borrow().lat,
            ));
            track_segment.points.push(waypoint);
        }

        let mut track = Track::new();
        track.segments.push(track_segment);

        gpx.tracks.push(track);

        for wp in self.forks.clone() {
            gpx.waypoints.push(wp.1);
        }

        let dir = format!(
            "{}/moto-router/debug/{}",
            std::env::temp_dir().to_str().expect("Temp dir not found"),
            self.id,
        );

        eprintln!("writing log file {}/{}.gpx", dir, self.walker_id);

        create_dir_all(dir.clone()).expect("unable to create dirs");

        match File::create(format!("{}/{}.gpx", dir, self.walker_id)) {
            Ok(file) => write(&gpx, file).unwrap(),
            Err(error) => panic!("Debug write error {}", error),
        };
    }
}
