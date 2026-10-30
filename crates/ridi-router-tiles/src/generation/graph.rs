use std::collections::{BTreeSet, HashMap};

use tracing::warn;

use super::{
    GenerationRestrictionRule, GenerationRestrictionRuleType, GenerationRestrictionSkipReason,
    GenerationRestrictionSkipStats,
};
use ridi_router_common::osm::{OsmRelation, OsmRelationMemberRole, OsmRelationMemberType};

/// What restriction materialization needs to know about a tile's graph.
pub(crate) trait RestrictionContext {
    fn has_point(&self, node_id: u64) -> bool;
    /// Indices (ascending, unique) of this way's local lines touching `via_node_id`.
    fn via_adjacent_lines_for_way(&self, way_id: u64, via_node_id: u64) -> Vec<u32>;
}

fn skip_relation(
    stats: &mut GenerationRestrictionSkipStats,
    log: bool,
    relation_id: u64,
    reason: GenerationRestrictionSkipReason,
    details: impl FnOnce() -> String,
) {
    stats.increment(reason);
    if log {
        warn!(
            "Skipping restriction relation {} [{}]: {}",
            relation_id,
            reason.label(),
            details()
        );
    }
}

fn resolve_required_via_adjacent_line_indices<C: RestrictionContext + ?Sized>(
    ctx: &C,
    way_ids: &[u64],
    via_node_id: u64,
) -> Option<Vec<u32>> {
    let mut line_indices = BTreeSet::new();

    for &way_id in way_ids {
        let way_line_indices = ctx.via_adjacent_lines_for_way(way_id, via_node_id);
        if way_line_indices.is_empty() {
            return None;
        }
        line_indices.extend(way_line_indices);
    }

    Some(line_indices.into_iter().collect())
}

/// Turn a restriction relation into a rule for this tile (the semantics of
/// the reference `insert_relation`). Skips are counted in `stats`; with `log` each skip is
/// also logged individually (the reference behaviour).
pub(crate) fn materialize_restriction<C: RestrictionContext + ?Sized>(
    ctx: &C,
    relation: &OsmRelation,
    stats: &mut GenerationRestrictionSkipStats,
    log: bool,
) -> Option<GenerationRestrictionRule> {
    use GenerationRestrictionSkipReason::{MalformedOrUnresolved, ViaWay};

    if relation.tags.get("type").map(|value| value.as_str()) != Some("restriction") {
        skip_relation(stats, log, relation.id, MalformedOrUnresolved, || {
            format!(
                "unsupported relation type {:?}; expected type=restriction",
                relation.tags.get("type")
            )
        });
        return None;
    }

    let restriction_value = resolve_motorcycle_restriction_value(relation, stats, log)?;

    let Some(rule_type) = map_restriction_value(&restriction_value) else {
        skip_relation(stats, log, relation.id, MalformedOrUnresolved, || {
            format!("unsupported restriction value {restriction_value:?}")
        });
        return None;
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
                skip_relation(stats, log, relation.id, MalformedOrUnresolved, || {
                    format!(
                        "from member {} must be a way, got {:?}",
                        member.member_ref, member.member_type
                    )
                });
                return None;
            }
            (OsmRelationMemberRole::To, OsmRelationMemberType::Way) => {
                to_way_ids.push(member.member_ref);
            }
            (OsmRelationMemberRole::To, _) => {
                skip_relation(stats, log, relation.id, MalformedOrUnresolved, || {
                    format!(
                        "to member {} must be a way, got {:?}",
                        member.member_ref, member.member_type
                    )
                });
                return None;
            }
            (OsmRelationMemberRole::Via, OsmRelationMemberType::Node) => {
                via_node_ids.push(member.member_ref);
            }
            (OsmRelationMemberRole::Via, OsmRelationMemberType::Way) => {
                skip_relation(stats, log, relation.id, ViaWay, || {
                    "via-way restrictions are not supported".to_string()
                });
                return None;
            }
            (OsmRelationMemberRole::Via, _) => {
                skip_relation(stats, log, relation.id, MalformedOrUnresolved, || {
                    format!(
                        "via member {} must be a node, got {:?}",
                        member.member_ref, member.member_type
                    )
                });
                return None;
            }
            (OsmRelationMemberRole::Other(_), _) => {}
        }
    }

    if from_way_ids.is_empty() {
        skip_relation(stats, log, relation.id, MalformedOrUnresolved, || {
            "restriction relation has no from way members".to_string()
        });
        return None;
    }

    if to_way_ids.is_empty() {
        skip_relation(stats, log, relation.id, MalformedOrUnresolved, || {
            "restriction relation has no to way members".to_string()
        });
        return None;
    }

    if via_node_ids.len() != 1 {
        skip_relation(stats, log, relation.id, MalformedOrUnresolved, || {
            format!(
                "restriction relation must have exactly one via node, got {}",
                via_node_ids.len()
            )
        });
        return None;
    }

    let via_node_id = via_node_ids[0];
    if !ctx.has_point(via_node_id) {
        skip_relation(stats, log, relation.id, MalformedOrUnresolved, || {
            format!("via node {via_node_id} is not present in this tile")
        });
        return None;
    }

    let Some(from_line_indices) =
        resolve_required_via_adjacent_line_indices(ctx, &from_way_ids, via_node_id)
    else {
        skip_relation(stats, log, relation.id, MalformedOrUnresolved, || {
            format!(
                "at least one from way is missing via-adjacent local lines for via node {via_node_id}; skipping in this tile so an adjacent tile can materialize it"
            )
        });
        return None;
    };

    let Some(to_line_indices) =
        resolve_required_via_adjacent_line_indices(ctx, &to_way_ids, via_node_id)
    else {
        skip_relation(stats, log, relation.id, MalformedOrUnresolved, || {
            format!(
                "at least one to way is missing via-adjacent local lines for via node {via_node_id}; skipping in this tile so an adjacent tile can materialize it"
            )
        });
        return None;
    };

    Some(GenerationRestrictionRule {
        relation_id: relation.id,
        via_node_id,
        rule_type,
        from_line_indices,
        to_line_indices,
    })
}

fn resolve_motorcycle_restriction_value(
    relation: &OsmRelation,
    stats: &mut GenerationRestrictionSkipStats,
    log: bool,
) -> Option<String> {
    if except_list_excludes_motorcycle(relation.tags.get("except").map(String::as_str)) {
        return None;
    }

    if let Some(value) = first_motorcycle_conditional_restriction_value(&relation.tags) {
        return Some(value);
    }

    if let Some(value) = first_motorcycle_variant_restriction_value(&relation.tags) {
        return Some(value.to_string());
    }

    if let Some(value) = relation.tags.get("restriction") {
        return Some(value.to_string());
    }

    if let Some(value) = generic_conditional_restriction_value(&relation.tags) {
        return Some(value);
    }

    if has_any_restriction_key(&relation.tags) {
        return None;
    }

    skip_relation(
        stats,
        log,
        relation.id,
        GenerationRestrictionSkipReason::MalformedOrUnresolved,
        || "missing restriction=* tag".to_string(),
    );
    None
}

fn except_list_excludes_motorcycle(except: Option<&str>) -> bool {
    except
        .map(|value| {
            value
                .split([';', ',', ' '])
                .map(str::trim)
                .any(restriction_vehicle_applies_to_motorcycle)
        })
        .unwrap_or(false)
}

fn first_motorcycle_conditional_restriction_value(
    tags: &HashMap<String, String>,
) -> Option<String> {
    [
        "restriction:motorcycle:conditional",
        "restriction:motor_vehicle:conditional",
        "restriction:vehicle:conditional",
    ]
    .into_iter()
    .find_map(|key| {
        tags.get(key)
            .and_then(|value| parse_conditional_restriction_value(value))
    })
}

fn first_motorcycle_variant_restriction_value(tags: &HashMap<String, String>) -> Option<&str> {
    [
        "restriction:motorcycle",
        "restriction:motor_vehicle",
        "restriction:vehicle",
    ]
    .into_iter()
    .find_map(|key| tags.get(key).map(String::as_str))
}

fn generic_conditional_restriction_value(tags: &HashMap<String, String>) -> Option<String> {
    tags.get("restriction:conditional")
        .and_then(|value| parse_conditional_restriction_value(value))
}

fn parse_conditional_restriction_value(value: &str) -> Option<String> {
    value.split(';').find_map(|clause| {
        let restriction = clause
            .split_once('@')
            .map(|(restriction, _condition)| restriction)
            .unwrap_or(clause)
            .trim();

        (!restriction.is_empty()).then(|| restriction.to_string())
    })
}

fn restriction_vehicle_applies_to_motorcycle(vehicle: &str) -> bool {
    matches!(vehicle, "motorcycle" | "motor_vehicle" | "vehicle")
}

fn has_any_restriction_key(tags: &HashMap<String, String>) -> bool {
    tags.keys().any(|key| key.starts_with("restriction:"))
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
