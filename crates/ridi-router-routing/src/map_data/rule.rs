use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use super::graph::MapDataLineRef;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MapDataRuleType {
    OnlyAllowed,
    NotAllowed,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct MapDataRule {
    pub from_lines: Vec<MapDataLineRef>,
    pub to_lines: Vec<MapDataLineRef>,
    pub rule_type: MapDataRuleType,
}
impl Debug for MapDataRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapDataRule")
            .field("rule_type", &self.rule_type)
            .field("from_line_count", &self.from_lines.len())
            .field("to_line_count", &self.to_lines.len())
            .finish()
    }
}
