pub mod segment;
pub mod segment_list;

use std::{collections::HashMap, hash::Hash};

use geo::{HaversineBearing, Point as GeoPoint};
use serde::{Deserialize, Serialize};

use crate::map_data::{line::MapDataLine, point::MapDataPoint};

use self::segment::Segment;

#[derive(Serialize, Deserialize, Debug)]
pub struct RouteStatElement {
    pub len_m: f64,
    pub percentage: f64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Point {
    pub lat: f64,
    pub lon: f64,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct RouteStats {
    pub len_m: f64,
    pub junction_count: u32,
    pub highway: HashMap<String, RouteStatElement>,
    pub surface: HashMap<String, RouteStatElement>,
    pub smoothness: HashMap<String, RouteStatElement>,
    pub mean_point: Point,
    pub direction_change_ratio: f64,
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
    pub fn get_steps_from_end(&self, num_of_steps: usize) -> Option<Segment> {
        if self.route_segments.len() < num_of_steps + 1 {
            return None;
        }
        self.route_segments
            .get(self.route_segments.len() - 1 - num_of_steps)
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
        let mut lat_sum: f64 = 0.;
        let mut lon_sum: f64 = 0.;
        let mut prev_bearing: Option<f32> = None;
        let mut tot_bearing_diff: f64 = 0.;
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

            lat_sum += segment.get_end_point().borrow().lat as f64;
            lon_sum += segment.get_end_point().borrow().lon as f64;

            let line = segment.get_line().borrow();
            let point_1 = line.points.0.borrow();
            let point_2 = line.points.1.borrow();
            let geo_point_1 = GeoPoint::new(point_1.lon, point_2.lat);
            let geo_point_2 = GeoPoint::new(point_2.lon, point_2.lat);
            let curr_bearing = if line.points.1 == *segment.get_end_point() {
                geo_point_1.haversine_bearing(geo_point_2)
            } else {
                geo_point_2.haversine_bearing(geo_point_1)
            };
            if let Some(prev_bearing) = prev_bearing {
                tot_bearing_diff += (prev_bearing - curr_bearing).abs() as f64;
            }
            prev_bearing = if segment.get_end_point().borrow().is_junction() {
                None
            } else {
                Some(curr_bearing)
            };
        }

        RouteStats {
            len_m,
            junction_count,
            highway: calc_stat_map(len_m, &highway),
            smoothness: calc_stat_map(len_m, &smoothness),
            surface: calc_stat_map(len_m, &surface),
            mean_point: Point {
                lat: (lat_sum / self.get_segment_count() as f64),
                lon: lon_sum / self.get_segment_count() as f64,
            },
            direction_change_ratio: tot_bearing_diff / len_m * 1000.,
        }
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
