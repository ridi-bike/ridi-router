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
        write!(
            f,
            "({:?}){}({:?})",
            self.from_lines
                .iter()
                .map(|l| l.borrow().line_id())
                .collect::<Vec<_>>(),
            if self.rule_type == MapDataRuleType::OnlyAllowed {
                "--->"
            } else {
                "-x->"
            },
            self.to_lines
                .iter()
                .map(|l| l.borrow().line_id())
                .collect::<Vec<_>>(),
        )
    }
}
