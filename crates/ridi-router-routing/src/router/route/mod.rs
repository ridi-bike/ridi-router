pub mod score;
pub mod segment;
pub mod segment_list;

use std::collections::HashMap;

use geo::Distance;

use score::Score;
use serde::{Deserialize, Serialize};

use crate::{
    map_data::{graph::MapDataPointRef, line::MapDataLine, point::MapDataPoint},
    router::rules::RouterRules,
    RoutingContext,
};

use self::segment::Segment;

const LOOP_DISTANCE_THRESHOLD: f32 = 50.;
const LOOP_SEGMENT_THESHOLD: usize = 10;

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
                    route_segment.get_end_point().get().is_junction()
                        && route_segment.get_end_point().get().id
                            != last_segment.get_end_point().get().id
                })
                .map_or(0, |v| v.0),
        };
        self.route_segments[idx_from..].to_vec()
    }

    pub fn get_route_chunk_since_junction_before_last_with_context(
        &self,
        ctx: &RoutingContext<'_>,
    ) -> Vec<Segment> {
        let idx_from = match self.get_segment_last() {
            None => 0,
            Some(last_segment) => {
                let last_point_id = ctx.point(last_segment.get_end_point()).id;
                self.route_segments
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_idx, route_segment)| {
                        let point = ctx.point(route_segment.get_end_point());
                        point.is_junction() && point.id != last_point_id
                    })
                    .map_or(0, |value| value.0)
            }
        };
        self.route_segments[idx_from..].to_vec()
    }
    pub fn get_junction_before_last_segment(&self) -> Option<&Segment> {
        match self.get_segment_last() {
            None => None,
            Some(last_segment) => self.route_segments.iter().rev().find(|route_segment| {
                route_segment.get_end_point().get().is_junction()
                    && route_segment.get_end_point().get().id
                        != last_segment.get_end_point().get().id
            }),
        }
    }

    pub fn get_junction_before_last_segment_with_context(
        &self,
        ctx: &RoutingContext<'_>,
    ) -> Option<&Segment> {
        match self.get_segment_last() {
            None => None,
            Some(last_segment) => {
                let last_point_id = ctx.point(last_segment.get_end_point()).id;
                self.route_segments.iter().rev().find(|route_segment| {
                    let point = ctx.point(route_segment.get_end_point());
                    point.is_junction() && point.id != last_point_id
                })
            }
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
            let last_segment_line_tags = last_segment.get_line().get().tags.get();
            let last_segment_line_hw_ref = last_segment_line_tags.hw_ref();
            let last_segment_line_name = last_segment_line_tags.name();
            let end_index = self.route_segments.len().checked_sub(1);
            if let Some(end_index) = end_index {
                let slice_len = self.route_segments[since_point_pos..end_index].len();
                return self.route_segments[since_point_pos..end_index]
                    .iter()
                    .enumerate()
                    .any(|(idx, segment)| {
                        // if points are equal or
                        // if points are less than 20m apart and
                        // there are at least 20 segments between points
                        // and hw ref or road name exist and match
                        // treat them as looped as they are proabaly two sides of a motorway or
                        // multi lane road with a direction separator
                        let segment_point = segment.get_end_point();
                        let are_points_eq = segment_point == last_segment_point;

                        let distance_between_points_over_threshold =
                            segment_point.get().distance_between(last_segment_point)
                                < LOOP_DISTANCE_THRESHOLD;
                        let route_segments_between_points_over_threshold =
                            slice_len - idx > LOOP_SEGMENT_THESHOLD;

                        let segment_line_tags = segment.get_line().get().tags.get();
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

    pub fn has_looped_with_context(
        &self,
        ctx: &RoutingContext<'_>,
        since_point: Option<&MapDataPointRef>,
    ) -> bool {
        let since_point_pos = if let Some(since_point) = since_point {
            self.route_segments
                .iter()
                .position(|segment| segment.get_end_point() == since_point)
                .map_or(0, |position| position)
        } else {
            0
        };
        let last_segment = self.route_segments.last();
        if let Some(last_segment) = last_segment {
            let last_segment_point = last_segment.get_end_point();
            let last_segment_line = ctx.line(last_segment.get_line());
            let last_segment_line_tags = ctx.tag_set(&last_segment_line.tags);
            let last_segment_line_hw_ref = ctx.tag_value(&last_segment_line_tags.hw_ref);
            let last_segment_line_name = ctx.tag_value(&last_segment_line_tags.name);
            let end_index = self.route_segments.len().checked_sub(1);
            if let Some(end_index) = end_index {
                let slice_len = self.route_segments[since_point_pos..end_index].len();
                return self.route_segments[since_point_pos..end_index]
                    .iter()
                    .enumerate()
                    .any(|(idx, segment)| {
                        let segment_point_ref = segment.get_end_point();
                        let are_points_eq = segment_point_ref == last_segment_point;
                        let segment_point = ctx.point(segment_point_ref);
                        let last_point = ctx.point(last_segment_point);
                        let segment_point_geo = geo::Point::new(segment_point.lon, segment_point.lat);
                        let last_point_geo = geo::Point::new(last_point.lon, last_point.lat);
                        let distance_between_points_over_threshold =
                            geo::Haversine.distance(segment_point_geo, last_point_geo)
                                < LOOP_DISTANCE_THRESHOLD;
                        let route_segments_between_points_over_threshold =
                            slice_len - idx > LOOP_SEGMENT_THESHOLD;

                        let segment_line = ctx.line(segment.get_line());
                        let segment_line_tags = ctx.tag_set(&segment_line.tags);
                        let segment_line_hw_ref = ctx.tag_value(&segment_line_tags.hw_ref);
                        let segment_line_name = ctx.tag_value(&segment_line_tags.name);

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
        hw_ref: Option<String>,
        hw_name: Option<String>,
        len_check_m: f32,
    ) -> bool {
        let mut len_tot_m = 0.;

        if hw_ref.is_none() && hw_name.is_none() {
            return false;
        }

        if let Some(last_route_segment) = self.get_segment_last() {
            if (last_route_segment
                .get_line()
                .get()
                .tags
                .get()
                .hw_ref()
                .is_some()
                && last_route_segment.get_line().get().tags.get().hw_ref() == hw_ref)
                || (last_route_segment
                    .get_line()
                    .get()
                    .tags
                    .get()
                    .name()
                    .is_some()
                    && last_route_segment.get_line().get().tags.get().name() == hw_name)
            {
                return false;
            }
        }

        let mut prev_segment: Option<&Segment> = None;
        for segment in self.iter().rev() {
            if let Some(prev_segment) = prev_segment {
                len_tot_m += prev_segment
                    .get_end_point()
                    .get()
                    .distance_between(segment.get_end_point());
                if (segment.get_line().get().tags.get().hw_ref().is_some()
                    && segment.get_line().get().tags.get().hw_ref() == hw_ref)
                    || (segment.get_line().get().tags.get().name().is_some()
                        && segment.get_line().get().tags.get().name() == hw_name)
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

    pub fn is_back_on_road_within_distance_with_context(
        &self,
        ctx: &RoutingContext<'_>,
        hw_ref: Option<String>,
        hw_name: Option<String>,
        len_check_m: f32,
    ) -> bool {
        let mut len_tot_m = 0.;

        if hw_ref.is_none() && hw_name.is_none() {
            return false;
        }

        if let Some(last_route_segment) = self.get_segment_last() {
            let last_line = ctx.line(last_route_segment.get_line());
            let last_tags = ctx.tag_set(&last_line.tags);
            let last_hw_ref = ctx.tag_value(&last_tags.hw_ref);
            let last_name = ctx.tag_value(&last_tags.name);
            if (last_hw_ref.is_some() && last_hw_ref == hw_ref)
                || (last_name.is_some() && last_name == hw_name)
            {
                return false;
            }
        }

        let mut prev_segment: Option<&Segment> = None;
        for segment in self.iter().rev() {
            if let Some(prev_segment) = prev_segment {
                let prev_point = ctx.point(prev_segment.get_end_point());
                let current_point = ctx.point(segment.get_end_point());
                let prev_geo = geo::Point::new(prev_point.lon, prev_point.lat);
                let current_geo = geo::Point::new(current_point.lon, current_point.lat);
                len_tot_m += geo::Haversine.distance(prev_geo, current_geo);

                let line = ctx.line(segment.get_line());
                let tags = ctx.tag_set(&line.tags);
                let segment_hw_ref = ctx.tag_value(&tags.hw_ref);
                let segment_name = ctx.tag_value(&tags.name);
                if (segment_hw_ref.is_some() && segment_hw_ref == hw_ref)
                    || (segment_name.is_some() && segment_name == hw_name)
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
            if segment.get_end_point().get().is_junction() {
                segment_num += 1;
            }
            if segment_num == num_of_junctions {
                return Some(segment.clone());
            }
        }

        None
    }

    pub fn get_junctions_from_end_with_context(
        &self,
        ctx: &RoutingContext<'_>,
        num_of_junctions: usize,
    ) -> Option<Segment> {
        if self.route_segments.len() < num_of_junctions + 1 {
            return None;
        }

        let mut segment_num = 0;
        for segment in self.route_segments.iter().rev() {
            if ctx.point(segment.get_end_point()).is_junction() {
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

    pub fn calc_stats(&self, rules: &RouterRules) -> RouteStats {
        fn update_map(tag_val: &Option<String>, line_len: f64, map: &mut HashMap<String, f64>) {
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
            let line_len: f64 = segment.get_line().get().get_len_m().into();
            len_m += line_len;
            if segment.get_end_point().get().is_junction() {
                junction_count += 1;
            }
            let line_tags = segment.get_line().get().tags.get();
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
            score: Score::calc_score(self, rules),
            cluster: None,
            approximated_route: Vec::new(),
        }
    }

    pub fn calc_stats_with_context(
        &self,
        ctx: &RoutingContext<'_>,
        rules: &RouterRules,
    ) -> RouteStats {
        fn update_map(tag_val: &Option<String>, line_len: f64, map: &mut HashMap<String, f64>) {
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
            let line = ctx.line(segment.get_line());
            let point_a = ctx.point(&line.points.0);
            let point_b = ctx.point(&line.points.1);
            let point_a_geo = geo::Point::new(point_a.lon, point_a.lat);
            let point_b_geo = geo::Point::new(point_b.lon, point_b.lat);
            let line_len: f64 = geo::Haversine.distance(point_a_geo, point_b_geo).into();
            len_m += line_len;
            if ctx.point(segment.get_end_point()).is_junction() {
                junction_count += 1;
            }
            let line_tags = ctx.tag_set(&line.tags);
            let highway_val = ctx.tag_value(&line_tags.highway);
            update_map(&highway_val, line_len, &mut highway);
            let surface_val = ctx.tag_value(&line_tags.surface);
            update_map(&surface_val, line_len, &mut surface);
            let smoothness_val = ctx.tag_value(&line_tags.smoothness);
            update_map(&smoothness_val, line_len, &mut smoothness);
        }

        RouteStats {
            len_m,
            junction_count,
            highway: calc_stat_map(len_m, &highway),
            smoothness: calc_stat_map(len_m, &smoothness),
            surface: calc_stat_map(len_m, &surface),
            score: Score::calc_score_with_context(ctx, self, rules),
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
