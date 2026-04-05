mod generation_line;
mod graph;
mod point;
mod restriction;
mod tags;

pub use generation_line::{GenerationLine, LineDirection};
pub use graph::GenerationGraph;
pub use point::GenerationPoint;
pub use restriction::{
    GenerationRestrictionRule, GenerationRestrictionRuleType, GenerationRestrictionSkipReason,
    GenerationRestrictionSkipStats,
};
#[allow(unused_imports)]
pub use tags::{GenerationTags, TagSet, TagSetId, TagValueId};

#[derive(Debug, PartialEq, Clone, thiserror::Error)]
pub enum GenerationError {
    #[error("Missing point with ID: {point_id}")]
    MissingPoint { point_id: u64 },
}
