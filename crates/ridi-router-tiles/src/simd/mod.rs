pub mod haversine;
pub mod trig_approx;

#[cfg(target_feature = "avx512f")]
pub use haversine::haversine_f32x16;
#[cfg(target_feature = "avx512f")]
pub use trig_approx::{asin_f32x16, cos_f32x16, sin_f32x16};
