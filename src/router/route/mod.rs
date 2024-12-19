pub mod score;
pub mod segment;
pub mod segment_list;

use std::{collections::HashMap, hash::Hash};

use geo::{HaversineBearing, Point as GeoPoint};
use score::Score;
use serde::{Deserialize, Serialize};

use crate::map_data::{line::MapDataLine, point::MapDataPoint};

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
    pub fn add_segment(&mut self, segment: Segment) -> () {
        self.route_segments.push(segment)
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
                    .distance_between(&segment.get_end_point());
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

        let mut junction_num = 0;
        let mut segment_num = 0;
        let mut segment: Option<Segment> = None;
        while junction_num < num_of_junctions {
            segment = self
                .route_segments
                .get(self.route_segments.len() - segment_num - 1)
                .cloned();
            if let Some(ref segment) = segment {
                if segment.get_end_point().borrow().is_junction() {
                    junction_num += 1;
                }
                segment_num += 1;
            } else {
                return None;
            }
        }
        segment
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
        ) -> () {
            if let Some(tag_val) = tag_val {
                if let Some(len) = map.get(tag_val.as_str()) {
                    map.insert(tag_val.to_string(), len + line_len);
                } else {
                    map.insert(tag_val.to_string(), line_len);
                }
            } else {
                if let Some(len) = map.get("unknown") {
                    map.insert("unknown".to_string(), len + line_len);
                } else {
                    map.insert("unknown".to_string(), line_len);
                }
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
                        len_m: line_len.clone(),
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
            score: Score::calc_score(&self),
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
