use std::{fmt::Debug, rc::Rc};

use crate::map_data::graph::MapDataPointRef;

use super::segment::Segment;

#[derive(PartialEq, Clone)]
pub struct SegmentList {
    segment_list: Vec<Segment>,
}

impl SegmentList {
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
            .map(|segment| segment.get_end_point().clone())
            .collect()
    }
    pub fn get_segment_from_point(&self, point: &MapDataPointRef) -> Option<&Segment> {
        self.segment_list
            .iter()
            .find(|segment| segment.get_end_point() == point)
    }
    pub fn exclude_segments_where_points_in(&self, points: &Vec<MapDataPointRef>) -> SegmentList {
        self.segment_list
            .iter()
            .filter(|segment| !points.contains(&&segment.get_end_point()))
            .collect()
    }
    pub fn get_first_segment(&self) -> Option<&Segment> {
        self.segment_list.get(0)
    }
}

impl Into<Vec<Segment>> for SegmentList {
    fn into(self) -> Vec<Segment> {
        self.segment_list
    }
}

impl IntoIterator for SegmentList {
    type Item = Segment;

    type IntoIter = std::vec::IntoIter<Segment>;

    fn into_iter(self) -> Self::IntoIter {
        self.segment_list.into_iter()
    }
}

impl From<Vec<Segment>> for SegmentList {
    fn from(value: Vec<Segment>) -> Self {
        Self {
            segment_list: value,
        }
    }
}

impl FromIterator<Segment> for SegmentList {
    fn from_iter<T: IntoIterator<Item = Segment>>(iter: T) -> Self {
        SegmentList {
            segment_list: iter.into_iter().collect(),
        }
    }
}
impl<'a> FromIterator<&'a Segment> for SegmentList {
    fn from_iter<T: IntoIterator<Item = &'a Segment>>(iter: T) -> Self {
        SegmentList {
            segment_list: Vec::from_iter(iter.into_iter().cloned()),
        }
    }
}
impl Debug for SegmentList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RouteSegmentList {:#?}", self.segment_list)
    }
}
