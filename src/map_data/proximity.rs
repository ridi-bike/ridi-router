#[cfg(feature = "debug-with-postgres")]
use std::hash::Hash;

#[cfg(feature = "debug-with-postgres")]
use geo::{Coord, Point};

#[cfg(feature = "debug-with-postgres")]
use wkt::ToWkt;

#[cfg(feature = "debug-with-postgres")]
/// two decimal places 1.1km precision
pub const GRID_CALC_DECIMAL_PLACES: usize = 2;
#[cfg(feature = "debug-with-postgres")]
pub const GRID_CALC_PRECISION: i16 = 10u32.pow(GRID_CALC_DECIMAL_PLACES as u32) as i16;

#[cfg(feature = "debug-with-postgres")]
pub enum RoundMethod {
    Ceil,
    Floor,
    Round,
}

#[cfg(feature = "debug-with-postgres")]
#[derive(Debug)]
pub struct AdjustedCoord(Coord);

#[cfg(feature = "debug-with-postgres")]
impl ToWkt<f64> for AdjustedCoord {
    fn to_wkt(&self) -> wkt::Wkt<f64> {
        Point::new(self.0.x, self.0.y).to_wkt()
    }
}

#[cfg(feature = "debug-with-postgres")]
impl Hash for AdjustedCoord {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let x_scaled = (self.0.x * GRID_CALC_PRECISION as f64).round() as i64;
        let y_scaled = (self.0.y * GRID_CALC_PRECISION as f64).round() as i64;
        x_scaled.hash(state);
        y_scaled.hash(state);
    }
}

#[cfg(feature = "debug-with-postgres")]
impl Eq for AdjustedCoord {}

#[cfg(feature = "debug-with-postgres")]
impl PartialEq for AdjustedCoord {
    fn eq(&self, other: &Self) -> bool {
        let x_scaled_self = (self.0.x * GRID_CALC_PRECISION as f64).round() as i64;
        let y_scaled_self = (self.0.y * GRID_CALC_PRECISION as f64).round() as i64;
        let x_scaled_other = (other.0.x * GRID_CALC_PRECISION as f64).round() as i64;
        let y_scaled_other = (other.0.y * GRID_CALC_PRECISION as f64).round() as i64;
        x_scaled_self == x_scaled_other && y_scaled_self == y_scaled_other
    }
}

#[cfg(feature = "debug-with-postgres")]
pub fn round_to_precision(v: f64, direction: RoundMethod) -> f64 {
    match direction {
        RoundMethod::Ceil => (v * GRID_CALC_PRECISION as f64).ceil() / GRID_CALC_PRECISION as f64,
        RoundMethod::Floor => (v * GRID_CALC_PRECISION as f64).floor() / GRID_CALC_PRECISION as f64,
        RoundMethod::Round => (v * GRID_CALC_PRECISION as f64).round() / GRID_CALC_PRECISION as f64,
    }
}
