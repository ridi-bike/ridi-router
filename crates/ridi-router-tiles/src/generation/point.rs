use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display};

#[derive(Clone, Serialize, Deserialize)]
pub struct GenerationPoint {
    pub id: u64,
    pub lat: f32,
    pub lon: f32,
    pub residential_in_proximity: bool,
    pub nogo_area: bool,
}

impl PartialEq for GenerationPoint {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Debug for GenerationPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GenerationPoint")
            .field("id", &self.id)
            .field("lat", &self.lat)
            .field("lon", &self.lon)
            .field(
                "residential_in_proximity",
                &self.residential_in_proximity,
            )
            .field("nogo_area", &self.nogo_area)
            .finish()
    }
}

impl Display for GenerationPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Point({}: {}, {})", self.id, self.lat, self.lon)
    }
}
