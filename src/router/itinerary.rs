use std::fmt::Display;

use crate::map_data::graph::MapDataPointRef;

#[derive(Clone, Debug)]
pub struct WaypointHistoryElement {
    pub on_point: MapDataPointRef,
    pub from_point: MapDataPointRef,
}

#[derive(Clone, Debug)]
pub struct Itinerary {
    pub start: MapDataPointRef,
    pub finish: MapDataPointRef,
    pub waypoints: Vec<MapDataPointRef>,
    pub next: MapDataPointRef,
    pub waypoint_radius: f32,
    pub switched_wps_on: Vec<WaypointHistoryElement>,
    pub check_loop_since_last_wp: bool,
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
            next: waypoints.first().map_or(finish.clone(), |w| w.clone()),
            waypoints,
            finish,
            switched_wps_on: Vec::new(),
            check_loop_since_last_wp: false,
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
            next: waypoints.first().map_or(finish.clone(), |w| w.clone()),
            waypoints,
            finish,
            switched_wps_on: Vec::new(),
            check_loop_since_last_wp: true,
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

    pub fn get_point_loop_check_since(&self) -> Option<&MapDataPointRef> {
        if self.check_loop_since_last_wp {
            return self.switched_wps_on.last().map(|v| &v.on_point);
        }
        Some(&self.start)
    }

    pub fn is_finished(&self, current: MapDataPointRef) -> bool {
        if current == self.next && self.next == self.finish {
            return true;
        }
        false
    }

    pub fn check_set_next(&mut self, current: MapDataPointRef) -> bool {
        if self.next != self.finish
            && current.borrow().distance_between(&self.next) <= self.waypoint_radius
        {
            if let Some(idx) = self.waypoints.iter().position(|w| w == &self.next) {
                let prev_point = self.next.clone();
                self.next = self
                    .waypoints
                    .get(idx + 1)
                    .map_or(self.finish.clone(), |w| w.clone());
                self.switched_wps_on.push(WaypointHistoryElement {
                    on_point: current.clone(),
                    from_point: prev_point.clone(),
                });
            } else {
                self.switched_wps_on.push(WaypointHistoryElement {
                    on_point: current.clone(),
                    from_point: self.next.clone(),
                });
                self.next = self.finish.clone();
            }
            return true;
        }
        false
    }
    pub fn check_set_back(&mut self, current: MapDataPointRef) -> bool {
        if let Some(history) = self.switched_wps_on.last() {
            if history.on_point == current {
                self.next = history.from_point.clone();
                self.switched_wps_on.pop();
                return true;
            }
        }
        false
    }
}
