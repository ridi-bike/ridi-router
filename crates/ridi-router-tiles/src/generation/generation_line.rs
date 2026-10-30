use serde::{Deserialize, Serialize};

#[cfg(test)]
use super::TagSetId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineDirection {
    BothWays = 0,
    OneWay = 1,
    Roundabout = 2,
}

/// Line of the test-only `GenerationGraph`.
#[cfg(test)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenerationLine {
    pub from_node_id: u64,
    pub to_node_id: u64,
    pub direction: LineDirection,
    pub tags: TagSetId,
}
