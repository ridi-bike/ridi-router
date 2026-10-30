//! In-memory tile graph used as the restriction-materialization context in tests (the
//! production context is `local_stage::tile`).

use std::collections::{BTreeSet, HashMap};

use super::graph::{materialize_restriction, RestrictionContext};
use super::{
    GenerationLine, GenerationPoint, GenerationRestrictionRule, GenerationRestrictionRuleType,
    GenerationRestrictionSkipStats, GenerationTags, LineDirection,
};
use ridi_router_common::osm::{OsmNode, OsmRelation, OsmWay};

/// Borrowed tag values read by `GenerationGraph::insert_way_parts`.
pub struct WayTagValues<'a> {
    pub name: Option<&'a String>,
    pub hw_ref: Option<&'a String>,
    pub highway: Option<&'a String>,
    pub surface: Option<&'a String>,
    pub smoothness: Option<&'a String>,
    pub oneway: Option<&'a str>,
    pub junction: Option<&'a str>,
}

pub struct GenerationGraph {
    pub(crate) points: Vec<GenerationPoint>,
    pub(crate) points_map: HashMap<u64, usize>,
    pub(crate) lines: Vec<GenerationLine>,
    pub(crate) tags: GenerationTags,
    pub(crate) way_line_indices: HashMap<u64, Vec<usize>>,
    pub(crate) restrictions_by_via: HashMap<u64, Vec<GenerationRestrictionRule>>,
    pub(crate) restriction_skip_stats: GenerationRestrictionSkipStats,
}

impl GenerationGraph {
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            points_map: HashMap::new(),
            lines: Vec::new(),
            tags: GenerationTags::new(),
            way_line_indices: HashMap::new(),
            restrictions_by_via: HashMap::new(),
            restriction_skip_stats: GenerationRestrictionSkipStats::default(),
        }
    }

    #[allow(dead_code)]
    pub fn get_tags_mut(&mut self) -> &mut GenerationTags {
        &mut self.tags
    }

    #[allow(dead_code)]
    pub fn get_restrictions_by_via(&self) -> &HashMap<u64, Vec<GenerationRestrictionRule>> {
        &self.restrictions_by_via
    }

    #[allow(dead_code)]
    pub fn get_restriction_skip_stats(&self) -> &GenerationRestrictionSkipStats {
        &self.restriction_skip_stats
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

        self.insert_way_parts(
            way.id,
            &way.point_ids,
            &WayTagValues {
                name: tags_map.get("name"),
                hw_ref: tags_map.get("ref"),
                highway: tags_map.get("highway"),
                surface: tags_map.get("surface"),
                smoothness: tags_map.get("smoothness"),
                oneway: tags_map.get("oneway").map(String::as_str),
                junction: tags_map.get("junction").map(String::as_str),
            },
        );
    }

    /// Insert a way from borrowed parts (same semantics as `insert_way` for a way with tags).
    pub fn insert_way_parts(&mut self, way_id: u64, point_ids: &[u64], tags: &WayTagValues<'_>) {
        if point_ids.len() < 2 {
            return;
        }

        let tag_set_id = self.tags.get_or_create(
            tags.name,
            tags.hw_ref,
            tags.highway,
            tags.surface,
            tags.smoothness,
        );

        let direction = if let Some(oneway) = tags.oneway {
            if oneway == "yes" || oneway == "1" || oneway == "true" {
                LineDirection::OneWay
            } else {
                LineDirection::BothWays
            }
        } else if let Some(junction) = tags.junction {
            if junction == "roundabout" {
                LineDirection::Roundabout
            } else {
                LineDirection::BothWays
            }
        } else {
            LineDirection::BothWays
        };

        let mut emitted_line_indices = Vec::new();

        for i in 0..point_ids.len() - 1 {
            let from_id = point_ids[i];
            let to_id = point_ids[i + 1];

            if !self.points_map.contains_key(&from_id) || !self.points_map.contains_key(&to_id) {
                continue;
            }

            let line_idx = self.lines.len();
            self.lines.push(GenerationLine {
                from_node_id: from_id,
                to_node_id: to_id,
                direction,
                tags: tag_set_id.clone(),
            });
            emitted_line_indices.push(line_idx);
        }

        if !emitted_line_indices.is_empty() {
            self.way_line_indices
                .entry(way_id)
                .or_default()
                .extend(emitted_line_indices);
        }
    }

    pub fn insert_relation(&mut self, relation: OsmRelation) {
        let mut stats = std::mem::take(&mut self.restriction_skip_stats);
        let rule = materialize_restriction(&*self, &relation, &mut stats, true);
        self.restriction_skip_stats = stats;
        if let Some(rule) = rule {
            self.restrictions_by_via
                .entry(rule.via_node_id)
                .or_default()
                .push(rule);
        }
    }

    fn resolve_via_adjacent_line_indices_for_way(&self, way_id: u64, via_node_id: u64) -> Vec<u32> {
        let mut line_indices = BTreeSet::new();

        let Some(way_line_indices) = self.way_line_indices.get(&way_id) else {
            return Vec::new();
        };

        for &line_idx in way_line_indices {
            let Some(line) = self.lines.get(line_idx) else {
                continue;
            };

            if line.from_node_id == via_node_id || line.to_node_id == via_node_id {
                line_indices.insert(
                    u32::try_from(line_idx)
                        .expect("tile-local generated line index must fit in u32"),
                );
            }
        }

        line_indices.into_iter().collect()
    }
}

impl Default for GenerationGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl RestrictionContext for GenerationGraph {
    fn has_point(&self, node_id: u64) -> bool {
        self.points_map.contains_key(&node_id)
    }

    fn via_adjacent_lines_for_way(&self, way_id: u64, via_node_id: u64) -> Vec<u32> {
        self.resolve_via_adjacent_line_indices_for_way(way_id, via_node_id)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        io,
        sync::{Arc, Mutex},
    };

    use tracing_subscriber::{
        fmt::{self, MakeWriter},
        prelude::*,
    };

    use super::*;
    use ridi_router_common::osm::{
        OsmRelationMember, OsmRelationMemberRole, OsmRelationMemberType,
    };

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

    #[test]
    fn test_insert_way_records_generated_line_indices() {
        let mut graph = seeded_graph();

        graph.insert_way(make_way(10, &[1, 2, 3]));

        assert_eq!(graph.way_line_indices.get(&10), Some(&vec![0, 1]));
        assert_eq!(graph.lines.len(), 2);
    }

    #[test]
    fn test_insert_relation_materializes_supported_rules() {
        let mut graph = seeded_graph();
        insert_base_ways(&mut graph);

        graph.insert_relation(make_relation(
            100,
            "no_left_turn",
            vec![
                relation_way_member(OsmRelationMemberRole::From, 10),
                relation_node_member(OsmRelationMemberRole::Via, 2),
                relation_way_member(OsmRelationMemberRole::To, 20),
            ],
            HashMap::new(),
        ));
        graph.insert_relation(make_relation(
            101,
            "only_right_turn",
            vec![
                relation_way_member(OsmRelationMemberRole::From, 10),
                relation_node_member(OsmRelationMemberRole::Via, 2),
                relation_way_member(OsmRelationMemberRole::To, 30),
            ],
            HashMap::new(),
        ));

        let rules = graph
            .restrictions_by_via
            .get(&2)
            .expect("via node 2 should have rules");
        assert_eq!(rules.len(), 2);

        assert_eq!(
            rules[0],
            GenerationRestrictionRule {
                relation_id: 100,
                via_node_id: 2,
                rule_type: GenerationRestrictionRuleType::NotAllowed,
                from_line_indices: vec![0, 1],
                to_line_indices: vec![2],
            }
        );

        assert_eq!(
            rules[1],
            GenerationRestrictionRule {
                relation_id: 101,
                via_node_id: 2,
                rule_type: GenerationRestrictionRuleType::OnlyAllowed,
                from_line_indices: vec![0, 1],
                to_line_indices: vec![4, 5],
            }
        );
    }

    #[test]
    fn test_insert_relation_keeps_only_via_adjacent_line_indices() {
        let mut graph = seeded_graph();
        insert_base_ways(&mut graph);

        graph.insert_relation(make_relation(
            200,
            "no_right_turn",
            vec![
                relation_way_member(OsmRelationMemberRole::From, 10),
                relation_node_member(OsmRelationMemberRole::Via, 2),
                relation_way_member(OsmRelationMemberRole::To, 20),
            ],
            HashMap::new(),
        ));

        let rule = &graph.restrictions_by_via.get(&2).unwrap()[0];
        assert_eq!(rule.from_line_indices, vec![0, 1]);
        assert_eq!(rule.to_line_indices, vec![2]);
        assert!(!rule.to_line_indices.contains(&3));
    }

    #[test]
    fn test_insert_relation_flattens_multi_to_relations() {
        let mut graph = seeded_graph();
        insert_base_ways(&mut graph);

        graph.insert_relation(make_relation(
            300,
            "no_straight_on",
            vec![
                relation_way_member(OsmRelationMemberRole::From, 10),
                relation_node_member(OsmRelationMemberRole::Via, 2),
                relation_way_member(OsmRelationMemberRole::To, 20),
                relation_way_member(OsmRelationMemberRole::To, 30),
            ],
            HashMap::new(),
        ));

        let rule = &graph.restrictions_by_via.get(&2).unwrap()[0];
        assert_eq!(rule.to_line_indices, vec![2, 4, 5]);
    }

    #[test]
    fn test_insert_relation_skips_when_any_to_member_is_missing_locally() {
        let mut graph = seeded_graph();
        insert_base_ways(&mut graph);

        graph.insert_relation(make_relation(
            301,
            "no_straight_on",
            vec![
                relation_way_member(OsmRelationMemberRole::From, 10),
                relation_node_member(OsmRelationMemberRole::Via, 2),
                relation_way_member(OsmRelationMemberRole::To, 20),
                relation_way_member(OsmRelationMemberRole::To, 9999),
            ],
            HashMap::new(),
        ));

        assert!(graph.restrictions_by_via.is_empty());
        assert_eq!(
            graph
                .restriction_skip_stats
                .malformed_or_unresolved_relations,
            1
        );
    }

    #[test]
    fn test_insert_relation_skips_when_any_from_member_is_missing_locally() {
        let mut graph = seeded_graph();
        insert_base_ways(&mut graph);

        graph.insert_relation(make_relation(
            302,
            "no_straight_on",
            vec![
                relation_way_member(OsmRelationMemberRole::From, 10),
                relation_way_member(OsmRelationMemberRole::From, 9999),
                relation_node_member(OsmRelationMemberRole::Via, 2),
                relation_way_member(OsmRelationMemberRole::To, 20),
            ],
            HashMap::new(),
        ));

        assert!(graph.restrictions_by_via.is_empty());
        assert_eq!(
            graph
                .restriction_skip_stats
                .malformed_or_unresolved_relations,
            1
        );
    }

    #[test]
    fn test_insert_relation_materializes_motorcycle_specific_restrictions() {
        let mut graph = seeded_graph();
        insert_base_ways(&mut graph);

        let members = || {
            vec![
                relation_way_member(OsmRelationMemberRole::From, 10),
                relation_node_member(OsmRelationMemberRole::Via, 2),
                relation_way_member(OsmRelationMemberRole::To, 30),
            ]
        };

        graph.insert_relation(make_relation(
            400,
            "no_right_turn",
            members(),
            HashMap::from([(
                "restriction:motorcycle".to_string(),
                "only_right_turn".to_string(),
            )]),
        ));
        graph.insert_relation(make_relation(
            401,
            "no_right_turn",
            members(),
            HashMap::from([(
                "restriction:motorcycle:conditional".to_string(),
                "only_straight_on @ (Mo-Fr 08:00-18:00)".to_string(),
            )]),
        ));
        graph.insert_relation(make_relation(
            402,
            "no_right_turn",
            members(),
            HashMap::from([("except".to_string(), "motorcycle".to_string())]),
        ));
        graph.insert_relation(make_relation(
            403,
            "no_right_turn",
            members(),
            HashMap::from([("except".to_string(), "bicycle".to_string())]),
        ));
        graph.insert_relation(make_relation_with_tags(
            404,
            members(),
            HashMap::from([
                ("type".to_string(), "restriction".to_string()),
                (
                    "restriction:conditional".to_string(),
                    "only_u_turn @ (Mo-Fr 08:00-18:00)".to_string(),
                ),
            ]),
        ));
        graph.insert_relation(make_relation_with_tags(
            405,
            members(),
            HashMap::from([
                ("type".to_string(), "restriction".to_string()),
                (
                    "restriction:bicycle".to_string(),
                    "no_right_turn".to_string(),
                ),
            ]),
        ));

        let rules = graph.restrictions_by_via.get(&2).unwrap();
        assert_eq!(rules.len(), 4);
        assert_eq!(
            rules
                .iter()
                .map(|rule| rule.relation_id)
                .collect::<Vec<_>>(),
            vec![400, 401, 403, 404]
        );
        assert_eq!(
            rules[0].rule_type,
            GenerationRestrictionRuleType::OnlyAllowed
        );
        assert_eq!(
            rules[1].rule_type,
            GenerationRestrictionRuleType::OnlyAllowed
        );
        assert_eq!(
            rules[2].rule_type,
            GenerationRestrictionRuleType::NotAllowed
        );
        assert_eq!(
            rules[3].rule_type,
            GenerationRestrictionRuleType::OnlyAllowed
        );
        assert!(graph.restriction_skip_stats.is_empty());
    }

    #[test]
    fn test_insert_relation_skips_unsupported_and_malformed_relations() {
        let mut graph = seeded_graph();
        insert_base_ways(&mut graph);

        graph.insert_relation(make_relation(
            500,
            "no_right_turn",
            vec![
                relation_way_member(OsmRelationMemberRole::From, 10),
                relation_way_member(OsmRelationMemberRole::Via, 20),
                relation_way_member(OsmRelationMemberRole::To, 30),
            ],
            HashMap::new(),
        ));
        graph.insert_relation(make_relation(
            501,
            "no_right_turn",
            vec![
                relation_way_member(OsmRelationMemberRole::From, 9999),
                relation_node_member(OsmRelationMemberRole::Via, 2),
                relation_way_member(OsmRelationMemberRole::To, 30),
            ],
            HashMap::new(),
        ));

        assert!(graph.restrictions_by_via.is_empty());
        assert_eq!(graph.restriction_skip_stats.via_way_relations, 1);
        assert_eq!(
            graph
                .restriction_skip_stats
                .malformed_or_unresolved_relations,
            1
        );
    }

    #[test]
    fn test_insert_relation_warns_on_skipped_relations() {
        let mut graph = seeded_graph();
        insert_base_ways(&mut graph);

        let log_buffer = SharedLogBuffer::default();
        let subscriber = tracing_subscriber::registry().with(
            fmt::layer()
                .without_time()
                .with_ansi(false)
                .with_target(false)
                .with_writer(log_buffer.clone())
                .with_filter(tracing_subscriber::filter::LevelFilter::WARN),
        );

        tracing::subscriber::with_default(subscriber, || {
            graph.insert_relation(make_relation(
                500,
                "no_right_turn",
                vec![
                    relation_way_member(OsmRelationMemberRole::From, 10),
                    relation_way_member(OsmRelationMemberRole::Via, 20),
                    relation_way_member(OsmRelationMemberRole::To, 30),
                ],
                HashMap::new(),
            ));
        });

        let output = log_buffer.as_string();
        assert!(output.contains("Skipping restriction relation 500 [via_way]"));
        assert!(output.contains("via-way restrictions are not supported"));
    }

    fn seeded_graph() -> GenerationGraph {
        let mut graph = GenerationGraph::new();
        for id in 1..=7 {
            graph.insert_node(OsmNode {
                id,
                lat: id as f64,
                lon: id as f64,
                residential_in_proximity: false,
                nogo_area: false,
            });
        }
        graph
    }

    fn insert_base_ways(graph: &mut GenerationGraph) {
        graph.insert_way(make_way(10, &[1, 2, 3]));
        graph.insert_way(make_way(20, &[2, 4, 5]));
        graph.insert_way(make_way(30, &[6, 2, 7]));
    }

    fn make_way(id: u64, point_ids: &[u64]) -> OsmWay {
        OsmWay {
            id,
            point_ids: point_ids.to_vec(),
            tags: Some(HashMap::from([(
                "highway".to_string(),
                "primary".to_string(),
            )])),
        }
    }

    fn make_relation(
        id: u64,
        restriction: &str,
        members: Vec<OsmRelationMember>,
        extra_tags: HashMap<String, String>,
    ) -> OsmRelation {
        let mut tags = HashMap::from([
            ("type".to_string(), "restriction".to_string()),
            ("restriction".to_string(), restriction.to_string()),
        ]);
        tags.extend(extra_tags);

        OsmRelation { id, members, tags }
    }

    fn make_relation_with_tags(
        id: u64,
        members: Vec<OsmRelationMember>,
        tags: HashMap<String, String>,
    ) -> OsmRelation {
        OsmRelation { id, members, tags }
    }

    fn relation_way_member(role: OsmRelationMemberRole, member_ref: u64) -> OsmRelationMember {
        OsmRelationMember {
            member_type: OsmRelationMemberType::Way,
            role,
            member_ref,
        }
    }

    fn relation_node_member(role: OsmRelationMemberRole, member_ref: u64) -> OsmRelationMember {
        OsmRelationMember {
            member_type: OsmRelationMemberType::Node,
            role,
            member_ref,
        }
    }

    #[derive(Clone, Default)]
    struct SharedLogBuffer {
        inner: Arc<Mutex<Vec<u8>>>,
    }

    impl SharedLogBuffer {
        fn as_string(&self) -> String {
            String::from_utf8(self.inner.lock().unwrap().clone()).unwrap()
        }
    }

    impl<'a> MakeWriter<'a> for SharedLogBuffer {
        type Writer = SharedLogWriter;

        fn make_writer(&'a self) -> Self::Writer {
            SharedLogWriter {
                inner: self.inner.clone(),
            }
        }
    }

    struct SharedLogWriter {
        inner: Arc<Mutex<Vec<u8>>>,
    }

    impl io::Write for SharedLogWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.inner.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
