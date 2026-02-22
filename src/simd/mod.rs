//! SIMD-accelerated mathematical functions for geographic calculations.
//!
//! This module provides SIMD-optimized implementations of trigonometric functions
//! and haversine distance calculations using the `wide` crate.
//!
//! The implementations use polynomial approximations (Taylor series) since the
//! `wide` crate doesn't provide transcendental functions directly.

pub mod haversine;
pub mod trig_approx;


#[cfg(target_feature = "avx512f")]
pub use haversine::haversine_f32x16;
#[cfg(target_feature = "avx512f")]
pub use trig_approx::{asin_f32x16, cos_f32x16, sin_f32x16};
