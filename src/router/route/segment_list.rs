use crate::map_data_graph::MapDataPointRef;
use std::{fmt::Debug, rc::Rc};

use super::segment::RouteSegment;

#[derive(PartialEq, Clone)]
pub struct RouteSegmentList {
    segment_list: Vec<RouteSegment>,
}

impl RouteSegmentList {
    pub fn new() -> Self {
        Self {
            segment_list: Vec::new(),
        }
    }
    pub fn get_segment_count(&self) -> usize {
        self.segment_list.len()
    }
    pub fn has_segment_with_point(&self, point: &MapDataPointRef) -> bool {
        self.segment_list
            .iter()
            .position(|route_segment| route_segment.get_end_point() == point)
            != None
    }
    pub fn get_all_segment_points(&self) -> Vec<MapDataPointRef> {
        self.segment_list
            .iter()
            .map(|segment| Rc::clone(&segment.get_end_point()))
            .collect()
    }
    pub fn get_segment_from_point(&self, point: &MapDataPointRef) -> Option<&RouteSegment> {
        self.segment_list
            .iter()
            .find(|segment| segment.get_end_point() == point)
    }
    pub fn exclude_segments_where_points_in(
        &self,
        points: &Vec<MapDataPointRef>,
    ) -> RouteSegmentList {
        self.segment_list
            .iter()
            .filter(|segment| !points.contains(&&segment.get_end_point()))
            .collect()
    }
    pub fn get_first_segment(&self) -> Option<&RouteSegment> {
        self.segment_list.get(0)
    }
}

impl IntoIterator for RouteSegmentList {
    type Item = RouteSegment;

    type IntoIter = std::vec::IntoIter<RouteSegment>;

    fn into_iter(self) -> Self::IntoIter {
        self.segment_list.into_iter()
    }
}

impl From<Vec<RouteSegment>> for RouteSegmentList {
    fn from(value: Vec<RouteSegment>) -> Self {
        Self {
            segment_list: value,
        }
    }
}

impl FromIterator<RouteSegment> for RouteSegmentList {
    fn from_iter<T: IntoIterator<Item = RouteSegment>>(iter: T) -> Self {
        RouteSegmentList {
            segment_list: iter.into_iter().collect(),
        }
    }
}
impl<'a> FromIterator<&'a RouteSegment> for RouteSegmentList {
    fn from_iter<T: IntoIterator<Item = &'a RouteSegment>>(iter: T) -> Self {
        RouteSegmentList {
            segment_list: Vec::from_iter(iter.into_iter().cloned()),
        }
    }
}
impl Debug for RouteSegmentList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RouteSegmentList {:#?}", self.segment_list)
    }
}
