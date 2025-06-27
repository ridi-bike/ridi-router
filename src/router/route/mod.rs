pub mod score;
pub mod segment;
pub mod segment_list;

use std::collections::HashMap;

use score::Score;
use serde::{Deserialize, Serialize};

use crate::map_data::{graph::MapDataPointRef, line::MapDataLine, point::MapDataPoint};

use self::segment::Segment;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RouteStatElement {
    pub len_m: f64,
    pub percentage: f64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Point {
    pub lat: f64,
    pub lon: f64,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RouteStats {
    pub len_m: f64,
    pub junction_count: u32,
    pub highway: HashMap<String, RouteStatElement>,
    pub surface: HashMap<String, RouteStatElement>,
    pub smoothness: HashMap<String, RouteStatElement>,
    pub score: f64,
    pub cluster: Option<usize>,
    pub approximated_route: Vec<(f32, f32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    route_segments: Vec<Segment>,
}

impl Route {
    pub fn new() -> Self {
        Route {
            route_segments: Vec::new(),
        }
    }
    pub fn get_route_chunk(&self, start: usize, end: usize) -> Vec<Segment> {
        self.route_segments[start..end].to_vec()
    }
    pub fn get_segment_last(&self) -> Option<&Segment> {
        self.route_segments.last()
    }
    pub fn get_segment_by_index(&self, idx: usize) -> Option<&Segment> {
        self.route_segments.get(idx)
    }
    pub fn get_segment_count(&self) -> usize {
        self.route_segments.len()
    }
    pub fn remove_last_segment(&mut self) -> Option<Segment> {
        self.route_segments.pop()
    }
    pub fn add_segment(&mut self, segment: Segment) {
        self.route_segments.push(segment)
    }

    pub fn split_at_point(&self, point: &MapDataPointRef) -> Self {
        let point_pos = self
            .route_segments
            .iter()
            .position(|seg| seg.get_end_point() == point)
            .map_or(0, |v| v);

        let route_segments = self.route_segments[point_pos..].to_vec();
        Self { route_segments }
    }

    pub fn get_route_chunk_since_junction_before_last(&self) -> Vec<Segment> {
        let idx_from = match self.get_segment_last() {
            None => 0,
            Some(last_segment) => self
                .route_segments
                .iter()
                .enumerate()
                .rev()
                .find(|(_idx, route_segment)| {
                    route_segment.get_end_point().borrow().is_junction()
                        && route_segment.get_end_point().borrow().id
                            != last_segment.get_end_point().borrow().id
                })
                .map_or(0, |v| v.0),
        };
        self.route_segments[idx_from..].to_vec()
    }
    pub fn get_junction_before_last_segment(&self) -> Option<&Segment> {
        match self.get_segment_last() {
            None => None,
            Some(last_segment) => self.route_segments.iter().rev().find(|route_segment| {
                route_segment.get_end_point().borrow().is_junction()
                    && route_segment.get_end_point().borrow().id
                        != last_segment.get_end_point().borrow().id
            }),
        }
    }
    pub fn has_looped(&self, since_point: Option<&MapDataPointRef>) -> bool {
        let since_point_pos = if let Some(since_point) = since_point {
            self.route_segments
                .iter()
                .position(|segment| segment.get_end_point() == since_point)
                .map_or(0, |p| p)
        } else {
            0
        };
        let last_segment = self.route_segments.last();
        if let Some(last_segment) = last_segment {
            let last_segment_point = last_segment.get_end_point();
            let last_segment_line_tags = last_segment.get_line().borrow().tags.borrow();
            let last_segment_line_hw_ref = last_segment_line_tags.hw_ref();
            let last_segment_line_name = last_segment_line_tags.name();
            let end_index = self.route_segments.len().checked_sub(1);
            if let Some(end_index) = end_index {
                let slice_len = self.route_segments[since_point_pos..end_index].len();
                return self.route_segments[since_point_pos..end_index]
                    .iter()
                    .enumerate()
                    .any(|(idx, segment)| {
                        // points are equal
                        // if points are less than 20m apart and
                        // there are at least 20 segments between points
                        // and hw ref or road name exist and match
                        // treat them as looped as they are proabaly two sides of a motorway or
                        // multi lane road with a direction separator
                        let segment_point = segment.get_end_point();
                        let are_points_eq = segment_point == last_segment_point;

                        let distance_between_points_over_threshold =
                            segment_point.borrow().distance_between(last_segment_point) < 50.;
                        let route_segments_between_points_over_threshold = slice_len - idx > 10;

                        let segment_line_tags = segment.get_line().borrow().tags.borrow();
                        let segment_line_hw_ref = segment_line_tags.hw_ref();
                        let segment_line_name = segment_line_tags.name();

                        are_points_eq
                            || (distance_between_points_over_threshold
                                && route_segments_between_points_over_threshold
                                && ((segment_line_hw_ref.is_some()
                                    && last_segment_line_hw_ref.is_some()
                                    && segment_line_hw_ref == last_segment_line_hw_ref)
                                    || (segment_line_name.is_some()
                                        && last_segment_line_name.is_some()
                                        && segment_line_name == last_segment_line_name)))
                    });
            }
        }
        false
    }
    pub fn is_back_on_road_within_distance(
        &self,
        hw_ref: Option<smartstring::alias::String>,
        hw_name: Option<smartstring::alias::String>,
        len_check_m: f32,
    ) -> bool {
        let mut len_tot_m = 0.;

        if hw_ref.is_none() && hw_name.is_none() {
            return false;
        }

        if let Some(last_route_segment) = self.get_segment_last() {
            if (last_route_segment
                .get_line()
                .borrow()
                .tags
                .borrow()
                .hw_ref()
                .is_some()
                && last_route_segment
                    .get_line()
                    .borrow()
                    .tags
                    .borrow()
                    .hw_ref()
                    == hw_ref.as_ref())
                || (last_route_segment
                    .get_line()
                    .borrow()
                    .tags
                    .borrow()
                    .name()
                    .is_some()
                    && last_route_segment.get_line().borrow().tags.borrow().name()
                        == hw_name.as_ref())
            {
                return false;
            }
        }

        let mut prev_segment: Option<&Segment> = None;
        for segment in self.iter().rev() {
            if let Some(prev_segment) = prev_segment {
                len_tot_m += prev_segment
                    .get_end_point()
                    .borrow()
                    .distance_between(segment.get_end_point());
                if (segment.get_line().borrow().tags.borrow().hw_ref().is_some()
                    && segment.get_line().borrow().tags.borrow().hw_ref() == hw_ref.as_ref())
                    || (segment.get_line().borrow().tags.borrow().name().is_some()
                        && segment.get_line().borrow().tags.borrow().name() == hw_name.as_ref())
                {
                    return len_check_m >= len_tot_m;
                }
            }
            if len_tot_m > len_check_m {
                return false;
            }
            prev_segment = Some(segment);
        }

        false
    }
    pub fn get_junctions_from_end(&self, num_of_junctions: usize) -> Option<Segment> {
        if self.route_segments.len() < num_of_junctions + 1 {
            return None;
        }

        let mut segment_num = 0;
        for segment in self.route_segments.iter().rev() {
            if segment.get_end_point().borrow().is_junction() {
                segment_num += 1;
            }
            if segment_num == num_of_junctions {
                return Some(segment.clone());
            }
        }

        None
    }
    pub fn get_segments_from_end(&self, num_of_segments: usize) -> Option<Segment> {
        if self.route_segments.len() < num_of_segments + 1 {
            return None;
        }
        self.route_segments
            .get(self.route_segments.len() - 1 - num_of_segments)
            .cloned()
    }

    pub fn calc_stats(&self) -> RouteStats {
        fn update_map(
            tag_val: &Option<&smartstring::alias::String>,
            line_len: f64,
            map: &mut HashMap<String, f64>,
        ) {
            if let Some(tag_val) = tag_val {
                if let Some(len) = map.get(tag_val.as_str()) {
                    map.insert(tag_val.to_string(), len + line_len);
                } else {
                    map.insert(tag_val.to_string(), line_len);
                }
            } else if let Some(len) = map.get("unknown") {
                map.insert("unknown".to_string(), len + line_len);
            } else {
                map.insert("unknown".to_string(), line_len);
            }
        }
        fn calc_stat_map(
            len_m: f64,
            map: &HashMap<String, f64>,
        ) -> HashMap<String, RouteStatElement> {
            let mut stat_map: HashMap<String, RouteStatElement> = HashMap::new();
            for (key, line_len) in map.iter() {
                stat_map.insert(
                    key.clone(),
                    RouteStatElement {
                        len_m: *line_len,
                        percentage: line_len / len_m * 100.,
                    },
                );
            }

            stat_map
        }
        let mut len_m: f64 = 0.;
        let mut junction_count = 0;
        let mut highway: HashMap<String, f64> = HashMap::new();
        let mut surface: HashMap<String, f64> = HashMap::new();
        let mut smoothness: HashMap<String, f64> = HashMap::new();

        for segment in &self.route_segments {
            let line_len: f64 = segment.get_line().borrow().get_len_m().into();
            len_m += line_len;
            if segment.get_end_point().borrow().is_junction() {
                junction_count += 1;
            }
            let line_tags = segment.get_line().borrow().tags.borrow();
            let highway_val = line_tags.highway();
            update_map(&highway_val, line_len, &mut highway);
            let surface_val = line_tags.surface();
            update_map(&surface_val, line_len, &mut surface);
            let smoothness_val = line_tags.smoothness();
            update_map(&smoothness_val, line_len, &mut smoothness);
        }

        RouteStats {
            len_m,
            junction_count,
            highway: calc_stat_map(len_m, &highway),
            smoothness: calc_stat_map(len_m, &smoothness),
            surface: calc_stat_map(len_m, &surface),
            score: Score::calc_score(self),
            cluster: None,
            approximated_route: Vec::new(),
        }
    }

    pub fn iter(&self) -> std::slice::Iter<Segment> {
        self.route_segments.iter()
    }
}

impl From<Vec<Segment>> for Route {
    fn from(route_segments: Vec<Segment>) -> Self {
        Route { route_segments }
    }
}

impl FromIterator<(MapDataLine, MapDataPoint)> for Route {
    fn from_iter<T: IntoIterator<Item = (MapDataLine, MapDataPoint)>>(iter: T) -> Self {
        iter.into_iter().collect::<Route>()
    }
}

impl IntoIterator for Route {
    type Item = Segment;

    type IntoIter = std::vec::IntoIter<Segment>;

    fn into_iter(self) -> Self::IntoIter {
        self.route_segments.into_iter()
    }
}
