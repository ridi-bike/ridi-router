use self::osm::OsmRelation;

pub mod graph;
pub mod line;
pub mod osm;
pub mod point;
mod proximity;
pub mod rule;

#[derive(Debug, PartialEq, Clone, thiserror::Error)]
pub enum MapDataError {
    #[error("Missing point with ID: {point_id}")]
    MissingPoint { point_id: u64 },

    #[error("Missing restriction for relation {relation_id}: {osm_relation:?}")]
    MissingRestriction {
        osm_relation: OsmRelation,
        relation_id: u64,
    },

    #[error("Unknown restriction type '{restriction}' in relation {relation_id}")]
    UnknownRestriction {
        relation_id: u64,
        restriction: String,
    },

    #[error("Missing 'via' member in relation {relation_id}")]
    MissingViaMember { relation_id: u64 },

    #[error("Missing 'via' point {point_id} in relation {relation_id}")]
    MissingViaPoint { relation_id: u64, point_id: u64 },

    #[error("Way {way_id} is not linked with 'via' point {point_id} in relation {relation_id}")]
    WayIdNotLinkedWithViaPoint {
        relation_id: u64,
        point_id: u64,
        way_id: u64,
    },

    #[error("{message} - Relation: {relation:?}")]
    NotYetImplemented {
        message: String,
        relation: OsmRelation,
    },
}
