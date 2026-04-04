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

pub(crate) struct RoutingContext<'a> {
    graph: &'a MapDataGraph,
}

impl<'a> RoutingContext<'a> {
    pub(crate) fn new(graph: &'a MapDataGraph) -> Self {
        Self { graph }
    }

    pub(crate) fn point(&self, point_ref: &MapDataPointRef) -> MapDataPoint {
        self.graph
            .get_point_from_tiles(point_ref.get_tile_id(), point_ref.get_element_id())
    }

    pub(crate) fn line(&self, line_ref: &MapDataLineRef) -> MapDataLine {
        self.graph
            .get_line_from_tiles(line_ref.get_tile_id(), line_ref.get_element_id() as usize)
    }

    pub(crate) fn tag_set(&self, tag_set_ref: &ElementTagSetRef) -> ElementTagSet {
        self.graph.get_tag_set(tag_set_ref)
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
}
