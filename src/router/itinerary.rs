use std::fmt::Display;

use crate::map_data::graph::MapDataPointRef;

#[derive(Clone, Debug)]
pub struct Itinerary {
    start: MapDataPointRef,
    finish: MapDataPointRef,
    waypoints: Vec<MapDataPointRef>,
    next: MapDataPointRef,
    waypoint_radius: f32,
    visit_all_waypoints: bool,
}

impl Display for Itinerary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Itinerary({} - {} - {})",
            self.start,
            self.waypoints
                .iter()
                .map(|p| format!("{p}"))
                .collect::<Vec<_>>()
                .join(" - "),
            self.finish
        )
    }
}

impl Itinerary {
    pub fn new_start_finish(
        start: MapDataPointRef,
        finish: MapDataPointRef,
        waypoints: Vec<MapDataPointRef>,
        waypoint_radius: f32,
    ) -> Self {
        Self {
            start,
            waypoint_radius,
            next: waypoints.get(0).map_or(finish.clone(), |w| w.clone()),
            waypoints,
            finish,
            visit_all_waypoints: false,
        }
    }
    pub fn new_round_trip(
        start: MapDataPointRef,
        finish: MapDataPointRef,
        waypoints: Vec<MapDataPointRef>,
        waypoint_radius: f32,
    ) -> Self {
        Self {
            start,
            waypoint_radius,
            next: waypoints.get(0).map_or(finish.clone(), |w| w.clone()),
            waypoints,
            finish,
            visit_all_waypoints: true,
        }
    }

    pub fn id(&self) -> String {
        format!(
            "{}-{}-{}",
            self.start.borrow().id,
            self.waypoints
                .iter()
                .map(|p| format!("{}", p.borrow().id))
                .collect::<Vec<_>>()
                .join("-"),
            self.finish.borrow().id
        )
    }

    pub fn is_finished(&self, current: MapDataPointRef) -> bool {
        if current == self.next && self.next == self.finish {
            return true;
        }
        false
    }

    pub fn check_set_next(&mut self, current: MapDataPointRef) -> () {
        // if !self.visit_all_waypoints
        //     && current.borrow().distance_between(&self.finish)
        //         < current.borrow().distance_between(&self.next)
        // {
        //     self.next = self.finish.clone();
        // } else if current.borrow().distance_between(&self.next) <= self.waypoint_radius {
        if current.borrow().distance_between(&self.next) <= self.waypoint_radius {
            if let Some(idx) = self.waypoints.iter().position(|w| w == &self.next) {
                self.next = self
                    .waypoints
                    .get(idx + 1)
                    .map_or(self.finish.clone(), |w| w.clone())
            } else {
                self.next = self.finish.clone();
            }
        }
    }

    pub fn get_next(&self) -> &MapDataPointRef {
        &self.next
    }

    pub fn get_start(&self) -> &MapDataPointRef {
        &self.start
    }

    pub fn get_finish(&self) -> &MapDataPointRef {
        &self.finish
    }

    pub fn get_waypoints(&self) -> &Vec<MapDataPointRef> {
        &self.waypoints
    }
}
