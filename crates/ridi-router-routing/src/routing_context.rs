use std::{cell::RefCell, collections::HashMap};

use crate::{
    map_data::{
        graph::{
            ElementTagSet, ElementTagSetRef, ElementTagValueRef, MapDataGraph, MapDataLineRef,
            MapDataPointRef,
        },
        line::MapDataLine,
        point::MapDataPoint,
    },
    rmdf::format::PointRecord,
    router::rules::RouterRules,
};

#[derive(Default)]
struct RoutingCaches {
    point_records: HashMap<MapDataPointRef, PointRecord>,
    points: HashMap<MapDataPointRef, MapDataPoint>,
    lines: HashMap<MapDataLineRef, MapDataLine>,
    tag_sets: HashMap<ElementTagSetRef, ElementTagSet>,
    adjacent: HashMap<MapDataPointRef, Vec<(MapDataLineRef, MapDataPointRef)>>,
}

pub(crate) struct RoutingContext<'a> {
    graph: &'a MapDataGraph,
    caches: RefCell<RoutingCaches>,
}

impl<'a> RoutingContext<'a> {
    pub(crate) fn new(graph: &'a MapDataGraph) -> Self {
        Self {
            graph,
            caches: RefCell::new(RoutingCaches::default()),
        }
    }

    pub(crate) fn graph(&self) -> &'a MapDataGraph {
        self.graph
    }

    #[cfg(test)]
    pub(crate) fn for_route_generation_task(&self) -> Self {
        Self::new(self.graph)
    }

    fn point_record(&self, point_ref: &MapDataPointRef) -> PointRecord {
        if let Some(point_record) = self.caches.borrow().point_records.get(point_ref).copied() {
            return point_record;
        }

        let point_record = self
            .graph
            .get_point_record_from_tiles(point_ref.get_tile_id(), point_ref.get_element_id())
            .expect("point record should exist for point ref");
        self.caches
            .borrow_mut()
            .point_records
            .insert(point_ref.clone(), point_record);
        point_record
    }

    pub(crate) fn point(&self, point_ref: &MapDataPointRef) -> MapDataPoint {
        self.with_point(point_ref, Clone::clone)
    }

    // Borrow a cached point for hot-path read access without cloning the owned point.
    pub(crate) fn with_point<R>(
        &self,
        point_ref: &MapDataPointRef,
        f: impl FnOnce(&MapDataPoint) -> R,
    ) -> R {
        self.ensure_point_cached(point_ref);
        let caches = self.caches.borrow();
        let point = caches
            .points
            .get(point_ref)
            .expect("point should exist in cache after hydration");
        f(point)
    }

    fn ensure_point_cached(&self, point_ref: &MapDataPointRef) {
        if self.caches.borrow().points.contains_key(point_ref) {
            return;
        }

        let point = self
            .graph
            .get_point_from_tiles(point_ref.get_tile_id(), point_ref.get_element_id());
        self.caches
            .borrow_mut()
            .points
            .insert(point_ref.clone(), point);
    }

    pub(crate) fn point_id(&self, point_ref: &MapDataPointRef) -> u64 {
        point_ref.get_element_id()
    }

    pub(crate) fn point_coords(&self, point_ref: &MapDataPointRef) -> (f32, f32) {
        let point = self.point_record(point_ref);
        (point.lat, point.lon)
    }

    pub(crate) fn point_degree(&self, point_ref: &MapDataPointRef) -> usize {
        self.point_record(point_ref).lines_count as usize
    }

    pub(crate) fn point_is_junction(&self, point_ref: &MapDataPointRef) -> bool {
        self.point_degree(point_ref) > 2
    }

    pub(crate) fn point_near_residential(&self, point_ref: &MapDataPointRef) -> bool {
        self.point_record(point_ref).residential_in_proximity()
    }

    pub(crate) fn point_is_nogo(&self, point_ref: &MapDataPointRef) -> bool {
        self.point_record(point_ref).nogo_area()
    }

    pub(crate) fn line(&self, line_ref: &MapDataLineRef) -> MapDataLine {
        self.with_line(line_ref, Clone::clone)
    }

    // Borrow a cached line for hot-path read access without cloning the owned line.
    pub(crate) fn with_line<R>(
        &self,
        line_ref: &MapDataLineRef,
        f: impl FnOnce(&MapDataLine) -> R,
    ) -> R {
        self.ensure_line_cached(line_ref);
        let caches = self.caches.borrow();
        let line = caches
            .lines
            .get(line_ref)
            .expect("line should exist in cache after hydration");
        f(line)
    }

    fn ensure_line_cached(&self, line_ref: &MapDataLineRef) {
        if self.caches.borrow().lines.contains_key(line_ref) {
            return;
        }

        let line = self
            .graph
            .get_line_from_tiles(line_ref.get_tile_id(), line_ref.get_element_id() as usize);
        self.caches
            .borrow_mut()
            .lines
            .insert(line_ref.clone(), line);
    }

    pub(crate) fn tag_set(&self, tag_set_ref: &ElementTagSetRef) -> ElementTagSet {
        if let Some(tag_set) = self.caches.borrow().tag_sets.get(tag_set_ref).cloned() {
            return tag_set;
        }

        let tag_set = self.graph.get_tag_set(tag_set_ref);
        self.caches
            .borrow_mut()
            .tag_sets
            .insert(tag_set_ref.clone(), tag_set.clone());
        tag_set
    }

    pub(crate) fn tag_value(&self, tag_value_ref: &ElementTagValueRef) -> Option<String> {
        self.graph.get_tag_value(tag_value_ref)
    }

    pub(crate) fn adjacent(
        &self,
        point_ref: &MapDataPointRef,
    ) -> Vec<(MapDataLineRef, MapDataPointRef)> {
        self.with_adjacent(point_ref, |adjacent| adjacent.to_vec())
    }

    // Borrow cached adjacency for hot-path iteration without cloning the backing vector.
    pub(crate) fn with_adjacent<R>(
        &self,
        point_ref: &MapDataPointRef,
        f: impl FnOnce(&[(MapDataLineRef, MapDataPointRef)]) -> R,
    ) -> R {
        self.ensure_adjacent_cached(point_ref);
        let caches = self.caches.borrow();
        let adjacent = caches
            .adjacent
            .get(point_ref)
            .expect("adjacent should exist in cache after hydration");
        f(adjacent.as_slice())
    }

    fn ensure_adjacent_cached(&self, point_ref: &MapDataPointRef) {
        if self.caches.borrow().adjacent.contains_key(point_ref) {
            return;
        }

        let adjacent = self.graph.get_adjacent(point_ref.clone());
        self.caches
            .borrow_mut()
            .adjacent
            .insert(point_ref.clone(), adjacent);
    }

    pub(crate) fn closest_to_coords(
        &self,
        lat: f32,
        lon: f32,
        rules: &RouterRules,
        avoid_proximity_to_residential: bool,
        limit_to_hw_tags: Option<&[&'static str]>,
    ) -> Option<MapDataPointRef> {
        self.graph.get_closest_to_coords(
            lat,
            lon,
            rules,
            avoid_proximity_to_residential,
            limit_to_hw_tags,
        )
    }

    #[cfg(test)]
    fn cache_sizes(&self) -> (usize, usize, usize, usize, usize) {
        let caches = self.caches.borrow();
        (
            caches.point_records.len(),
            caches.points.len(),
            caches.lines.len(),
            caches.tag_sets.len(),
            caches.adjacent.len(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, hint::black_box, time::Instant};

    use crate::{
        map_data::{
            graph::{MapDataGraph, MapDataLineRef, MapDataPointRef},
            line::{LineDirection, MapDataLine},
            point::MapDataPoint,
        },
        rmdf::TileManager,
        RoutingContext,
    };
    use ridi_router_test_support::rmdf::{
        create_linear_single_tile_fixture, create_missing_neighbor_fixture, SYNTHETIC_TILE_ID,
    };

    #[test]
    fn caches_point_and_line_lookups_within_one_context() {
        let graph = MapDataGraph::new_test();
        let point_ref = MapDataPointRef::new(SYNTHETIC_TILE_ID, 1);
        let line_ref = MapDataLineRef::new(SYNTHETIC_TILE_ID, 7);

        graph.test_insert_point(MapDataPoint {
            id: 1,
            lat: 1.0,
            lon: 2.0,
            lines: vec![line_ref.clone()],
            rules: Vec::new(),
            residential_in_proximity: false,
            nogo_area: false,
        });
        graph.test_insert_line(
            MapDataLine {
                points: (
                    point_ref.clone(),
                    MapDataPointRef::new(SYNTHETIC_TILE_ID, 2),
                ),
                direction: LineDirection::BothWays,
                tags: crate::map_data::graph::ElementTagSetRef::new(SYNTHETIC_TILE_ID, 0),
            },
            7,
        );

        let ctx = RoutingContext::new(&graph);
        assert_eq!(ctx.cache_sizes(), (0, 0, 0, 0, 0));

        assert_eq!(ctx.point(&point_ref).id, 1);
        assert_eq!(ctx.point(&point_ref).id, 1);
        assert_eq!(ctx.line(&line_ref).points.0, point_ref);
        assert_eq!(ctx.line(&line_ref).points.0.get_element_id(), 1);
        assert_eq!(ctx.cache_sizes(), (0, 1, 1, 0, 0));
    }

    #[test]
    fn route_generation_task_context_starts_with_fresh_local_caches() {
        let graph = MapDataGraph::new_test();
        let point_ref = MapDataPointRef::new(SYNTHETIC_TILE_ID, 1);
        graph.test_insert_point(MapDataPoint {
            id: 1,
            lat: 1.0,
            lon: 2.0,
            lines: Vec::new(),
            rules: Vec::new(),
            residential_in_proximity: false,
            nogo_area: false,
        });

        let parent = RoutingContext::new(&graph);
        parent.point(&point_ref);
        assert_eq!(parent.cache_sizes(), (0, 1, 0, 0, 0));

        let task_ctx = parent.for_route_generation_task();
        assert_eq!(task_ctx.cache_sizes(), (0, 0, 0, 0, 0));

        task_ctx.point(&point_ref);
        assert_eq!(task_ctx.cache_sizes(), (0, 1, 0, 0, 0));
        assert_eq!(parent.cache_sizes(), (0, 1, 0, 0, 0));
    }

    #[test]
    fn caches_point_records_for_field_level_access() {
        let fixture = create_linear_single_tile_fixture("routing-context-point-record-cache");
        let graph = MapDataGraph::new(TileManager::new(fixture.dir.clone()).unwrap());
        let ctx = RoutingContext::new(&graph);
        let point_ref = MapDataPointRef::new(fixture.tile_id, fixture.start_osm_id);

        assert_eq!(ctx.cache_sizes(), (0, 0, 0, 0, 0));
        assert_eq!(ctx.point_id(&point_ref), fixture.start_osm_id);
        assert_eq!(ctx.point_degree(&point_ref), 1);
        assert!(!ctx.point_is_junction(&point_ref));
        assert_eq!(
            ctx.point_coords(&point_ref),
            (fixture.start_lat, fixture.start_lon)
        );
        assert_eq!(ctx.cache_sizes(), (1, 0, 0, 0, 0));

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn caches_tag_sets_within_one_context() {
        let fixture = create_linear_single_tile_fixture("routing-context-tag-set-cache");
        let graph = MapDataGraph::new(TileManager::new(fixture.dir.clone()).unwrap());
        let ctx = RoutingContext::new(&graph);
        let line_ref = MapDataLineRef::new(fixture.tile_id, 0);
        let line = ctx.line(&line_ref);

        assert_eq!(ctx.cache_sizes(), (0, 0, 1, 0, 0));

        let first = ctx.tag_set(&line.tags);
        let second = ctx.tag_set(&line.tags);

        assert_eq!(first, second);
        assert_eq!(ctx.cache_sizes(), (0, 0, 1, 1, 0));

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn caches_adjacency_within_one_context() {
        let fixture = create_missing_neighbor_fixture("routing-context-adjacent-cache");
        let graph = MapDataGraph::new(TileManager::new(fixture.dir.clone()).unwrap());
        let ctx = RoutingContext::new(&graph);
        let point_ref = MapDataPointRef::new(fixture.tile_a, fixture.center_osm_id);

        assert_eq!(ctx.cache_sizes(), (0, 0, 0, 0, 0));

        let first = ctx.adjacent(&point_ref);
        assert_eq!(first.len(), 1);
        assert!(first.contains(&(
            MapDataLineRef::new(fixture.tile_a, 0),
            MapDataPointRef::new(fixture.tile_a, fixture.in_tile_neighbor_osm_id),
        )));
        assert_eq!(ctx.cache_sizes(), (0, 0, 0, 0, 1));

        let second = ctx.adjacent(&point_ref);
        assert_eq!(second, first);
        assert_eq!(ctx.cache_sizes(), (0, 0, 0, 0, 1));

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn with_point_exposes_same_visible_data_as_point() {
        let graph = MapDataGraph::new_test();
        let point_ref = MapDataPointRef::new(SYNTHETIC_TILE_ID, 1);
        let line_ref = MapDataLineRef::new(SYNTHETIC_TILE_ID, 7);

        graph.test_insert_point(MapDataPoint {
            id: 1,
            lat: 1.0,
            lon: 2.0,
            lines: vec![line_ref],
            rules: Vec::new(),
            residential_in_proximity: true,
            nogo_area: false,
        });

        let ctx = RoutingContext::new(&graph);
        let owned = ctx.point(&point_ref);
        let borrowed = ctx.with_point(&point_ref, |point| point.clone());

        assert_eq!(borrowed, owned);
        assert_eq!(ctx.cache_sizes(), (0, 1, 0, 0, 0));
    }

    #[test]
    fn with_line_exposes_same_visible_data_as_line() {
        let graph = MapDataGraph::new_test();
        let point_ref = MapDataPointRef::new(SYNTHETIC_TILE_ID, 1);
        let line_ref = MapDataLineRef::new(SYNTHETIC_TILE_ID, 7);

        graph.test_insert_line(
            MapDataLine {
                points: (point_ref, MapDataPointRef::new(SYNTHETIC_TILE_ID, 2)),
                direction: LineDirection::BothWays,
                tags: crate::map_data::graph::ElementTagSetRef::new(SYNTHETIC_TILE_ID, 0),
            },
            7,
        );

        let ctx = RoutingContext::new(&graph);
        let owned = ctx.line(&line_ref);
        let borrowed = ctx.with_line(&line_ref, |line| line.clone());

        assert_eq!(borrowed, owned);
        assert_eq!(ctx.cache_sizes(), (0, 0, 1, 0, 0));
    }

    #[test]
    fn with_adjacent_exposes_same_visible_data_as_adjacent() {
        let fixture = create_missing_neighbor_fixture("routing-context-with-adjacent-equivalence");
        let graph = MapDataGraph::new(TileManager::new(fixture.dir.clone()).unwrap());
        let ctx = RoutingContext::new(&graph);
        let point_ref = MapDataPointRef::new(fixture.tile_a, fixture.center_osm_id);

        let owned = ctx.adjacent(&point_ref);
        let borrowed = ctx.with_adjacent(&point_ref, |adjacent| adjacent.to_vec());

        assert_eq!(borrowed, owned);
        assert_eq!(ctx.cache_sizes(), (0, 0, 0, 0, 1));

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn with_point_line_and_with_adjacent_work_with_cached_data() {
        let fixture = create_missing_neighbor_fixture("routing-context-borrowed-cache");
        let graph = MapDataGraph::new(TileManager::new(fixture.dir.clone()).unwrap());
        let ctx = RoutingContext::new(&graph);
        let point_ref = MapDataPointRef::new(fixture.tile_a, fixture.center_osm_id);

        let first_point = ctx.point(&point_ref);
        let first_adjacent = ctx.adjacent(&point_ref);
        let first_line = ctx.line(&first_adjacent[0].0);
        assert_eq!(ctx.cache_sizes(), (0, 1, 1, 0, 1));

        let cached_point_id = ctx.with_point(&point_ref, |point| point.id);
        let cached_adjacent = ctx.with_adjacent(&point_ref, |adjacent| adjacent.to_vec());
        let cached_line = ctx.with_line(&first_adjacent[0].0, |line| line.clone());

        assert_eq!(cached_point_id, first_point.id);
        assert_eq!(cached_adjacent, first_adjacent);
        assert_eq!(cached_line, first_line);
        assert_eq!(ctx.cache_sizes(), (0, 1, 1, 0, 1));

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn route_generation_task_context_does_not_reuse_parent_adjacency_cache() {
        let fixture =
            create_missing_neighbor_fixture("routing-context-adjacent-cache-task-isolation");
        let graph = MapDataGraph::new(TileManager::new(fixture.dir.clone()).unwrap());
        let parent = RoutingContext::new(&graph);
        let point_ref = MapDataPointRef::new(fixture.tile_a, fixture.center_osm_id);

        let expected = parent.adjacent(&point_ref);
        assert_eq!(parent.cache_sizes(), (0, 0, 0, 0, 1));

        let task_ctx = parent.for_route_generation_task();
        assert_eq!(task_ctx.cache_sizes(), (0, 0, 0, 0, 0));

        let actual = task_ctx.adjacent(&point_ref);
        assert_eq!(actual, expected);
        assert_eq!(task_ctx.cache_sizes(), (0, 0, 0, 0, 1));
        assert_eq!(parent.cache_sizes(), (0, 0, 0, 0, 1));

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    #[ignore = "micro-benchmark"]
    fn microbench_repeated_same_point_hydration() {
        let fixture = create_linear_single_tile_fixture("routing-context-same-point-bench");
        let graph = MapDataGraph::new(TileManager::new(fixture.dir.clone()).unwrap());
        let ctx = RoutingContext::new(&graph);
        let point_ref = MapDataPointRef::new(fixture.tile_id, fixture.start_osm_id);

        let started = Instant::now();
        for _ in 0..50_000 {
            black_box(ctx.point(&point_ref));
        }
        eprintln!("same-point hydration bench: {:?}", started.elapsed());

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    #[ignore = "micro-benchmark"]
    fn microbench_many_points_hydration_within_one_tile() {
        let fixture = create_linear_single_tile_fixture("routing-context-many-points-bench");
        let graph = MapDataGraph::new(TileManager::new(fixture.dir.clone()).unwrap());
        let ctx = RoutingContext::new(&graph);
        let point_refs = (fixture.start_osm_id..=fixture.finish_osm_id)
            .map(|osm_id| MapDataPointRef::new(fixture.tile_id, osm_id))
            .collect::<Vec<_>>();

        let started = Instant::now();
        for _ in 0..10_000 {
            for point_ref in &point_refs {
                black_box(ctx.point(point_ref));
            }
        }
        eprintln!("many-points hydration bench: {:?}", started.elapsed());

        fs::remove_dir_all(fixture.dir).unwrap();
    }
}
