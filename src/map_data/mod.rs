use self::osm::OsmRelation;

pub mod graph;
pub mod line;
pub mod osm;
pub mod point;
pub mod rule;
pub mod way;

#[derive(Debug, PartialEq, Clone)]
pub enum MapDataError {
    MissingPoint {
        point_id: u64,
    },
    MissingRestriction {
        osm_relation: OsmRelation,
        relation_id: u64,
    },
    UnknownRestriction {
        relation_id: u64,
        restriction: String,
    },
    MissingViaMember {
        relation_id: u64,
    },
    MissingViaPoint {
        relation_id: u64,
        point_id: u64,
    },
    WayIdNotLinkedWithViaPoint {
        relation_id: u64,
        point_id: u64,
        way_id: u64,
    },
    NotYetImplemented {
        message: String,
        relation: OsmRelation,
    },
}
