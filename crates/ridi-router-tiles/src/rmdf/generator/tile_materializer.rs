use crate::osm_data::in_memory_pbf::InMemoryPbf;
use crate::rmdf::format::TileBounds;
use anyhow::Result;
use geo::{Coord, Intersects, Line, Rect};
use ridi_router_common::osm::{
    OsmNode, OsmRelation, OsmRelationMemberRole, OsmRelationMemberType, OsmWay,
};
use std::collections::{HashMap, HashSet};

const ALLOWED_HIGHWAY_VALUES: [&str; 17] = [
    "motorway",
    "trunk",
    "primary",
    "secondary",
    "tertiary",
    "unclassified",
    "residential",
    "motorway_link",
    "trunk_link",
    "primary_link",
    "secondary_link",
    "tertiary_link",
    "living_street",
    "track",
    "escape",
    "raceway",
    "road",
];

#[derive(Debug, Default)]
pub(crate) struct MaterializedTileData {
    pub(crate) nodes: HashMap<u64, OsmNode>,
    pub(crate) ways: Vec<OsmWay>,
    pub(crate) relations: Vec<OsmRelation>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum MaterializationError {
    #[error(
        "locally relevant segment for way {way_id} is missing endpoint nodes {from_node_id}->{to_node_id}"
    )]
    MissingLocallyRelevantSegmentEndpoints {
        way_id: u64,
        from_node_id: u64,
        to_node_id: u64,
    },
}

type RestrictionSupport = (Vec<u64>, u64, Vec<u64>);

pub(crate) fn extract_materialized_tile(
    pbf_data: &InMemoryPbf,
    buffered_bounds: TileBounds,
) -> Result<MaterializedTileData> {
    let mut tile = MaterializedTileData::default();
    let mut way_ids = HashSet::new();

    for node in pbf_data.query_nodes_in_bounds(&buffered_bounds) {
        tile.nodes.insert(node.id, node.clone());
    }

    for way_with_bounds in pbf_data.query_ways_in_bounds(&buffered_bounds) {
        let way = &way_with_bounds.way;
        if !is_routable_highway(way) {
            continue;
        }

        if materialize_locally_relevant_way(pbf_data, way, &buffered_bounds, &mut tile.nodes) {
            insert_way_once(&mut tile.ways, &mut way_ids, way.clone());
        }
    }

    expand_restriction_support(pbf_data, &buffered_bounds, &mut tile, &mut way_ids);
    validate_materialized_tile(pbf_data, &buffered_bounds, &tile)?;

    if tile.nodes.is_empty() && tile.ways.is_empty() {
        return Ok(tile);
    }

    let node_ids: HashSet<u64> = tile.nodes.keys().copied().collect();

    for rel_with_bounds in pbf_data.query_relations_in_bounds(&buffered_bounds) {
        let relation = &rel_with_bounds.relation;
        if !is_restriction_relation(relation) {
            continue;
        }

        if relation_has_members_in_closure(relation, &node_ids, &way_ids) {
            tile.relations.push(relation.clone());
        }
    }

    Ok(tile)
}

fn is_routable_highway(way: &OsmWay) -> bool {
    let Some(tags) = way.tags.as_ref() else {
        return false;
    };

    let Some(highway_value) = tags.get("highway") else {
        return false;
    };

    ALLOWED_HIGHWAY_VALUES.contains(&highway_value.as_str())
        || (highway_value == "path" && tags.get("motorcycle").map(|v| v.as_str()) == Some("yes"))
}

fn is_restriction_relation(relation: &OsmRelation) -> bool {
    relation
        .tags
        .get("type")
        .map(|value| value.starts_with("restriction"))
        .unwrap_or(false)
}

fn materialize_locally_relevant_way(
    pbf_data: &InMemoryPbf,
    way: &OsmWay,
    bounds: &TileBounds,
    tile_nodes: &mut HashMap<u64, OsmNode>,
) -> bool {
    let mut kept_segment = false;

    for segment in way.point_ids.windows(2) {
        let Some(from_node) = pbf_data.node_by_osm_id(segment[0]) else {
            continue;
        };
        let Some(to_node) = pbf_data.node_by_osm_id(segment[1]) else {
            continue;
        };

        if segment_intersects_bounds(from_node, to_node, bounds) {
            tile_nodes.insert(from_node.id, from_node.clone());
            tile_nodes.insert(to_node.id, to_node.clone());
            kept_segment = true;
        }
    }

    kept_segment
}

fn expand_restriction_support(
    pbf_data: &InMemoryPbf,
    bounds: &TileBounds,
    tile: &mut MaterializedTileData,
    way_ids: &mut HashSet<u64>,
) {
    for rel_with_bounds in pbf_data.query_relations_in_bounds(bounds) {
        let relation = &rel_with_bounds.relation;
        if !is_restriction_relation(relation) {
            continue;
        }

        let Some((from_way_ids, via_node_id, to_way_ids)) = parse_restriction_support(relation)
        else {
            continue;
        };

        let node_ids: HashSet<u64> = tile.nodes.keys().copied().collect();
        let has_members_in_closure = relation_has_members_in_closure(relation, &node_ids, way_ids);

        if !has_members_in_closure
            && !restriction_support_intersects_bounds(
                pbf_data,
                &from_way_ids,
                via_node_id,
                &to_way_ids,
                bounds,
            )
        {
            continue;
        }

        if let Some(via_node) = pbf_data.node_by_osm_id(via_node_id) {
            tile.nodes.insert(via_node_id, via_node.clone());
        }

        for &way_id in from_way_ids.iter().chain(to_way_ids.iter()) {
            let Some(way) = pbf_data.way_by_osm_id(way_id) else {
                continue;
            };
            if !is_routable_highway(way) {
                continue;
            }

            if materialize_locally_relevant_way(pbf_data, way, bounds, &mut tile.nodes) {
                insert_way_once(&mut tile.ways, way_ids, way.clone());
            }
        }
    }
}

fn validate_materialized_tile(
    pbf_data: &InMemoryPbf,
    bounds: &TileBounds,
    tile: &MaterializedTileData,
) -> Result<()> {
    for way in &tile.ways {
        for segment in way.point_ids.windows(2) {
            let Some(from_node) = pbf_data.node_by_osm_id(segment[0]) else {
                continue;
            };
            let Some(to_node) = pbf_data.node_by_osm_id(segment[1]) else {
                continue;
            };

            if segment_intersects_bounds(from_node, to_node, bounds)
                && (!tile.nodes.contains_key(&from_node.id)
                    || !tile.nodes.contains_key(&to_node.id))
            {
                return Err(
                    MaterializationError::MissingLocallyRelevantSegmentEndpoints {
                        way_id: way.id,
                        from_node_id: from_node.id,
                        to_node_id: to_node.id,
                    }
                    .into(),
                );
            }
        }
    }

    Ok(())
}

fn parse_restriction_support(relation: &OsmRelation) -> Option<RestrictionSupport> {
    let mut from_way_ids = Vec::new();
    let mut to_way_ids = Vec::new();
    let mut via_node_ids = Vec::new();

    for member in &relation.members {
        match (&member.role, &member.member_type) {
            (OsmRelationMemberRole::From, OsmRelationMemberType::Way) => {
                from_way_ids.push(member.member_ref);
            }
            (OsmRelationMemberRole::To, OsmRelationMemberType::Way) => {
                to_way_ids.push(member.member_ref);
            }
            (OsmRelationMemberRole::Via, OsmRelationMemberType::Node) => {
                via_node_ids.push(member.member_ref);
            }
            (OsmRelationMemberRole::Via, OsmRelationMemberType::Way) => return None,
            _ => {}
        }
    }

    if from_way_ids.is_empty() || to_way_ids.is_empty() || via_node_ids.len() != 1 {
        return None;
    }

    Some((from_way_ids, via_node_ids[0], to_way_ids))
}

fn relation_has_members_in_closure(
    relation: &OsmRelation,
    node_ids: &HashSet<u64>,
    way_ids: &HashSet<u64>,
) -> bool {
    relation
        .members
        .iter()
        .any(|member| match member.member_type {
            OsmRelationMemberType::Node => node_ids.contains(&member.member_ref),
            OsmRelationMemberType::Way => way_ids.contains(&member.member_ref),
            OsmRelationMemberType::Relation => true,
        })
}

fn restriction_support_intersects_bounds(
    pbf_data: &InMemoryPbf,
    from_way_ids: &[u64],
    via_node_id: u64,
    to_way_ids: &[u64],
    bounds: &TileBounds,
) -> bool {
    if pbf_data
        .node_by_osm_id(via_node_id)
        .is_some_and(|via_node| node_in_bounds(via_node, bounds))
    {
        return true;
    }

    from_way_ids.iter().chain(to_way_ids.iter()).any(|&way_id| {
        pbf_data.way_by_osm_id(way_id).is_some_and(|way| {
            way_has_via_adjacent_segment_in_bounds(pbf_data, way, via_node_id, bounds)
        })
    })
}

fn way_has_via_adjacent_segment_in_bounds(
    pbf_data: &InMemoryPbf,
    way: &OsmWay,
    via_node_id: u64,
    bounds: &TileBounds,
) -> bool {
    way.point_ids.windows(2).any(|segment| {
        if segment[0] != via_node_id && segment[1] != via_node_id {
            return false;
        }

        let Some(from_node) = pbf_data.node_by_osm_id(segment[0]) else {
            return false;
        };
        let Some(to_node) = pbf_data.node_by_osm_id(segment[1]) else {
            return false;
        };

        segment_intersects_bounds(from_node, to_node, bounds)
    })
}

fn insert_way_once(tile_ways: &mut Vec<OsmWay>, way_ids: &mut HashSet<u64>, way: OsmWay) {
    if way_ids.insert(way.id) {
        tile_ways.push(way);
    }
}

fn segment_intersects_bounds(from_node: &OsmNode, to_node: &OsmNode, bounds: &TileBounds) -> bool {
    if node_in_bounds(from_node, bounds) || node_in_bounds(to_node, bounds) {
        return true;
    }

    let rect = Rect::new(
        Coord {
            x: bounds.lon_min as f64,
            y: bounds.lat_min as f64,
        },
        Coord {
            x: bounds.lon_max as f64,
            y: bounds.lat_max as f64,
        },
    );
    let line = Line::new(
        Coord {
            x: from_node.lon,
            y: from_node.lat,
        },
        Coord {
            x: to_node.lon,
            y: to_node.lat,
        },
    );

    line.intersects(&rect)
}

fn node_in_bounds(node: &OsmNode, bounds: &TileBounds) -> bool {
    node.lat >= bounds.lat_min as f64
        && node.lat <= bounds.lat_max as f64
        && node.lon >= bounds.lon_min as f64
        && node.lon <= bounds.lon_max as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::GenerationGraph;
    use ridi_router_common::osm::{OsmRelationMember, OsmRelationMemberRole};

    #[test]
    fn materializes_sparse_border_crossing_way_without_seed_nodes() {
        let pbf = test_pbf(
            vec![make_node(1, 0.5, -0.1), make_node(2, 0.5, 1.1)],
            vec![make_way(10, &[1, 2])],
            vec![],
        );

        let tile = extract_materialized_tile(&pbf, test_bounds()).unwrap();
        let graph = build_graph(&tile);

        assert_eq!(tile.nodes.len(), 2);
        assert_eq!(tile.ways.len(), 1);
        assert_eq!(graph.get_lines().len(), 1);
        assert_eq!(graph.validate_line_endpoints(), Ok(()));
    }

    #[test]
    fn does_not_duplicate_entire_remote_tail_of_long_way() {
        let pbf = test_pbf(
            vec![
                make_node(1, 0.5, -0.1),
                make_node(2, 0.5, 0.5),
                make_node(3, 0.5, 1.1),
                make_node(4, 0.5, 5.0),
            ],
            vec![make_way(10, &[1, 2, 3, 4])],
            vec![],
        );

        let tile = extract_materialized_tile(&pbf, test_bounds()).unwrap();

        assert!(tile.nodes.contains_key(&1));
        assert!(tile.nodes.contains_key(&2));
        assert!(tile.nodes.contains_key(&3));
        assert!(!tile.nodes.contains_key(&4));
    }

    #[test]
    fn keeps_restriction_support_after_border_node_expansion() {
        let pbf = test_pbf(
            vec![
                make_node(1, 0.5, -0.1),
                make_node(2, 0.5, 0.5),
                make_node(3, 0.5, 1.1),
            ],
            vec![make_way(10, &[1, 2]), make_way(20, &[2, 3])],
            vec![make_relation(100, 10, 2, 20)],
        );

        let tile = extract_materialized_tile(&pbf, test_bounds()).unwrap();
        let graph = build_graph(&tile);

        assert_eq!(tile.relations.len(), 1);
        assert!(graph.get_restrictions_by_via().contains_key(&2));
    }

    #[test]
    fn relation_driven_expansion_adds_missing_support_way() {
        let pbf = test_pbf(
            vec![
                make_node(1, 0.5, -0.1),
                make_node(2, 0.5, 0.5),
                make_node(3, 0.5, 1.1),
            ],
            vec![make_way(10, &[1, 2]), make_way(20, &[2, 3])],
            vec![make_relation(100, 10, 2, 20)],
        );

        let mut tile = MaterializedTileData {
            nodes: HashMap::from([(1, make_node(1, 0.5, -0.1)), (2, make_node(2, 0.5, 0.5))]),
            ways: vec![make_way(10, &[1, 2])],
            relations: Vec::new(),
        };
        let mut way_ids = HashSet::from([10]);

        expand_restriction_support(&pbf, &test_bounds(), &mut tile, &mut way_ids);

        assert!(way_ids.contains(&20));
        assert!(tile.nodes.contains_key(&3));
        assert!(tile.ways.iter().any(|way| way.id == 20));
    }

    #[test]
    fn unresolved_restriction_still_skips_after_expansion() {
        let pbf = test_pbf(
            vec![
                make_node(1, 0.5, 0.5),
                make_node(2, 0.5, 1.1),
                make_node(3, 0.5, 5.0),
            ],
            vec![make_way(10, &[1, 2]), make_way(20, &[2, 3])],
            vec![make_relation(100, 10, 2, 20)],
        );

        let tile = extract_materialized_tile(&pbf, test_bounds()).unwrap();
        let graph = build_graph(&tile);

        assert_eq!(tile.relations.len(), 1);
        assert!(tile.ways.iter().any(|way| way.id == 10));
        assert!(!tile.ways.iter().any(|way| way.id == 20));
        assert!(graph.get_restrictions_by_via().is_empty());
        assert_eq!(
            graph
                .get_restriction_skip_stats()
                .malformed_or_unresolved_relations,
            1
        );
    }

    #[test]
    fn validation_rejects_missing_locally_relevant_segment_endpoints() {
        let pbf = test_pbf(
            vec![make_node(1, 0.5, -0.1), make_node(2, 0.5, 1.1)],
            vec![make_way(10, &[1, 2])],
            vec![],
        );
        let tile = MaterializedTileData {
            nodes: HashMap::from([(1, make_node(1, 0.5, -0.1))]),
            ways: vec![make_way(10, &[1, 2])],
            relations: Vec::new(),
        };

        let err = validate_materialized_tile(&pbf, &test_bounds(), &tile)
            .unwrap_err()
            .downcast::<MaterializationError>()
            .unwrap();

        assert_eq!(
            err,
            MaterializationError::MissingLocallyRelevantSegmentEndpoints {
                way_id: 10,
                from_node_id: 1,
                to_node_id: 2,
            }
        );
    }

    fn build_graph(tile: &MaterializedTileData) -> GenerationGraph {
        let mut graph = GenerationGraph::new();

        let mut nodes: Vec<_> = tile.nodes.values().cloned().collect();
        nodes.sort_unstable_by_key(|node| node.id);
        for node in nodes {
            graph.insert_node(node);
        }
        for way in &tile.ways {
            graph.insert_way(way.clone());
        }
        for relation in &tile.relations {
            graph.insert_relation(relation.clone());
        }

        graph
    }

    fn test_bounds() -> TileBounds {
        TileBounds {
            lat_min: 0.0,
            lat_max: 1.0,
            lon_min: 0.0,
            lon_max: 1.0,
        }
    }

    fn test_pbf(
        nodes: Vec<OsmNode>,
        ways: Vec<OsmWay>,
        relations: Vec<OsmRelation>,
    ) -> InMemoryPbf {
        InMemoryPbf::from_parts(nodes, ways, relations)
    }

    fn make_node(id: u64, lat: f64, lon: f64) -> OsmNode {
        OsmNode {
            id,
            lat,
            lon,
            residential_in_proximity: false,
            nogo_area: false,
        }
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

    fn make_relation(id: u64, from_way: u64, via_node: u64, to_way: u64) -> OsmRelation {
        OsmRelation {
            id,
            members: vec![
                OsmRelationMember {
                    member_type: OsmRelationMemberType::Way,
                    role: OsmRelationMemberRole::From,
                    member_ref: from_way,
                },
                OsmRelationMember {
                    member_type: OsmRelationMemberType::Node,
                    role: OsmRelationMemberRole::Via,
                    member_ref: via_node,
                },
                OsmRelationMember {
                    member_type: OsmRelationMemberType::Way,
                    role: OsmRelationMemberRole::To,
                    member_ref: to_way,
                },
            ],
            tags: HashMap::from([
                ("type".to_string(), "restriction".to_string()),
                ("restriction".to_string(), "no_right_turn".to_string()),
            ]),
        }
    }
}
