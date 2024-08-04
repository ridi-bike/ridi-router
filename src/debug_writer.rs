use core::panic;
use std::io::Write;
use std::{
    collections::HashMap,
    fs::{create_dir_all, File},
    rc::Rc,
};

use geo::Point;
use gpx::{write, Gpx, GpxVersion, Track, TrackSegment, Waypoint};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::route::navigator::WeightCalcResult;
use crate::route::walker::RouteSegment;
use crate::{
    map_data_graph::MapDataPointRef,
    route::{
        navigator::{DiscardedForkChoices, ForkWeights},
        walker::{Route, RouteSegmentList, RouteWalkerMoveResult},
        weights,
    },
};

#[derive(Debug, Clone)]
enum ForkAction {
    SetChoice(u64),
    MoveBack(Option<RouteSegment>, Option<RouteSegmentList>),
}

#[derive(Debug, Clone)]
struct StepData {
    last_segment: Option<RouteSegment>,
    fork_weights: Option<ForkWeights>,
    fork_action: Option<ForkAction>,
    discarded_point_ids: Option<Vec<u64>>,
    fork_choices: Option<RouteSegmentList>,
    fork_point_weights: Option<Vec<(u64, Vec<WeightCalcResult>)>>,
}

impl StepData {
    pub fn new() -> Self {
        Self {
            last_segment: None,
            fork_weights: None,
            fork_action: None,
            discarded_point_ids: None,
            fork_choices: None,
            fork_point_weights: None,
        }
    }
}

pub struct DebugWriter {
    id: u64,
    step_id: u64,
    walker_id: u16,
    start_point: MapDataPointRef,
    last_pont: MapDataPointRef,
    dead_end_tracks: Vec<Track>,
    forks: HashMap<u64, Waypoint>,
    route: Route,
    step_data: HashMap<u64, StepData>,
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
            last_pont: Rc::clone(&start_point),
            start_point,
            dead_end_tracks: Vec::new(),
            route: Route::new(),
            forks: HashMap::new(),
            step_data: HashMap::new(),
        }
    }

    pub fn log_weight(&mut self, point_id: u64, weights: Vec<WeightCalcResult>) -> () {
        if self.step_data.get(&self.step_id).is_none() {
            self.step_data.insert(self.step_id, StepData::new());
        }
        let step_data = self.step_data.get(&self.step_id);
        if let Some(step_data) = step_data {
            let mut step_data = step_data.clone();
            if let Some(ref mut point_weights) = step_data.fork_point_weights {
                point_weights.push((point_id, weights));
            } else {
                step_data.fork_point_weights = Some(vec![(point_id, weights)]);
            }
            self.step_data.insert(self.step_id, step_data);
        }
    }

    pub fn log_choices(&mut self, choices: &RouteSegmentList) -> () {
        if self.step_data.get(&self.step_id).is_none() {
            self.step_data.insert(self.step_id, StepData::new());
        }
        let step_data = self.step_data.get(&self.step_id);
        if let Some(step_data) = step_data {
            let mut step_data = step_data.clone();
            step_data.fork_choices = Some(choices.clone());
            self.step_data.insert(self.step_id, step_data);
        }
    }
    pub fn log_fork_action_choice(&mut self, id: u64) -> () {
        if self.step_data.get(&self.step_id).is_none() {
            self.step_data.insert(self.step_id, StepData::new());
        }
        let step_data = self.step_data.get(&self.step_id);
        if let Some(step_data) = step_data {
            let mut step_data = step_data.clone();
            step_data.fork_action = Some(ForkAction::SetChoice(id));
            self.step_data.insert(self.step_id, step_data);
        }
    }
    pub fn log_fork_action_back(
        &mut self,
        last_segment: Option<RouteSegment>,
        segment_list: Option<RouteSegmentList>,
    ) -> () {
        if self.step_data.get(&self.step_id).is_none() {
            self.step_data.insert(self.step_id, StepData::new());
        }
        let step_data = self.step_data.get(&self.step_id);
        if let Some(step_data) = step_data {
            let mut step_data = step_data.clone();
            step_data.fork_action = Some(ForkAction::MoveBack(last_segment, segment_list));
            self.step_data.insert(self.step_id, step_data);
        }
    }
    pub fn log_weights(&mut self, weights: &ForkWeights) -> () {
        if self.step_data.get(&self.step_id).is_none() {
            self.step_data.insert(self.step_id, StepData::new());
        }
        let step_data = self.step_data.get(&self.step_id);
        if let Some(step_data) = step_data {
            let mut step_data = step_data.clone();
            step_data.fork_weights = Some(weights.clone());
            self.step_data.insert(self.step_id, step_data);
        }
    }

    pub fn log_discarded(&mut self, discarded: &DiscardedForkChoices) -> () {
        if self.step_data.get(&self.step_id).is_none() {
            self.step_data.insert(self.step_id, StepData::new());
        }
        let step_data = self.step_data.get(&self.step_id);
        if let Some(step_data) = step_data {
            let mut step_data = step_data.clone();
            step_data.discarded_point_ids =
                discarded.get_discarded_choices_for_pont(&self.last_pont.borrow().id);
            self.step_data.insert(self.step_id, step_data);
        }
    }

    pub fn log_move(&mut self, move_result: &RouteWalkerMoveResult, route: &Route) -> () {
        self.step_id += 1;
        if self.step_data.get(&self.step_id).is_none() {
            self.step_data.insert(self.step_id, StepData::new());
        }
        let last_segment = route.get_segment_last();
        if let Some(last_segment) = last_segment {
            let point = last_segment.get_end_point();

            if let Some(step_data) = self.step_data.get(&self.step_id) {
                let mut step_data = step_data.clone();
                step_data.last_segment = Some(last_segment.clone());
                self.step_data.insert(self.step_id, step_data);
            }

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

        self.write();
    }

    fn get_dir(&self) -> String {
        let dir = format!(
            "{}/moto-router/debug/{}",
            std::env::temp_dir().to_str().expect("Temp dir not found"),
            self.id,
        );

        eprintln!("writing file {}/{}.(gpx|log)", dir, self.walker_id);

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

        for track in self.dead_end_tracks.clone() {
            gpx.tracks.push(track);
        }

        let dir = self.get_dir();

        match File::create(format!("{}/{}_{}.gpx", dir, self.walker_id, self.step_id)) {
            Ok(file) => write(&gpx, file).unwrap(),
            Err(error) => panic!("Debug write error {}", error),
        };
    }

    fn write_log(&self) -> () {
        let dir = self.get_dir();
        let mut log_file =
            match File::create(format!("{}/{}_{}.log", dir, self.walker_id, self.step_id)) {
                Ok(file) => file,
                Err(error) => panic!("Debug write error {}", error),
            };

        let mut step_data: Vec<_> = self.step_data.iter().collect();
        step_data.sort_by(|(step_num, _), (step_num2, _)| step_num.cmp(step_num2));

        for step_data in step_data {
            let (step_num, step_data) = step_data;
            writeln!(
                log_file,
                "\nStep num: {}\n\tLast Segment: {:?}\n\tFork Choices: {:?}\n\t Discarded Points: {:?}\n\tFork Point Weights: {:?}\n\tFork Weights: {:?}\n\t Fork Action: {:?}",
                step_num,
                step_data.last_segment,
                step_data.fork_choices,
                step_data.discarded_point_ids,
                step_data.fork_point_weights,
                step_data.fork_weights,
                step_data.fork_action
            )
            .expect("could not write log file");
        }
    }

    pub fn write(&self) -> () {
        self.write_log();
        self.write_gpx();
    }
}
