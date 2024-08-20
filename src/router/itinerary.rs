use std::{cell::RefCell, rc::Rc};

use crate::map_data::point::{MapDataPoint, MapDataPointRef};

#[derive(Clone, Debug)]
pub struct Itinerary {
    from: MapDataPointRef,
    to: MapDataPointRef,
    waypoints: Vec<MapDataPointRef>,
    next: MapDataPointRef,
    waypoint_radius: f64,
}

impl Itinerary {
    pub fn new(
        from: MapDataPointRef,
        to: MapDataPointRef,
        waypoints: Vec<MapDataPointRef>,
        waypoint_radius: f64,
    ) -> Self {
        Self {
            from,
            waypoint_radius,
            next: waypoints.get(0).map_or(Rc::clone(&to), |w| Rc::clone(&w)),
            waypoints,
            to,
        }
    }

    pub fn check_set_next(&mut self, current: MapDataPointRef) -> &MapDataPointRef {
        if current.borrow().distance_between(&self.next) <= self.waypoint_radius {
            if let Some(idx) = self.waypoints.iter().position(|w| w == &self.next) {
                self.next = self
                    .waypoints
                    .get(idx + 1)
                    .map_or(Rc::clone(&self.to), |w| Rc::clone(w))
            } else {
                self.next = Rc::clone(&self.to);
            }
        }
        &self.next
    }

    pub fn get_next(&self) -> &MapDataPointRef {
        &self.next
    }

    pub fn get_from(&self) -> &MapDataPointRef {
        &self.from
    }

    pub fn get_to(&self) -> &MapDataPointRef {
        &self.to
    }
}
