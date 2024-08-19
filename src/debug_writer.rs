use core::panic;
use std::fs::OpenOptions;
use std::io::Write;
use std::{
    collections::HashMap,
    fs::{create_dir_all, File},
    rc::Rc,
};

use geo::Point;
use gpx::{write, Gpx, GpxVersion, Track, TrackSegment, Waypoint};
use rand::Rng;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::map_data_graph::MapDataPointRef;
use crate::router::route::Route;
use crate::router::walker::RouteWalkerMoveResult;

pub trait DebugLogger {
    fn log_move(&mut self, move_result: &RouteWalkerMoveResult, route: &Route) -> ();
    fn log_step(&mut self) -> ();
    fn log(&self, msg: String) -> ();
    fn split(&self) -> Box<dyn DebugLogger>;
}

#[derive(Clone, Default)]
pub struct DebugLoggerVoidSink;

impl DebugLogger for DebugLoggerVoidSink {
    fn log_move(&mut self, _move_result: &RouteWalkerMoveResult, _route: &Route) -> () {}

    fn log_step(&mut self) -> () {}

    fn log(&self, _msg: String) -> () {}

    fn split(&self) -> Box<dyn DebugLogger> {
        Box::new(Self)
    }
}

#[derive(Clone)]
pub struct DebugLoggerFileSink {
    gpx_every_n: u64,
    id: u64,
    step_id: u64,
    walker_id: u64,
    start_point: MapDataPointRef,
    end_point: MapDataPointRef,
    last_pont: MapDataPointRef,
    dead_end_tracks: Vec<Track>,
    forks: HashMap<u64, Waypoint>,
    route: Route,
}

impl DebugLogger for DebugLoggerFileSink {
    fn split(&self) -> Box<dyn DebugLogger> {
        let mut rng = rand::thread_rng();
        let walker_id = (rng.gen::<f64>() * 10000000.0) as u64;
        Box::new(Self {
            gpx_every_n: self.gpx_every_n,
            id: self.id,
            step_id: self.step_id,
            walker_id,
            last_pont: Rc::clone(&self.last_pont),
            start_point: Rc::clone(&self.start_point),
            end_point: Rc::clone(&self.end_point),
            dead_end_tracks: self.dead_end_tracks.clone(),
            route: self.route.clone(),
            forks: self.forks.clone(),
        })
    }
    fn log(&self, msg: String) -> () {
        let msg = msg.replace("\n", "\t\n");
        self.log_to_file(format!("\t{}", msg));
    }

    fn log_step(&mut self) -> () {
        self.step_id += 1;
        self.log_to_file(format!("Step: {}", self.step_id));
    }

    fn log_move(&mut self, move_result: &RouteWalkerMoveResult, route: &Route) -> () {
        self.log_to_file(format!("\tLast Point: {:#?}", route.get_segment_last()));
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
            let comment_data =
                format!("Step: {}\nMoveResult: {}\n", self.step_id, move_result_type);
            waypoint.name = Some(point.borrow().id.to_string());
            waypoint.comment = if let Some(comment) = waypoint.comment {
                Some(format!("{}\n{}", comment, comment_data))
            } else {
                Some(format!("{}", comment_data))
            };
            self.forks.insert(point.borrow().id, waypoint);
            self.last_pont = Rc::clone(&point);
        }

        if self.dead_end_tracks.last().is_none() {
            self.dead_end_tracks.push(Track::new());
        }

        let dead_track = self.dead_end_tracks.pop();
        if let Some(dead_track) = dead_track {
            if self.route.get_segment_count() > route.get_segment_count() {
                let mut track_segment = TrackSegment::new();
                let mut dead_segment_idx = route.get_segment_count() - 1;
                loop {
                    let dead_segment = self.route.get_segment_by_index(dead_segment_idx);
                    dead_segment_idx += 1;
                    if dead_segment_idx > self.route.get_segment_count() {
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
                self.dead_end_tracks.push(dead_track);
                self.dead_end_tracks.push(Track::new());
            }
        }
        self.route = route.clone();

        if self.step_id % self.gpx_every_n == 0 {
            self.write_gpx();
        }
    }
}

impl DebugLoggerFileSink {
    pub fn new(gpx_every_n: u64, start_point: MapDataPointRef, end_point: MapDataPointRef) -> Self {
        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");
        let mut rng = rand::thread_rng();
        let walker_id = (rng.gen::<f64>() * 10000000.0) as u64;
        Self {
            gpx_every_n,
            id: since_the_epoch.as_secs(),
            step_id: 0,
            walker_id,
            last_pont: Rc::clone(&start_point),
            start_point,
            end_point,
            dead_end_tracks: Vec::new(),
            route: Route::new(),
            forks: HashMap::new(),
        }
    }

    fn get_dir(&self) -> String {
        let dir = format!(
            "{}/moto-router/debug/{}",
            std::env::temp_dir().to_str().expect("Temp dir not found"),
            self.id,
        );

        create_dir_all(dir.clone()).expect("unable to create dirs");

        dir
    }

    fn write_gpx(&self) -> () {
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

        let mut end_point = Waypoint::new(Point::new(
            self.end_point.borrow().lon,
            self.end_point.borrow().lat,
        ));
        end_point.name = Some(String::from("END"));
        gpx.waypoints.push(end_point);

        for track in self.dead_end_tracks.clone() {
            gpx.tracks.push(track);
        }

        let dir = self.get_dir();

        match File::create(format!("{}/{}_{}.gpx", dir, self.walker_id, self.step_id)) {
            Ok(file) => write(&gpx, file).unwrap(),
            Err(error) => panic!("Debug write error {}", error),
        };
    }

    fn log_to_file(&self, msg: String) -> () {
        let dir = self.get_dir();
        let file_path = format!("{}/{}_logfile.log", dir, self.walker_id);
        let mut log_file = match File::create_new(file_path.clone()) {
            Ok(file) => file,
            Err(_) => match OpenOptions::new().append(true).open(file_path) {
                Err(error) => panic!("cant open for appending: {}", error),
                Ok(file) => file,
            },
        };

        writeln!(log_file, "{}", msg).expect("could not write log file");
    }
}
