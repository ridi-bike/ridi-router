use std::collections::HashMap;
use anyhow::Result;

use super::{
    point::MapDataPoint,
    line::MapDataLine,
    graph::ElementTags,
    rule::MapDataRule,
    osm::{OsmNode, OsmWay, OsmRelation},
    MapDataError,
};

/// Graph for building map data during tile generation.
/// Separate from MapDataGraph which is for routing queries via TileManager.
pub struct GenerationGraph {
    pub(crate) points: Vec<MapDataPoint>,
    pub(crate) points_map: HashMap<u64, usize>,  // OSM ID -> index in points vec
    pub(crate) lines: Vec<MapDataLine>,
    pub(crate) tags: ElementTags,
}

impl GenerationGraph {
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            points_map: HashMap::new(),
            lines: Vec::new(),
            tags: ElementTags::new(),
        }
    }

    /// Get points slice for iteration (used by RMDF writer)
    pub fn get_points(&self) -> &[MapDataPoint] {
        &self.points
    }

    /// Get lines slice for iteration (used by RMDF writer)
    pub fn get_lines(&self) -> &[MapDataLine] {
        &self.lines
    }

    /// Get tags reference (used by RMDF writer)
    pub fn get_tags(&self) -> &ElementTags {
        &self.tags
    }

    /// Get mutable tags reference (used during graph building)
    pub fn get_tags_mut(&mut self) -> &mut ElementTags {
        &mut self.tags
    }

    /// Insert a node from OSM data
    pub fn insert_node(&mut self, node: OsmNode) {
        let point = MapDataPoint {
            id: node.id,
            lat: node.lat as f32,  // OsmNode has f64, MapDataPoint uses f32
            lon: node.lon as f32,
            lines: Vec::new(),
            rules: Vec::new(),
            residential_in_proximity: false,
            nogo_area: false,
        };

        let idx = self.points.len();
        self.points.push(point);
        self.points_map.insert(node.id, idx);
    }

    /// Insert a way from OSM data
    /// This method needs to be implemented based on the old MapDataGraph logic
    pub fn insert_way(&mut self, way: OsmWay) -> Result<(), MapDataError> {
        // TODO: Implement way insertion logic
        // This is a complex method that needs to:
        // 1. Check if way is valid for routing
        // 2. Create MapDataLine segments
        // 3. Link lines to points
        // 4. Store tag information

        // For now, return Ok to allow compilation
        // Will need to port logic from old MapDataGraph
        Ok(())
    }

    /// Insert a relation from OSM data (turn restrictions, etc.)
    pub fn insert_relation(&mut self, relation: OsmRelation) -> Result<(), MapDataError> {
        // TODO: Implement relation insertion logic
        // This handles turn restrictions and other routing rules

        // For now, return Ok to allow compilation
        Ok(())
    }

    /// Generate point hashes for spatial indexing
    pub fn generate_point_hashes(&mut self) {
        // TODO: Implement spatial hashing if needed
        // This may not be needed if we're relying on the grid cell spatial index
        // in build_spatial_index() instead
    }
}

impl Default for GenerationGraph {
    fn default() -> Self {
        Self::new()
    }
}
