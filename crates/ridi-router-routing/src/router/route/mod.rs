pub mod score;
pub mod segment;
pub mod segment_list;

use std::collections::HashMap;

use score::Score;
use serde::{Deserialize, Serialize};
use smartstring::alias::String as SmartString;

use crate::{
    map_data::{graph::MapDataPointRef, line::MapDataLine, point::MapDataPoint},
    router::rules::RouterRules,
    RoutingContext,
};

use self::segment::Segment;

const LOOP_DISTANCE_THRESHOLD: f32 = 50.;
const LOOP_SEGMENT_THRESHOLD: usize = 10;
// Roughly 33 m of latitude per bucket. That keeps nearby-loop candidate sets small while
// a 3x3 neighbor-cell search still comfortably covers the 50 m loop threshold.
const LOOP_CELL_SIZE_DEGREES: f32 = 0.0003;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RoadKey(SmartString);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CellId {
    lat_bucket: i32,
    lon_bucket: i32,
}

impl CellId {
    fn from_lat_lon(lat: f32, lon: f32) -> Self {
        Self {
            lat_bucket: (lat / LOOP_CELL_SIZE_DEGREES).floor() as i32,
            lon_bucket: (lon / LOOP_CELL_SIZE_DEGREES).floor() as i32,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct LoopMeta {
    end_point: MapDataPointRef,
    point_id: u64,
    lat: f32,
    lon: f32,
    hw_ref: Option<RoadKey>,
    name: Option<RoadKey>,
    cell_id: CellId,
}

impl LoopMeta {
    fn from_segment(ctx: &RoutingContext<'_>, segment: &Segment) -> Self {
        let end_point = segment.get_end_point().clone();
        let point = ctx.point(&end_point);
        let line = ctx.line(segment.get_line());
        let line_tags = ctx.tag_set(&line.tags);
        let hw_ref = ctx
            .tag_value(&line_tags.hw_ref)
            .map(|value| RoadKey(value.into()));
        let name = ctx
            .tag_value(&line_tags.name)
            .map(|value| RoadKey(value.into()));
        let cell_id = CellId::from_lat_lon(point.lat, point.lon);

        Self {
            end_point,
            point_id: point.id,
            lat: point.lat,
            lon: point.lon,
            hw_ref,
            name,
            cell_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
struct LoopDetector {
    // One meta entry per route segment, same index as route_segments.
    metas: Vec<LoopMeta>,

    // Exact point revisit lookup.
    point_hits: HashMap<MapDataPointRef, Vec<usize>>,

    // Spatial indexes scoped by road identity.
    hw_ref_cells: HashMap<RoadKey, HashMap<CellId, Vec<usize>>>,
    name_cells: HashMap<RoadKey, HashMap<CellId, Vec<usize>>>,
}

impl LoopDetector {
    fn push_meta(&mut self, meta: LoopMeta) {
        let idx = self.metas.len();
        self.point_hits
            .entry(meta.end_point.clone())
            .or_default()
            .push(idx);

        if let Some(hw_ref) = meta.hw_ref.clone() {
            self.hw_ref_cells
                .entry(hw_ref)
                .or_default()
                .entry(meta.cell_id)
                .or_default()
                .push(idx);
        }

        if let Some(name) = meta.name.clone() {
            self.name_cells
                .entry(name)
                .or_default()
                .entry(meta.cell_id)
                .or_default()
                .push(idx);
        }

        self.metas.push(meta);
    }

    fn push_segment(&mut self, ctx: &RoutingContext<'_>, segment: &Segment) {
        self.push_meta(LoopMeta::from_segment(ctx, segment));
    }

    fn pop(&mut self) {
        let Some(meta) = self.metas.pop() else {
            return;
        };
        let popped_idx = self.metas.len();

        Self::remove_point_hit(&mut self.point_hits, &meta.end_point, popped_idx);

        if let Some(hw_ref) = meta.hw_ref {
            Self::remove_cell_hit(&mut self.hw_ref_cells, &hw_ref, meta.cell_id, popped_idx);
        }

        if let Some(name) = meta.name {
            Self::remove_cell_hit(&mut self.name_cells, &name, meta.cell_id, popped_idx);
        }
    }

    fn has_looped(&self, since_point: Option<&MapDataPointRef>) -> bool {
        let Some((current_idx, current)) =
            self.metas.len().checked_sub(1).and_then(|current_idx| {
                self.metas
                    .get(current_idx)
                    .map(|current| (current_idx, current))
            })
        else {
            return false;
        };

        // Preserve the current since_point behavior for compatibility: we start at the first
        // matching point in route history. That matches the existing `.position(...)` logic,
        // even though it may not line up with the original waypoint-transition intent when the
        // same point appears multiple times.
        let since_idx = since_point
            .and_then(|point| self.point_hits.get(point))
            .and_then(|indices| indices.first().copied())
            .unwrap_or(0);

        if self
            .point_hits
            .get(&current.end_point)
            .is_some_and(|indices| {
                indices
                    .iter()
                    .any(|&candidate_idx| candidate_idx < current_idx && candidate_idx >= since_idx)
            })
        {
            return true;
        }

        self.has_nearby_same_road_loop(current_idx, current, since_idx)
    }

    fn has_nearby_same_road_loop(
        &self,
        current_idx: usize,
        current: &LoopMeta,
        since_idx: usize,
    ) -> bool {
        self.has_nearby_same_road_key_loop(
            current_idx,
            current,
            since_idx,
            current.hw_ref.as_ref(),
            &self.hw_ref_cells,
            |candidate| current.hw_ref.is_some() && candidate.hw_ref == current.hw_ref,
        ) || self.has_nearby_same_road_key_loop(
            current_idx,
            current,
            since_idx,
            current.name.as_ref(),
            &self.name_cells,
            |candidate| current.name.is_some() && candidate.name == current.name,
        )
    }

    fn has_nearby_same_road_key_loop(
        &self,
        current_idx: usize,
        current: &LoopMeta,
        since_idx: usize,
        road_key: Option<&RoadKey>,
        cells_by_key: &HashMap<RoadKey, HashMap<CellId, Vec<usize>>>,
        road_matches: impl Fn(&LoopMeta) -> bool,
    ) -> bool {
        let Some(road_key) = road_key else {
            return false;
        };
        let Some(cells) = cells_by_key.get(road_key) else {
            return false;
        };

        let neighbor_cells = Self::neighbor_cells(current.cell_id);
        for cell_id in neighbor_cells {
            let Some(candidate_indices) = cells.get(&cell_id) else {
                continue;
            };
            for &candidate_idx in candidate_indices {
                if candidate_idx < since_idx || candidate_idx >= current_idx {
                    continue;
                }
                if current_idx - candidate_idx <= LOOP_SEGMENT_THRESHOLD {
                    continue;
                }

                let candidate = &self.metas[candidate_idx];
                if !road_matches(candidate) {
                    continue;
                }
                if Self::squared_distance_m(current, candidate)
                    < LOOP_DISTANCE_THRESHOLD * LOOP_DISTANCE_THRESHOLD
                {
                    return true;
                }
            }
        }

        false
    }

    fn neighbor_cells(cell_id: CellId) -> [CellId; 9] {
        let mut neighbors = [cell_id; 9];
        let mut idx = 0;
        for lat_offset in -1..=1 {
            for lon_offset in -1..=1 {
                neighbors[idx] = CellId {
                    lat_bucket: cell_id.lat_bucket + lat_offset,
                    lon_bucket: cell_id.lon_bucket + lon_offset,
                };
                idx += 1;
            }
        }
        neighbors
    }

    fn squared_distance_m(a: &LoopMeta, b: &LoopMeta) -> f32 {
        // This intentionally uses a cheap local planar approximation instead of haversine.
        // For this detector we only compare very small distances against a 50 m threshold,
        // so the approximation error is not meaningful while the lower cost helps keep the
        // loop check hot path tiny.
        let mean_lat_rad = ((a.lat + b.lat) * 0.5).to_radians();
        let meters_per_degree_lat = 111_320.0_f32;
        let meters_per_degree_lon = meters_per_degree_lat * mean_lat_rad.cos();
        let dy_m = (a.lat - b.lat) * meters_per_degree_lat;
        let dx_m = (a.lon - b.lon) * meters_per_degree_lon;
        dx_m * dx_m + dy_m * dy_m
    }

    fn remove_cell_hit<K>(
        map: &mut HashMap<K, HashMap<CellId, Vec<usize>>>,
        key: &K,
        cell_id: CellId,
        idx: usize,
    ) where
        K: std::cmp::Eq + std::hash::Hash,
    {
        let mut remove_key = false;
        if let Some(cells) = map.get_mut(key) {
            let mut remove_cell = false;
            if let Some(indices) = cells.get_mut(&cell_id) {
                if indices.last().copied() == Some(idx) {
                    indices.pop();
                } else if let Some(pos) = indices.iter().rposition(|candidate| *candidate == idx) {
                    indices.remove(pos);
                }
                remove_cell = indices.is_empty();
            }
            if remove_cell {
                cells.remove(&cell_id);
            }
            remove_key = cells.is_empty();
        }
        if remove_key {
            map.remove(key);
        }
    }

    fn remove_point_hit<K>(map: &mut HashMap<K, Vec<usize>>, key: &K, idx: usize)
    where
        K: std::cmp::Eq + std::hash::Hash,
    {
        let mut remove_key = false;
        if let Some(indices) = map.get_mut(key) {
            if indices.last().copied() == Some(idx) {
                indices.pop();
            } else if let Some(pos) = indices.iter().rposition(|candidate| *candidate == idx) {
                indices.remove(pos);
            }
            remove_key = indices.is_empty();
        }
        if remove_key {
            map.remove(key);
        }
    }
}

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
    loop_detector: LoopDetector,
}

impl Route {
    pub fn new() -> Self {
        Route {
            route_segments: Vec::new(),
            loop_detector: LoopDetector::default(),
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
        let segment = self.route_segments.pop();
        if segment.is_some() {
            self.loop_detector.pop();
        }
        segment
    }
    pub fn add_segment(&mut self, ctx: &RoutingContext<'_>, segment: Segment) {
        self.loop_detector.push_segment(ctx, &segment);
        self.route_segments.push(segment)
    }

    pub fn split_at_point(&self, point: &MapDataPointRef) -> Self {
        let point_pos = self
            .route_segments
            .iter()
            .position(|seg| seg.get_end_point() == point)
            .map_or(0, |v| v);

        let route_segments = self.route_segments[point_pos..].to_vec();
        Self::from(route_segments)
    }

    pub fn get_route_chunk_since_junction_before_last(
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
    pub fn get_junction_before_last_segment(&self, ctx: &RoutingContext<'_>) -> Option<&Segment> {
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
    #[hotpath::measure]
    pub fn has_looped(
        &self,
        _ctx: &RoutingContext<'_>,
        since_point: Option<&MapDataPointRef>,
    ) -> bool {
        self.loop_detector.has_looped(since_point)
    }
    #[hotpath::measure]
    pub fn is_back_on_road_within_distance(
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
                len_tot_m += prev_point.distance_between(&current_point);

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
    pub fn get_junctions_from_end(
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

    #[hotpath::measure]
    pub fn calc_stats(&self, ctx: &RoutingContext<'_>, rules: &RouterRules) -> RouteStats {
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
            let line_len: f64 = line.len_m(&point_a, &point_b).into();
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
            score: Score::calc_score(ctx, self, rules),
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
        // Routes built from a pre-existing segment list do not have a RoutingContext available
        // at construction time, so phase 3 only hydrates detector metadata on incremental push.
        let loop_detector = LoopDetector::default();
        Route {
            route_segments,
            loop_detector,
        }
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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs, path::PathBuf, sync::OnceLock};

    use ridi_router_common::format::{LineRecord, PointRecord, TagSetRecord};
    use ridi_router_test_support::rmdf::{
        empty_neighbors, manifest_bounds, unique_test_dir, write_manifest, write_tile,
        TileManifest, TileMetadata, TileSpec, SYNTHETIC_TILE_BOUNDS, SYNTHETIC_TILE_ID,
        SYNTHETIC_TILE_SIZE_DEGREES,
    };

    use crate::{
        map_data::graph::{MapDataGraph, MapDataLineRef, MapDataPointRef},
        routing_context::RoutingContext,
    };

    use super::{segment::Segment, CellId, RoadKey, Route, LOOP_DISTANCE_THRESHOLD};

    static LOOP_FIXTURE_DIR: OnceLock<PathBuf> = OnceLock::new();

    const SOURCE_POINT_ID: u64 = 1;
    const EXACT_A_POINT_ID: u64 = 2;
    const EXACT_B_POINT_ID: u64 = 3;
    const HW_REF_A_POINT_ID: u64 = 10;
    const HW_REF_CLOSE_POINT_ID: u64 = 11;
    const HW_REF_OUTSIDE_POINT_ID: u64 = 12;
    const HW_REF_OTHER_CLOSE_POINT_ID: u64 = 13;
    const HW_REF_NONE_CLOSE_POINT_ID: u64 = 14;
    const NAME_A_POINT_ID: u64 = 20;
    const NAME_CLOSE_POINT_ID: u64 = 21;
    const NAME_OTHER_CLOSE_POINT_ID: u64 = 22;
    const NAME_NONE_CLOSE_POINT_ID: u64 = 23;
    const SINCE_POINT_ID: u64 = 30;
    const FILLER_START_POINT_ID: u64 = 40;
    const FILLER_COUNT: usize = 10;

    const EXACT_A_LINE_IDX: u64 = 0;
    const EXACT_B_LINE_IDX: u64 = 1;
    const HW_REF_A_LINE_IDX: u64 = 2;
    const HW_REF_CLOSE_LINE_IDX: u64 = 3;
    const HW_REF_OUTSIDE_LINE_IDX: u64 = 4;
    const HW_REF_OTHER_CLOSE_LINE_IDX: u64 = 5;
    const HW_REF_NONE_CLOSE_LINE_IDX: u64 = 6;
    const NAME_A_LINE_IDX: u64 = 7;
    const NAME_CLOSE_LINE_IDX: u64 = 8;
    const NAME_NONE_CLOSE_LINE_IDX: u64 = 10;
    const SINCE_LINE_IDX: u64 = 11;
    const FILLER_START_LINE_IDX: u64 = 12;

    fn loop_fixture_dir() -> PathBuf {
        LOOP_FIXTURE_DIR
            .get_or_init(|| create_loop_fixture("route-has-looped"))
            .clone()
    }

    fn create_loop_fixture(prefix: &str) -> PathBuf {
        fn point_record(
            osm_id: u64,
            lat: f32,
            lon: f32,
            lines_offset: u64,
            lines_count: u32,
        ) -> PointRecord {
            PointRecord {
                osm_id,
                lat,
                lon,
                lines_offset,
                lines_count,
                _padding1: 0,
                rules_offset: 0,
                rules_count: 0,
                flags: 0,
                _padding2: 0,
            }
        }

        fn line_record(
            point_a_osm_id: u64,
            point_a_lat: f32,
            point_a_lon: f32,
            point_b_osm_id: u64,
            point_b_lat: f32,
            point_b_lon: f32,
            tag_set_index: u32,
        ) -> LineRecord {
            LineRecord {
                point_a_osm_id,
                point_a_lat,
                point_a_lon,
                point_b_osm_id,
                point_b_lat,
                point_b_lon,
                direction: 0,
                _padding1: 0,
                _padding2: 0,
                tag_set_index,
            }
        }

        let dir = unique_test_dir(prefix);
        fs::create_dir_all(&dir).unwrap();

        let points = vec![
            (SOURCE_POINT_ID, 10.5000, 20.5000),
            (EXACT_A_POINT_ID, 10.1000, 20.1000),
            (EXACT_B_POINT_ID, 10.2000, 20.1000),
            (HW_REF_A_POINT_ID, 10.3000, 20.1000),
            (HW_REF_CLOSE_POINT_ID, 10.3003, 20.1000),
            (HW_REF_OUTSIDE_POINT_ID, 10.3006, 20.1000),
            (HW_REF_OTHER_CLOSE_POINT_ID, 10.3003, 20.1002),
            (HW_REF_NONE_CLOSE_POINT_ID, 10.3003, 20.1004),
            (NAME_A_POINT_ID, 10.4000, 20.1000),
            (NAME_CLOSE_POINT_ID, 10.4003, 20.1000),
            (NAME_OTHER_CLOSE_POINT_ID, 10.4003, 20.1002),
            (NAME_NONE_CLOSE_POINT_ID, 10.4003, 20.1004),
            (SINCE_POINT_ID, 10.6000, 20.1000),
            (40, 10.7000, 20.1000),
            (41, 10.7100, 20.1000),
            (42, 10.7200, 20.1000),
            (43, 10.7300, 20.1000),
            (44, 10.7400, 20.1000),
            (45, 10.7500, 20.1000),
            (46, 10.7600, 20.1000),
            (47, 10.7700, 20.1000),
            (48, 10.7800, 20.1000),
            (49, 10.7900, 20.1000),
        ];

        let point_lookup: HashMap<u64, (f32, f32)> = points
            .iter()
            .map(|(id, lat, lon)| (*id, (*lat, *lon)))
            .collect();

        let mut lines: Vec<(u64, u64, u32)> = vec![
            (SOURCE_POINT_ID, EXACT_A_POINT_ID, 0),
            (SOURCE_POINT_ID, EXACT_B_POINT_ID, 0),
            (SOURCE_POINT_ID, HW_REF_A_POINT_ID, 1),
            (SOURCE_POINT_ID, HW_REF_CLOSE_POINT_ID, 1),
            (SOURCE_POINT_ID, HW_REF_OUTSIDE_POINT_ID, 1),
            (SOURCE_POINT_ID, HW_REF_OTHER_CLOSE_POINT_ID, 2),
            (SOURCE_POINT_ID, HW_REF_NONE_CLOSE_POINT_ID, 0),
            (SOURCE_POINT_ID, NAME_A_POINT_ID, 3),
            (SOURCE_POINT_ID, NAME_CLOSE_POINT_ID, 3),
            (SOURCE_POINT_ID, NAME_OTHER_CLOSE_POINT_ID, 4),
            (SOURCE_POINT_ID, NAME_NONE_CLOSE_POINT_ID, 0),
            (SOURCE_POINT_ID, SINCE_POINT_ID, 0),
        ];
        for offset in 0..FILLER_COUNT as u64 {
            lines.push((SOURCE_POINT_ID, FILLER_START_POINT_ID + offset, 0));
        }

        let line_records: Vec<LineRecord> = lines
            .iter()
            .map(|(point_a, point_b, tag_set_index)| {
                let (point_a_lat, point_a_lon) = point_lookup[point_a];
                let (point_b_lat, point_b_lon) = point_lookup[point_b];
                line_record(
                    *point_a,
                    point_a_lat,
                    point_a_lon,
                    *point_b,
                    point_b_lat,
                    point_b_lon,
                    *tag_set_index,
                )
            })
            .collect();

        let mut point_to_lines: HashMap<u64, Vec<u64>> = HashMap::new();
        for (line_idx, (point_a, point_b, _)) in lines.iter().enumerate() {
            let line_idx = line_idx as u64;
            point_to_lines.entry(*point_a).or_default().push(line_idx);
            point_to_lines.entry(*point_b).or_default().push(line_idx);
        }

        let mut line_refs = Vec::new();
        let point_records: Vec<PointRecord> = points
            .iter()
            .map(|(id, lat, lon)| {
                let refs = point_to_lines.get(id).cloned().unwrap_or_default();
                let record =
                    point_record(*id, *lat, *lon, line_refs.len() as u64, refs.len() as u32);
                line_refs.extend(refs);
                record
            })
            .collect();

        let tag_values = vec![
            "R1".to_string(),
            "R2".to_string(),
            "Main".to_string(),
            "Other".to_string(),
        ];
        let tag_sets = vec![
            TagSetRecord {
                name_idx: TagSetRecord::NONE,
                hw_ref_idx: TagSetRecord::NONE,
                highway_idx: TagSetRecord::NONE,
                surface_idx: TagSetRecord::NONE,
                smoothness_idx: TagSetRecord::NONE,
            },
            TagSetRecord {
                name_idx: TagSetRecord::NONE,
                hw_ref_idx: 0,
                highway_idx: TagSetRecord::NONE,
                surface_idx: TagSetRecord::NONE,
                smoothness_idx: TagSetRecord::NONE,
            },
            TagSetRecord {
                name_idx: TagSetRecord::NONE,
                hw_ref_idx: 1,
                highway_idx: TagSetRecord::NONE,
                surface_idx: TagSetRecord::NONE,
                smoothness_idx: TagSetRecord::NONE,
            },
            TagSetRecord {
                name_idx: 2,
                hw_ref_idx: TagSetRecord::NONE,
                highway_idx: TagSetRecord::NONE,
                surface_idx: TagSetRecord::NONE,
                smoothness_idx: TagSetRecord::NONE,
            },
            TagSetRecord {
                name_idx: 3,
                hw_ref_idx: TagSetRecord::NONE,
                highway_idx: TagSetRecord::NONE,
                surface_idx: TagSetRecord::NONE,
                smoothness_idx: TagSetRecord::NONE,
            },
        ];

        let tile_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: SYNTHETIC_TILE_ID,
                bounds: SYNTHETIC_TILE_BOUNDS,
                spatial_index: Vec::new(),
                points: point_records,
                lines: line_records,
                line_refs,
                tag_values,
                tag_sets,
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        write_manifest(
            &dir,
            &TileManifest {
                version: "test".to_string(),
                tile_size_degrees: SYNTHETIC_TILE_SIZE_DEGREES,
                format_version: 1,
                generated_at: "2026-04-06T00:00:00Z".to_string(),
                source_files: vec!["synthetic".to_string()],
                tiles: vec![TileMetadata {
                    filename: SYNTHETIC_TILE_ID.to_filename(),
                    col: SYNTHETIC_TILE_ID.col,
                    row: SYNTHETIC_TILE_ID.row,
                    bounds: manifest_bounds(SYNTHETIC_TILE_BOUNDS),
                    neighbors: empty_neighbors(),
                    size_bytes: fs::metadata(&tile_path).unwrap().len(),
                    point_count: points.len() as u64,
                    line_count: lines.len() as u64,
                    checksum: format!("sha256:{prefix}"),
                    military_geojson_filename: None,
                }],
            },
        );

        dir
    }

    fn loop_test_graph() -> MapDataGraph {
        let dir = loop_fixture_dir();
        let manifest: TileManifest =
            serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap();
        MapDataGraph::open(dir, manifest)
    }

    fn point_ref(point_id: u64) -> MapDataPointRef {
        MapDataPointRef::new(SYNTHETIC_TILE_ID, point_id)
    }

    fn segment(line_idx: u64, point_id: u64) -> Segment {
        Segment::new(
            MapDataLineRef::new(SYNTHETIC_TILE_ID, line_idx),
            point_ref(point_id),
        )
    }

    fn route_from_specs(ctx: &RoutingContext<'_>, specs: &[(u64, u64)]) -> Route {
        let mut route = Route::new();
        for (line_idx, point_id) in specs {
            route.add_segment(ctx, segment(*line_idx, *point_id));
        }
        route
    }

    fn filler_specs(count: usize) -> Vec<(u64, u64)> {
        (0..count)
            .map(|offset| {
                (
                    FILLER_START_LINE_IDX + offset as u64,
                    FILLER_START_POINT_ID + offset as u64,
                )
            })
            .collect()
    }

    #[test]
    fn exact_endpoint_revisit_returns_true() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let route = route_from_specs(
            &ctx,
            &[
                (EXACT_A_LINE_IDX, EXACT_A_POINT_ID),
                (EXACT_B_LINE_IDX, EXACT_B_POINT_ID),
                (EXACT_A_LINE_IDX, EXACT_A_POINT_ID),
            ],
        );

        assert!(route.has_looped(&ctx, None));
    }

    #[test]
    fn exact_endpoint_revisit_respects_since_point() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let route = route_from_specs(
            &ctx,
            &[
                (EXACT_A_LINE_IDX, EXACT_A_POINT_ID),
                (EXACT_B_LINE_IDX, EXACT_B_POINT_ID),
                (EXACT_A_LINE_IDX, EXACT_A_POINT_ID),
            ],
        );

        assert!(!route.has_looped(&ctx, Some(&point_ref(EXACT_B_POINT_ID))));
    }

    #[test]
    fn since_point_none_scans_from_start() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let route = route_from_specs(
            &ctx,
            &[
                (EXACT_A_LINE_IDX, EXACT_A_POINT_ID),
                (EXACT_B_LINE_IDX, EXACT_B_POINT_ID),
                (EXACT_A_LINE_IDX, EXACT_A_POINT_ID),
            ],
        );

        assert!(route.has_looped(&ctx, None));
    }

    #[test]
    fn since_point_present_once_behaves_as_expected() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let route = route_from_specs(
            &ctx,
            &[
                (EXACT_A_LINE_IDX, EXACT_A_POINT_ID),
                (EXACT_B_LINE_IDX, EXACT_B_POINT_ID),
                (EXACT_A_LINE_IDX, EXACT_A_POINT_ID),
            ],
        );

        assert!(!route.has_looped(&ctx, Some(&point_ref(EXACT_B_POINT_ID))));
    }

    #[test]
    fn same_hw_ref_and_close_points_returns_true() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let mut specs = vec![(HW_REF_A_LINE_IDX, HW_REF_A_POINT_ID)];
        specs.extend(filler_specs(FILLER_COUNT));
        specs.push((HW_REF_CLOSE_LINE_IDX, HW_REF_CLOSE_POINT_ID));
        let route = route_from_specs(&ctx, &specs);

        assert!(route.has_looped(&ctx, None));
    }

    #[test]
    fn same_name_and_close_points_returns_true() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let mut specs = vec![(NAME_A_LINE_IDX, NAME_A_POINT_ID)];
        specs.extend(filler_specs(FILLER_COUNT));
        specs.push((NAME_CLOSE_LINE_IDX, NAME_CLOSE_POINT_ID));
        let route = route_from_specs(&ctx, &specs);

        assert!(route.has_looped(&ctx, None));
    }

    #[test]
    fn same_close_points_with_different_road_identity_returns_false() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let mut specs = vec![(HW_REF_A_LINE_IDX, HW_REF_A_POINT_ID)];
        specs.extend(filler_specs(FILLER_COUNT));
        specs.push((HW_REF_OTHER_CLOSE_LINE_IDX, HW_REF_OTHER_CLOSE_POINT_ID));
        let route = route_from_specs(&ctx, &specs);

        assert!(!route.has_looped(&ctx, None));
    }

    #[test]
    fn only_one_side_having_hw_ref_does_not_match() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let mut specs = vec![(HW_REF_A_LINE_IDX, HW_REF_A_POINT_ID)];
        specs.extend(filler_specs(FILLER_COUNT));
        specs.push((HW_REF_NONE_CLOSE_LINE_IDX, HW_REF_NONE_CLOSE_POINT_ID));
        let route = route_from_specs(&ctx, &specs);

        assert!(!route.has_looped(&ctx, None));
    }

    #[test]
    fn only_one_side_having_name_does_not_match() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let mut specs = vec![(NAME_A_LINE_IDX, NAME_A_POINT_ID)];
        specs.extend(filler_specs(FILLER_COUNT));
        specs.push((NAME_NONE_CLOSE_LINE_IDX, NAME_NONE_CLOSE_POINT_ID));
        let route = route_from_specs(&ctx, &specs);

        assert!(!route.has_looped(&ctx, None));
    }

    #[test]
    fn segments_with_index_gap_at_threshold_do_not_count_as_loop() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let mut specs = vec![(HW_REF_A_LINE_IDX, HW_REF_A_POINT_ID)];
        specs.extend(filler_specs(FILLER_COUNT - 1));
        specs.push((HW_REF_CLOSE_LINE_IDX, HW_REF_CLOSE_POINT_ID));
        let route = route_from_specs(&ctx, &specs);

        assert!(!route.has_looped(&ctx, None));
    }

    #[test]
    fn since_point_present_multiple_times_uses_first_occurrence() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let mut specs = vec![
            (SINCE_LINE_IDX, SINCE_POINT_ID),
            (HW_REF_A_LINE_IDX, HW_REF_A_POINT_ID),
        ];
        specs.extend(filler_specs(FILLER_COUNT - 1));
        specs.push((SINCE_LINE_IDX, SINCE_POINT_ID));
        specs.push((HW_REF_CLOSE_LINE_IDX, HW_REF_CLOSE_POINT_ID));
        let route = route_from_specs(&ctx, &specs);

        assert!(route.has_looped(&ctx, Some(&point_ref(SINCE_POINT_ID))));
    }

    #[test]
    fn repeated_same_point_before_since_point_is_ignored() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let route = route_from_specs(
            &ctx,
            &[
                (EXACT_A_LINE_IDX, EXACT_A_POINT_ID),
                (EXACT_B_LINE_IDX, EXACT_B_POINT_ID),
                (EXACT_A_LINE_IDX, EXACT_A_POINT_ID),
            ],
        );

        assert!(!route.has_looped(&ctx, Some(&point_ref(EXACT_B_POINT_ID))));
    }

    #[test]
    fn close_points_outside_threshold_return_false() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let mut specs = vec![(HW_REF_A_LINE_IDX, HW_REF_A_POINT_ID)];
        specs.extend(filler_specs(FILLER_COUNT));
        specs.push((HW_REF_OUTSIDE_LINE_IDX, HW_REF_OUTSIDE_POINT_ID));
        let route = route_from_specs(&ctx, &specs);

        assert!(!route.has_looped(&ctx, None));
    }

    #[test]
    fn close_points_inside_threshold_return_true() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let distance_m = ctx
            .point(&point_ref(HW_REF_A_POINT_ID))
            .distance_between(&ctx.point(&point_ref(HW_REF_CLOSE_POINT_ID)));
        let mut specs = vec![(HW_REF_A_LINE_IDX, HW_REF_A_POINT_ID)];
        specs.extend(filler_specs(FILLER_COUNT));
        specs.push((HW_REF_CLOSE_LINE_IDX, HW_REF_CLOSE_POINT_ID));
        let route = route_from_specs(&ctx, &specs);

        assert!(distance_m < LOOP_DISTANCE_THRESHOLD);
        assert!(route.has_looped(&ctx, None));
    }

    #[test]
    fn adding_segments_updates_later_loop_outcomes() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let mut route = Route::new();
        route.add_segment(&ctx, segment(HW_REF_A_LINE_IDX, HW_REF_A_POINT_ID));
        for (line_idx, point_id) in filler_specs(FILLER_COUNT) {
            route.add_segment(&ctx, segment(line_idx, point_id));
        }

        assert!(!route.has_looped(&ctx, None));

        route.add_segment(&ctx, segment(HW_REF_CLOSE_LINE_IDX, HW_REF_CLOSE_POINT_ID));

        assert!(route.has_looped(&ctx, None));
    }

    #[test]
    fn removing_last_segment_restores_prior_outcome() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let mut route = Route::new();
        route.add_segment(&ctx, segment(HW_REF_A_LINE_IDX, HW_REF_A_POINT_ID));
        for (line_idx, point_id) in filler_specs(FILLER_COUNT) {
            route.add_segment(&ctx, segment(line_idx, point_id));
        }
        route.add_segment(&ctx, segment(HW_REF_CLOSE_LINE_IDX, HW_REF_CLOSE_POINT_ID));

        assert!(route.has_looped(&ctx, None));

        route.remove_last_segment();

        assert!(!route.has_looped(&ctx, None));
    }

    #[test]
    fn add_remove_add_again_keeps_behavior_stable() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let mut route = Route::new();
        route.add_segment(&ctx, segment(HW_REF_A_LINE_IDX, HW_REF_A_POINT_ID));
        for (line_idx, point_id) in filler_specs(FILLER_COUNT) {
            route.add_segment(&ctx, segment(line_idx, point_id));
        }
        route.add_segment(&ctx, segment(HW_REF_CLOSE_LINE_IDX, HW_REF_CLOSE_POINT_ID));
        assert!(route.has_looped(&ctx, None));

        route.remove_last_segment();
        assert!(!route.has_looped(&ctx, None));

        route.add_segment(&ctx, segment(HW_REF_CLOSE_LINE_IDX, HW_REF_CLOSE_POINT_ID));
        assert!(route.has_looped(&ctx, None));
    }

    #[test]
    fn detector_state_tracks_route_mutation() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let mut route = Route::new();
        let first = segment(EXACT_A_LINE_IDX, EXACT_A_POINT_ID);
        let second = segment(EXACT_B_LINE_IDX, EXACT_B_POINT_ID);

        route.add_segment(&ctx, first.clone());
        route.add_segment(&ctx, second.clone());

        assert_eq!(route.route_segments.len(), 2);
        assert_eq!(route.loop_detector.metas.len(), 2);
        assert_eq!(
            route.loop_detector.point_hits.get(first.get_end_point()),
            Some(&vec![0])
        );
        assert_eq!(
            route.loop_detector.point_hits.get(second.get_end_point()),
            Some(&vec![1])
        );

        let removed = route.remove_last_segment();
        assert_eq!(removed, Some(second));
        assert_eq!(route.route_segments.len(), 1);
        assert_eq!(route.loop_detector.metas.len(), 1);
        assert!(!route
            .loop_detector
            .point_hits
            .contains_key(&point_ref(EXACT_B_POINT_ID)));
        assert_eq!(
            route
                .loop_detector
                .point_hits
                .get(&point_ref(EXACT_A_POINT_ID)),
            Some(&vec![0])
        );
    }

    #[test]
    fn cloned_route_keeps_detector_state_independent() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let mut route = Route::new();
        route.add_segment(&ctx, segment(HW_REF_A_LINE_IDX, HW_REF_A_POINT_ID));
        for (line_idx, point_id) in filler_specs(FILLER_COUNT) {
            route.add_segment(&ctx, segment(line_idx, point_id));
        }
        route.add_segment(&ctx, segment(HW_REF_CLOSE_LINE_IDX, HW_REF_CLOSE_POINT_ID));

        let mut cloned_route = route.clone();

        assert!(route.has_looped(&ctx, None));
        assert!(cloned_route.has_looped(&ctx, None));

        route.remove_last_segment();
        assert!(!route.has_looped(&ctx, None));
        assert!(cloned_route.has_looped(&ctx, None));

        cloned_route.remove_last_segment();
        assert!(!cloned_route.has_looped(&ctx, None));
    }

    #[test]
    fn detector_hydrates_loop_meta_on_push() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let mut route = Route::new();

        route.add_segment(&ctx, segment(HW_REF_A_LINE_IDX, HW_REF_A_POINT_ID));

        let meta = &route.loop_detector.metas[0];
        assert_eq!(meta.end_point, point_ref(HW_REF_A_POINT_ID));
        assert_eq!(meta.point_id, HW_REF_A_POINT_ID);
        assert_eq!(meta.lat, ctx.point(&point_ref(HW_REF_A_POINT_ID)).lat);
        assert_eq!(meta.lon, ctx.point(&point_ref(HW_REF_A_POINT_ID)).lon);
        assert_eq!(meta.hw_ref, Some(RoadKey("R1".into())));
        assert_eq!(meta.name, None);
        assert_eq!(meta.cell_id, CellId::from_lat_lon(meta.lat, meta.lon),);
    }

    #[test]
    fn repeated_point_insertions_are_removed_in_stack_order() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let mut route = Route::new();

        route.add_segment(&ctx, segment(EXACT_A_LINE_IDX, EXACT_A_POINT_ID));
        route.add_segment(&ctx, segment(EXACT_A_LINE_IDX, EXACT_A_POINT_ID));

        assert_eq!(
            route
                .loop_detector
                .point_hits
                .get(&point_ref(EXACT_A_POINT_ID)),
            Some(&vec![0, 1])
        );

        route.remove_last_segment();
        assert_eq!(
            route
                .loop_detector
                .point_hits
                .get(&point_ref(EXACT_A_POINT_ID)),
            Some(&vec![0])
        );

        route.remove_last_segment();
        assert!(!route
            .loop_detector
            .point_hits
            .contains_key(&point_ref(EXACT_A_POINT_ID)));
    }

    #[test]
    fn repeated_road_key_insertions_across_multiple_cells_are_reversible() {
        let graph = loop_test_graph();
        let ctx = RoutingContext::new(&graph);
        let mut route = Route::new();

        route.add_segment(&ctx, segment(HW_REF_A_LINE_IDX, HW_REF_A_POINT_ID));
        route.add_segment(&ctx, segment(HW_REF_CLOSE_LINE_IDX, HW_REF_CLOSE_POINT_ID));

        let road_key = RoadKey("R1".into());
        let first_cell = CellId::from_lat_lon(
            ctx.point(&point_ref(HW_REF_A_POINT_ID)).lat,
            ctx.point(&point_ref(HW_REF_A_POINT_ID)).lon,
        );
        let second_cell = CellId::from_lat_lon(
            ctx.point(&point_ref(HW_REF_CLOSE_POINT_ID)).lat,
            ctx.point(&point_ref(HW_REF_CLOSE_POINT_ID)).lon,
        );

        let cells = route.loop_detector.hw_ref_cells.get(&road_key).unwrap();
        assert_eq!(cells.get(&first_cell), Some(&vec![0]));
        assert_eq!(cells.get(&second_cell), Some(&vec![1]));

        route.remove_last_segment();
        let cells = route.loop_detector.hw_ref_cells.get(&road_key).unwrap();
        assert_eq!(cells.get(&first_cell), Some(&vec![0]));
        assert!(!cells.contains_key(&second_cell));

        route.remove_last_segment();
        assert!(!route.loop_detector.hw_ref_cells.contains_key(&road_key));
    }
}
