use anyhow::Result;
use std::collections::HashMap;

use super::{
    graph::{ElementTagSetRef, ElementTags},
    line::LineDirection,
    osm::{OsmNode, OsmRelation, OsmWay},
    point::MapDataPoint,
    MapDataError,
};

/// Simple line structure for generation (without runtime Ref types)
#[derive(Clone)]
pub struct GenerationLine {
    pub from_node_id: u64, // OSM node ID
    pub to_node_id: u64,   // OSM node ID
    pub direction: LineDirection,
    pub tags: ElementTagSetRef,
}

/// Graph for building map data during tile generation.
/// Separate from MapDataGraph which is for routing queries via TileManager.
pub struct GenerationGraph {
    pub(crate) points: Vec<MapDataPoint>,
    pub(crate) points_map: HashMap<u64, usize>, // OSM ID -> index in points vec
    pub(crate) lines: Vec<GenerationLine>,
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
    pub fn get_lines(&self) -> &[GenerationLine] {
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
            lat: node.lat as f32, // OsmNode has f64, MapDataPoint uses f32
            lon: node.lon as f32,
            lines: Vec::new(),
            rules: Vec::new(),
            residential_in_proximity: node.residential_in_proximity, // FIX: Use actual value
            nogo_area: node.nogo_area,                               // FIX: Use actual value
        };

        let idx = self.points.len();
        self.points.push(point);
        self.points_map.insert(node.id, idx);
    }

    /// Insert a way from OSM data
    pub fn insert_way(&mut self, way: OsmWay) -> Result<(), MapDataError> {
        // Validate way has at least 2 nodes
        if way.point_ids.len() < 2 {
            return Ok(()); // Skip invalid ways
        }

        // Get tags for this way
        let tags = way.tags.as_ref();
        if tags.is_none() {
            return Ok(()); // Skip ways without tags
        }

        let tags_map = tags.unwrap();

        // Extract key tag values for ElementTagSet
        let name = tags_map.get("name");
        let hw_ref = tags_map.get("ref");
        let highway = tags_map.get("highway");
        let surface = tags_map.get("surface");
        let smoothness = tags_map.get("smoothness");

        // Get or create tag set
        let tag_set_ref = self
            .tags
            .get_or_create(name, hw_ref, highway, surface, smoothness);

        // Determine line direction
        let direction = if let Some(oneway) = tags_map.get("oneway") {
            if oneway == "yes" || oneway == "1" || oneway == "true" {
                LineDirection::OneWay
            } else {
                LineDirection::BothWays
            }
        } else if let Some(junction) = tags_map.get("junction") {
            if junction == "roundabout" {
                LineDirection::Roundabout
            } else {
                LineDirection::BothWays
            }
        } else {
            LineDirection::BothWays
        };

        // Create line segments between consecutive nodes
        for i in 0..way.point_ids.len() - 1 {
            let from_id = way.point_ids[i];
            let to_id = way.point_ids[i + 1];

            // Look up node indices in graph
            let from_idx = self.points_map.get(&from_id);
            let to_idx = self.points_map.get(&to_id);

            if from_idx.is_none() || to_idx.is_none() {
                // Nodes not in graph (outside tile bounds), skip segment
                continue;
            }

            let from_idx = *from_idx.unwrap();
            let to_idx = *to_idx.unwrap();

            // Create line using simple structure (no Refs)
            let line = GenerationLine {
                from_node_id: from_id,
                to_node_id: to_id,
                direction: direction.clone(),
                tags: tag_set_ref.clone(),
            };

            self.lines.push(line);

            // NOTE: We don't update point.lines here because MapDataPoint.lines expects MapDataLineRef
            // The writer will build line refs when serializing to RMDF format
        }

        Ok(())
    }

    /// Insert a relation from OSM data (turn restrictions, etc.)
    pub fn insert_relation(&mut self, _relation: OsmRelation) -> Result<(), MapDataError> {
        // Turn restrictions are complex and require additional data structures
        // For now, we'll skip relation processing as it's not critical for basic routing
        // TODO: Implement proper turn restriction storage and processing
        // This will require adding a relations field to GenerationGraph
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_node_preserves_flags() {
        let mut graph = GenerationGraph::new();

        // Create node with flags set
        let node = OsmNode {
            id: 12345,
            lat: 56.95,
            lon: 24.1,
            residential_in_proximity: true,
            nogo_area: true,
        };

        graph.insert_node(node);

        // Verify flags are preserved in MapDataPoint
        assert_eq!(graph.points.len(), 1);
        let point = &graph.points[0];

        assert_eq!(point.id, 12345);
        assert_eq!(point.residential_in_proximity, true);
        assert_eq!(point.nogo_area, true);
    }

    #[test]
    fn test_insert_node_with_false_flags() {
        let mut graph = GenerationGraph::new();

        // Create node with flags false
        let node = OsmNode {
            id: 67890,
            lat: 56.95,
            lon: 24.1,
            residential_in_proximity: false,
            nogo_area: false,
        };

        graph.insert_node(node);

        // Verify flags are preserved as false (not hardcoded)
        let point = &graph.points[0];
        assert_eq!(point.residential_in_proximity, false);
        assert_eq!(point.nogo_area, false);
    }
}
