mod generation_line;
mod graph;
mod point;
mod tags;

pub use generation_line::{GenerationLine, LineDirection};
pub use graph::GenerationGraph;
pub use point::GenerationPoint;
#[allow(unused_imports)]
pub use tags::{GenerationTags, TagSet, TagSetId, TagValueId};

#[derive(Debug, PartialEq, Clone, thiserror::Error)]
pub enum GenerationError {
    #[error("Missing point with ID: {point_id}")]
    MissingPoint { point_id: u64 },
}
