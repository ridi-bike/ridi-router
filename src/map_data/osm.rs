use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct OsmNode {
    pub id: u64,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OsmWay {
    pub id: u64,
    pub point_ids: Vec<u64>,
    pub tags: Option<HashMap<String, String>>,
}

impl OsmWay {
    pub fn is_one_way(&self) -> bool {
        if let Some(tags) = &self.tags {
            tags.get("oneway").map_or(false, |one_way| one_way == "yes")
                || tags
                    .get("junction")
                    .map_or(false, |junction| junction == "roundabout")
        } else {
            false
        }
    }

    pub fn is_roundabout(&self) -> bool {
        if let Some(tags) = &self.tags {
            tags.get("junction")
                .map_or(false, |junction| junction == "roundabout")
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OsmRelationMemberType {
    Way,
    Node,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OsmRelationMemberRole {
    From,
    To,
    Via,
    Other(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct OsmRelationMember {
    pub member_type: OsmRelationMemberType,
    pub role: OsmRelationMemberRole,
    pub member_ref: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OsmRelation {
    pub id: u64,
    pub members: Vec<OsmRelationMember>,
    pub tags: HashMap<String, String>,
}
