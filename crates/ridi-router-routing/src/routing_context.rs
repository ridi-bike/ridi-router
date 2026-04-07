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
    router::rules::RouterRules,
};

#[derive(Default)]
struct RoutingCaches {
    points: HashMap<MapDataPointRef, MapDataPoint>,
    lines: HashMap<MapDataLineRef, MapDataLine>,
    tag_sets: HashMap<ElementTagSetRef, ElementTagSet>,
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

    pub(crate) fn point(&self, point_ref: &MapDataPointRef) -> MapDataPoint {
        if let Some(point) = self.caches.borrow().points.get(point_ref).cloned() {
            return point;
        }

        let point = self
            .graph
            .get_point_from_tiles(point_ref.get_tile_id(), point_ref.get_element_id());
        self.caches
            .borrow_mut()
            .points
            .insert(point_ref.clone(), point.clone());
        point
    }

    pub(crate) fn line(&self, line_ref: &MapDataLineRef) -> MapDataLine {
        if let Some(line) = self.caches.borrow().lines.get(line_ref).cloned() {
            return line;
        }

        let line = self
            .graph
            .get_line_from_tiles(line_ref.get_tile_id(), line_ref.get_element_id() as usize);
        self.caches
            .borrow_mut()
            .lines
            .insert(line_ref.clone(), line.clone());
        line
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
        self.graph.get_adjacent(point_ref.clone())
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
    fn cache_sizes(&self) -> (usize, usize, usize) {
        let caches = self.caches.borrow();
        (
            caches.points.len(),
            caches.lines.len(),
            caches.tag_sets.len(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::{
        map_data::{
            graph::{MapDataGraph, MapDataLineRef, MapDataPointRef},
            line::{LineDirection, MapDataLine},
            point::MapDataPoint,
        },
        rmdf::TileManager,
        RoutingContext,
    };
    use ridi_router_test_support::rmdf::{create_linear_single_tile_fixture, SYNTHETIC_TILE_ID};

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
        assert_eq!(ctx.cache_sizes(), (0, 0, 0));

        assert_eq!(ctx.point(&point_ref).id, 1);
        assert_eq!(ctx.point(&point_ref).id, 1);
        assert_eq!(ctx.line(&line_ref).points.0, point_ref);
        assert_eq!(ctx.line(&line_ref).points.0.get_element_id(), 1);
        assert_eq!(ctx.cache_sizes(), (1, 1, 0));
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
        assert_eq!(parent.cache_sizes(), (1, 0, 0));

        let task_ctx = parent.for_route_generation_task();
        assert_eq!(task_ctx.cache_sizes(), (0, 0, 0));

        task_ctx.point(&point_ref);
        assert_eq!(task_ctx.cache_sizes(), (1, 0, 0));
        assert_eq!(parent.cache_sizes(), (1, 0, 0));
    }

    #[test]
    fn caches_tag_sets_within_one_context() {
        let fixture = create_linear_single_tile_fixture("routing-context-tag-set-cache");
        let graph = MapDataGraph::new(TileManager::new(fixture.dir.clone()).unwrap());
        let ctx = RoutingContext::new(&graph);
        let line_ref = MapDataLineRef::new(fixture.tile_id, 0);
        let line = ctx.line(&line_ref);

        assert_eq!(ctx.cache_sizes(), (0, 1, 0));

        let first = ctx.tag_set(&line.tags);
        let second = ctx.tag_set(&line.tags);

        assert_eq!(first, second);
        assert_eq!(ctx.cache_sizes(), (0, 1, 1));

        fs::remove_dir_all(fixture.dir).unwrap();
    }
}
