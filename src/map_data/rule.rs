use super::line::MapDataLineRef;
use std::fmt::Debug;

#[derive(Debug, Clone, PartialEq)]
pub enum MapDataRuleType {
    OnlyAllowed,
    NotAllowed,
}

#[derive(Clone)]
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
                .map(|l| l.borrow().id.clone())
                .collect::<Vec<_>>(),
            if self.rule_type == MapDataRuleType::OnlyAllowed {
                "--->"
            } else {
                "-x->"
            },
            self.to_lines
                .iter()
                .map(|l| l.borrow().id.clone())
                .collect::<Vec<_>>(),
        )
    }
}
