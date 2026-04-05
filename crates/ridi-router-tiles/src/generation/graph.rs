use std::collections::HashMap;

use super::{GenerationLine, GenerationPoint, GenerationTags, LineDirection};
use crate::osm_data::{OsmNode, OsmRelation, OsmWay};

pub struct GenerationGraph {
    pub(crate) points: Vec<GenerationPoint>,
    pub(crate) points_map: HashMap<u64, usize>,
    pub(crate) lines: Vec<GenerationLine>,
    pub(crate) tags: GenerationTags,
}

impl GenerationGraph {
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            points_map: HashMap::new(),
            lines: Vec::new(),
            tags: GenerationTags::new(),
        }
    }

    pub fn get_points(&self) -> &[GenerationPoint] {
        &self.points
    }

    pub fn get_lines(&self) -> &[GenerationLine] {
        &self.lines
    }

    pub fn get_tags(&self) -> &GenerationTags {
        &self.tags
    }

    #[allow(dead_code)]
    pub fn get_tags_mut(&mut self) -> &mut GenerationTags {
        &mut self.tags
    }

    pub fn insert_node(&mut self, node: OsmNode) {
        let point = GenerationPoint {
            id: node.id,
            lat: node.lat as f32,
            lon: node.lon as f32,
            residential_in_proximity: node.residential_in_proximity,
            nogo_area: node.nogo_area,
        };

        let idx = self.points.len();
        self.points.push(point);
        self.points_map.insert(node.id, idx);
    }

    pub fn insert_way(&mut self, way: OsmWay) {
        if way.point_ids.len() < 2 {
            return;
        }

        let Some(tags_map) = way.tags.as_ref() else {
            return;
        };

        let tag_set_id = self.tags.get_or_create(
            tags_map.get("name"),
            tags_map.get("ref"),
            tags_map.get("highway"),
            tags_map.get("surface"),
            tags_map.get("smoothness"),
        );

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

        for i in 0..way.point_ids.len() - 1 {
            let from_id = way.point_ids[i];
            let to_id = way.point_ids[i + 1];

            if !self.points_map.contains_key(&from_id) || !self.points_map.contains_key(&to_id) {
                continue;
            }

            self.lines.push(GenerationLine {
                from_node_id: from_id,
                to_node_id: to_id,
                direction: direction.clone(),
                tags: tag_set_id.clone(),
            });
        }
    }

    pub fn insert_relation(&mut self, _relation: OsmRelation) {
        // TODO: restriction materialization stays in the later rules/restrictions todo.
    }

    pub fn generate_point_hashes(&mut self) {
        // TODO: Implement spatial hashing if needed.
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
        let node = OsmNode {
            id: 12345,
            lat: 56.95,
            lon: 24.1,
            residential_in_proximity: true,
            nogo_area: true,
        };

        graph.insert_node(node);

        assert_eq!(graph.points.len(), 1);
        let point = &graph.points[0];
        assert_eq!(point.id, 12345);
        assert!(point.residential_in_proximity);
        assert!(point.nogo_area);
    }

    #[test]
    fn test_insert_node_with_false_flags() {
        let mut graph = GenerationGraph::new();
        let node = OsmNode {
            id: 67890,
            lat: 56.95,
            lon: 24.1,
            residential_in_proximity: false,
            nogo_area: false,
        };

        graph.insert_node(node);

        let point = &graph.points[0];
        assert!(!point.residential_in_proximity);
        assert!(!point.nogo_area);
    }
}
