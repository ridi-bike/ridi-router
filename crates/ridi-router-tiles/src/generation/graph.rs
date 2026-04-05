use std::collections::{BTreeSet, HashMap};

use tracing::warn;

use super::{
    GenerationLine, GenerationPoint, GenerationRestrictionRule, GenerationRestrictionRuleType,
    GenerationRestrictionSkipReason, GenerationRestrictionSkipStats, GenerationTags, LineDirection,
};
use crate::osm_data::{OsmNode, OsmRelation, OsmRelationMemberRole, OsmRelationMemberType, OsmWay};

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

        let mut emitted_line_indices = Vec::new();

        for i in 0..way.point_ids.len() - 1 {
            let from_id = way.point_ids[i];
            let to_id = way.point_ids[i + 1];

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
                .entry(way.id)
                .or_default()
                .extend(emitted_line_indices);
        }
    }

    pub fn insert_relation(&mut self, relation: OsmRelation) {
        if relation.tags.get("type").map(|value| value.as_str()) != Some("restriction") {
            self.warn_and_count_skipped_relation(
                relation.id,
                GenerationRestrictionSkipReason::MalformedOrUnresolved,
                format!(
                    "unsupported relation type {:?}; expected type=restriction",
                    relation.tags.get("type")
                ),
            );
            return;
        }

        if relation
            .tags
            .keys()
            .any(|key| key.ends_with(":conditional"))
        {
            self.warn_and_count_skipped_relation(
                relation.id,
                GenerationRestrictionSkipReason::Conditional,
                "conditional restrictions are not supported".to_string(),
            );
            return;
        }

        if relation.tags.contains_key("except") {
            self.warn_and_count_skipped_relation(
                relation.id,
                GenerationRestrictionSkipReason::Except,
                "except=* restrictions are not supported".to_string(),
            );
            return;
        }

        if relation
            .tags
            .keys()
            .any(|key| key.starts_with("restriction:"))
        {
            self.warn_and_count_skipped_relation(
                relation.id,
                GenerationRestrictionSkipReason::RestrictionVariant,
                "restriction:* variants are not supported".to_string(),
            );
            return;
        }

        let Some(restriction_value) = relation.tags.get("restriction") else {
            self.warn_and_count_skipped_relation(
                relation.id,
                GenerationRestrictionSkipReason::MalformedOrUnresolved,
                "missing restriction=* tag".to_string(),
            );
            return;
        };

        let Some(rule_type) = Self::map_restriction_value(restriction_value) else {
            self.warn_and_count_skipped_relation(
                relation.id,
                GenerationRestrictionSkipReason::MalformedOrUnresolved,
                format!("unsupported restriction value {restriction_value:?}"),
            );
            return;
        };

        let mut from_way_ids = Vec::new();
        let mut to_way_ids = Vec::new();
        let mut via_node_ids = Vec::new();

        for member in &relation.members {
            match (&member.role, &member.member_type) {
                (OsmRelationMemberRole::From, OsmRelationMemberType::Way) => {
                    from_way_ids.push(member.member_ref);
                }
                (OsmRelationMemberRole::From, _) => {
                    self.warn_and_count_skipped_relation(
                        relation.id,
                        GenerationRestrictionSkipReason::MalformedOrUnresolved,
                        format!(
                            "from member {} must be a way, got {:?}",
                            member.member_ref, member.member_type
                        ),
                    );
                    return;
                }
                (OsmRelationMemberRole::To, OsmRelationMemberType::Way) => {
                    to_way_ids.push(member.member_ref);
                }
                (OsmRelationMemberRole::To, _) => {
                    self.warn_and_count_skipped_relation(
                        relation.id,
                        GenerationRestrictionSkipReason::MalformedOrUnresolved,
                        format!(
                            "to member {} must be a way, got {:?}",
                            member.member_ref, member.member_type
                        ),
                    );
                    return;
                }
                (OsmRelationMemberRole::Via, OsmRelationMemberType::Node) => {
                    via_node_ids.push(member.member_ref);
                }
                (OsmRelationMemberRole::Via, OsmRelationMemberType::Way) => {
                    self.warn_and_count_skipped_relation(
                        relation.id,
                        GenerationRestrictionSkipReason::ViaWay,
                        "via-way restrictions are not supported".to_string(),
                    );
                    return;
                }
                (OsmRelationMemberRole::Via, _) => {
                    self.warn_and_count_skipped_relation(
                        relation.id,
                        GenerationRestrictionSkipReason::MalformedOrUnresolved,
                        format!(
                            "via member {} must be a node, got {:?}",
                            member.member_ref, member.member_type
                        ),
                    );
                    return;
                }
                (OsmRelationMemberRole::Other(_), _) => {}
            }
        }

        if from_way_ids.is_empty() {
            self.warn_and_count_skipped_relation(
                relation.id,
                GenerationRestrictionSkipReason::MalformedOrUnresolved,
                "restriction relation has no from way members".to_string(),
            );
            return;
        }

        if to_way_ids.is_empty() {
            self.warn_and_count_skipped_relation(
                relation.id,
                GenerationRestrictionSkipReason::MalformedOrUnresolved,
                "restriction relation has no to way members".to_string(),
            );
            return;
        }

        if via_node_ids.len() != 1 {
            self.warn_and_count_skipped_relation(
                relation.id,
                GenerationRestrictionSkipReason::MalformedOrUnresolved,
                format!(
                    "restriction relation must have exactly one via node, got {}",
                    via_node_ids.len()
                ),
            );
            return;
        }

        let via_node_id = via_node_ids[0];
        if !self.points_map.contains_key(&via_node_id) {
            self.warn_and_count_skipped_relation(
                relation.id,
                GenerationRestrictionSkipReason::MalformedOrUnresolved,
                format!("via node {via_node_id} is not present in this tile"),
            );
            return;
        }

        let from_line_indices = self.resolve_via_adjacent_line_indices(&from_way_ids, via_node_id);
        if from_line_indices.is_empty() {
            self.warn_and_count_skipped_relation(
                relation.id,
                GenerationRestrictionSkipReason::MalformedOrUnresolved,
                format!("no via-adjacent local from lines found for via node {via_node_id}"),
            );
            return;
        }

        let Some(to_line_indices) = self.resolve_required_to_line_indices(&to_way_ids, via_node_id)
        else {
            self.warn_and_count_skipped_relation(
                relation.id,
                GenerationRestrictionSkipReason::MalformedOrUnresolved,
                format!(
                    "at least one to way is missing via-adjacent local lines for via node {via_node_id}; skipping in this tile so an adjacent tile can materialize it"
                ),
            );
            return;
        };

        self.restrictions_by_via
            .entry(via_node_id)
            .or_default()
            .push(GenerationRestrictionRule {
                relation_id: relation.id,
                via_node_id,
                rule_type,
                from_line_indices,
                to_line_indices,
            });
    }

    pub fn log_restriction_skip_summary(&self) {
        if self.restriction_skip_stats.is_empty() {
            return;
        }

        warn!(
            "Skipped restriction relations during tile generation: {}",
            self.restriction_skip_stats.summary_parts().join(", ")
        );
    }

    pub fn generate_point_hashes(&mut self) {
        // TODO: Implement spatial hashing if needed.
    }

    fn warn_and_count_skipped_relation(
        &mut self,
        relation_id: u64,
        reason: GenerationRestrictionSkipReason,
        details: String,
    ) {
        self.restriction_skip_stats.increment(reason);
        warn!(
            "Skipping restriction relation {} [{}]: {}",
            relation_id,
            reason.label(),
            details
        );
    }

    fn map_restriction_value(value: &str) -> Option<GenerationRestrictionRuleType> {
        match value {
            "no_left_turn" | "no_right_turn" | "no_straight_on" | "no_u_turn" | "no_entry"
            | "no_exit" => Some(GenerationRestrictionRuleType::NotAllowed),
            "only_left_turn" | "only_right_turn" | "only_straight_on" | "only_u_turn" => {
                Some(GenerationRestrictionRuleType::OnlyAllowed)
            }
            _ => None,
        }
    }

    fn resolve_via_adjacent_line_indices(&self, way_ids: &[u64], via_node_id: u64) -> Vec<u32> {
        let mut line_indices = BTreeSet::new();

        for &way_id in way_ids {
            line_indices
                .extend(self.resolve_via_adjacent_line_indices_for_way(way_id, via_node_id));
        }

        line_indices.into_iter().collect()
    }

    fn resolve_required_to_line_indices(
        &self,
        way_ids: &[u64],
        via_node_id: u64,
    ) -> Option<Vec<u32>> {
        let mut line_indices = BTreeSet::new();

        for &way_id in way_ids {
            let way_line_indices =
                self.resolve_via_adjacent_line_indices_for_way(way_id, via_node_id);
            if way_line_indices.is_empty() {
                return None;
            }
            line_indices.extend(way_line_indices);
        }

        Some(line_indices.into_iter().collect())
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
    use crate::osm_data::{OsmRelationMember, OsmRelationMemberRole, OsmRelationMemberType};

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
    fn test_insert_relation_skips_unsupported_and_malformed_relations() {
        let mut graph = seeded_graph();
        insert_base_ways(&mut graph);

        graph.insert_relation(make_relation(
            400,
            "no_right_turn",
            vec![
                relation_way_member(OsmRelationMemberRole::From, 10),
                relation_way_member(OsmRelationMemberRole::Via, 20),
                relation_way_member(OsmRelationMemberRole::To, 30),
            ],
            HashMap::new(),
        ));
        graph.insert_relation(make_relation(
            401,
            "no_right_turn",
            vec![
                relation_way_member(OsmRelationMemberRole::From, 10),
                relation_node_member(OsmRelationMemberRole::Via, 2),
                relation_way_member(OsmRelationMemberRole::To, 30),
            ],
            HashMap::from([(
                "restriction:conditional".to_string(),
                "no_right_turn @ (Mo-Fr 08:00-18:00)".to_string(),
            )]),
        ));
        graph.insert_relation(make_relation(
            402,
            "no_right_turn",
            vec![
                relation_way_member(OsmRelationMemberRole::From, 10),
                relation_node_member(OsmRelationMemberRole::Via, 2),
                relation_way_member(OsmRelationMemberRole::To, 30),
            ],
            HashMap::from([(
                "restriction:motorcycle".to_string(),
                "no_right_turn".to_string(),
            )]),
        ));
        graph.insert_relation(make_relation(
            403,
            "no_right_turn",
            vec![
                relation_way_member(OsmRelationMemberRole::From, 10),
                relation_node_member(OsmRelationMemberRole::Via, 2),
                relation_way_member(OsmRelationMemberRole::To, 30),
            ],
            HashMap::from([("except".to_string(), "bicycle".to_string())]),
        ));
        graph.insert_relation(make_relation(
            404,
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
        assert_eq!(graph.restriction_skip_stats.conditional_relations, 1);
        assert_eq!(
            graph.restriction_skip_stats.restriction_variant_relations,
            1
        );
        assert_eq!(graph.restriction_skip_stats.except_relations, 1);
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
