use crate::map_data_graph::{MapDataLine, MapDataPoint};

use self::segment::RouteSegment;

pub mod navigator;
pub mod segment;
pub mod segment_list;
pub mod walker;
pub mod weights;

#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    route_segments: Vec<RouteSegment>,
}

impl Route {
    pub fn new() -> Self {
        Route {
            route_segments: Vec::new(),
        }
    }
    pub fn get_segment_last(&self) -> Option<&RouteSegment> {
        self.route_segments.last()
    }
    pub fn get_segment_by_index(&self, idx: usize) -> Option<&RouteSegment> {
        self.route_segments.get(idx)
    }
    pub fn get_segment_count(&self) -> usize {
        self.route_segments.len()
    }
    pub fn remove_last_segment(&mut self) -> Option<RouteSegment> {
        self.route_segments.pop()
    }
    pub fn add_segment(&mut self, segment: RouteSegment) -> () {
        self.route_segments.push(segment)
    }
    pub fn get_junction_before_last_segment(&self) -> Option<&RouteSegment> {
        match self.get_segment_last() {
            None => None,
            Some(last_segment) => self.route_segments.iter().rev().find(|route_segment| {
                route_segment.get_end_point().borrow().junction == true
                    && route_segment.get_end_point().borrow().id
                        != last_segment.get_end_point().borrow().id
            }),
        }
    }
    pub fn has_looped(&self) -> bool {
        let last_segment = self.route_segments.last();
        if let Some(last_segment) = last_segment {
            let end_index = self.route_segments.len().checked_sub(1);
            if let Some(end_index) = end_index {
                return self.route_segments[..end_index].iter().any(|segment| {
                    segment.get_end_point().borrow().id == last_segment.get_end_point().borrow().id
                });
            }
        }
        false
    }
    pub fn get_steps_from_end(&self, num_of_steps: usize) -> Option<RouteSegment> {
        if self.route_segments.len() < num_of_steps + 1 {
            return None;
        }
        self.route_segments
            .get(self.route_segments.len() - 1 - num_of_steps)
            .cloned()
    }
}

impl From<Vec<RouteSegment>> for Route {
    fn from(route_segments: Vec<RouteSegment>) -> Self {
        Route { route_segments }
    }
}

impl FromIterator<(MapDataLine, MapDataPoint)> for Route {
    fn from_iter<T: IntoIterator<Item = (MapDataLine, MapDataPoint)>>(iter: T) -> Self {
        iter.into_iter().collect::<Route>()
    }
}

impl IntoIterator for Route {
    type Item = RouteSegment;

    type IntoIter = std::vec::IntoIter<RouteSegment>;

    fn into_iter(self) -> Self::IntoIter {
        self.route_segments.into_iter()
    }
}
