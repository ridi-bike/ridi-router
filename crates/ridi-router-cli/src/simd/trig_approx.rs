//! SIMD polynomial approximations for trigonometric functions.
//!
//! Since the `wide` crate doesn't provide transcendental functions (sin, cos, asin),
//! we implement them using Taylor series polynomial approximations.
//!
//! These approximations are optimized for the ranges used in haversine calculations:
//! - sin/cos: Input angles typically in range [-π, π]
//! - asin: Input typically in range [-1, 1], but haversine uses sqrt(a) where a ∈ [0, 1]

use wide::{f32x8, CmpLt};

// Constants for Taylor series coefficients
// sin(x) ≈ x - x³/6 + x⁵/120 - x⁷/5040 + x⁹/362880 - x¹¹/39916800
const SIN_COEFF_3: f32 = 1.0 / 6.0;
const SIN_COEFF_5: f32 = 1.0 / 120.0;
const SIN_COEFF_7: f32 = 1.0 / 5040.0;
const SIN_COEFF_9: f32 = 1.0 / 362880.0;
const SIN_COEFF_11: f32 = 1.0 / 39916800.0;
// cos(x) ≈ 1 - x²/2 + x⁴/24 - x⁶/720 + x⁸/40320 - x¹⁰/3628800
const COS_COEFF_2: f32 = 1.0 / 2.0;
const COS_COEFF_4: f32 = 1.0 / 24.0;
const COS_COEFF_6: f32 = 1.0 / 720.0;
const COS_COEFF_8: f32 = 1.0 / 40320.0;
const COS_COEFF_10: f32 = 1.0 / 3628800.0;
// asin(x) ≈ x + x³/6 + 3x⁵/40 + 15x⁷/336 + 105x⁹/3456 for |x| < 0.5
// For |x| >= 0.5, we use: asin(x) = π/2 - 2*asin(sqrt((1-x)/2))
const ASIN_COEFF_3: f32 = 1.0 / 6.0;
const ASIN_COEFF_5: f32 = 3.0 / 40.0;
const ASIN_COEFF_7: f32 = 15.0 / 336.0;
const ASIN_COEFF_9: f32 = 105.0 / 3456.0;

const PI: f32 = std::f32::consts::PI;
const FRAC_PI_2: f32 = std::f32::consts::FRAC_PI_2;

/// Normalize angle to [-π, π] range for better Taylor series accuracy
#[inline]
fn normalize_angle_f32x8(x: f32x8) -> f32x8 {
    // Use modulo reduction: x - 2π * round(x / 2π)
    let two_pi = f32x8::splat(2.0 * PI);
    let inv_two_pi = f32x8::splat(1.0 / (2.0 * PI));

    // round(x / 2π)
    let n = (x * inv_two_pi).round();

    // x - 2π * n
    x - two_pi * n
}

/// sin(x) using Taylor series - accurate to <0.01% for |x| < π
///
/// Uses 5-term Taylor series: sin(x) ≈ x - x³/6 + x⁵/120 - x⁷/5040 + x⁹/362880
///
/// Input angles are normalized to [-π, π] for best accuracy.
#[inline]
pub fn sin_f32x8(x: f32x8) -> f32x8 {
    // Normalize to [-π, π]
    let x = normalize_angle_f32x8(x);

    let x2 = x * x;
    let x3 = x2 * x;
    let x5 = x3 * x2;
    let x7 = x5 * x2;
    let x9 = x7 * x2;
    let x11 = x9 * x2;

    let result = x - x3 * f32x8::splat(SIN_COEFF_3) + x5 * f32x8::splat(SIN_COEFF_5)
        - x7 * f32x8::splat(SIN_COEFF_7)
        + x9 * f32x8::splat(SIN_COEFF_9)
        - x11 * f32x8::splat(SIN_COEFF_11);

    // Clamp to [-1, 1] to handle approximation errors at boundaries
    let one = f32x8::splat(1.0);
    let neg_one = f32x8::splat(-1.0);
    result.fast_max(neg_one).fast_min(one)
}
/// cos(x) using Taylor series - accurate to <0.001% for |x| < π
///
/// Uses 6-term Taylor series: cos(x) ≈ 1 - x²/2 + x⁴/24 - x⁶/720 + x⁸/40320 - x¹⁰/3628800
///
/// Input angles are normalized to [-π, π] for best accuracy.
#[inline]
pub fn cos_f32x8(x: f32x8) -> f32x8 {
    // Normalize to [-π, π]
    let x = normalize_angle_f32x8(x);

    let x2 = x * x;
    let x4 = x2 * x2;
    let x6 = x4 * x2;
    let x8 = x6 * x2;
    let x10 = x8 * x2;

    let result = f32x8::splat(1.0) - x2 * f32x8::splat(COS_COEFF_2)
        + x4 * f32x8::splat(COS_COEFF_4)
        - x6 * f32x8::splat(COS_COEFF_6)
        + x8 * f32x8::splat(COS_COEFF_8)
        - x10 * f32x8::splat(COS_COEFF_10);

    // Clamp to [-1, 1] to handle approximation errors at boundaries
    let one = f32x8::splat(1.0);
    let neg_one = f32x8::splat(-1.0);
    result.fast_max(neg_one).fast_min(one)
}
/// asin(x) approximation using Taylor series with range reduction
///
/// For |x| < 0.5: uses direct Taylor series
/// For |x| >= 0.5: uses identity asin(x) = π/2 - 2*asin(sqrt((1-x)/2))
///
/// This provides good accuracy across the full [-1, 1] range needed for haversine.
#[inline]
pub fn asin_f32x8(x: f32x8) -> f32x8 {
    let abs_x = x.abs();
    let threshold = f32x8::splat(0.5);
    let one = f32x8::splat(1.0);
    let two = f32x8::splat(2.0);
    let frac_pi_2 = f32x8::splat(FRAC_PI_2);

    // Mask for |x| < 0.5
    let small_mask = abs_x.cmp_lt(threshold);

    // For small x: use direct Taylor series
    let x2 = x * x;
    let x3 = x2 * x;
    let x5 = x3 * x2;
    let x7 = x5 * x2;
    let x9 = x7 * x2;

    let small_result = x
        + x3 * f32x8::splat(ASIN_COEFF_3)
        + x5 * f32x8::splat(ASIN_COEFF_5)
        + x7 * f32x8::splat(ASIN_COEFF_7)
        + x9 * f32x8::splat(ASIN_COEFF_9);

    // For large |x|: use identity asin(x) = sign(x) * (π/2 - 2*asin(sqrt((1-|x|)/2)))
    // Compute y = sqrt((1 - |x|) / 2)
    let y_squared = (one - abs_x) / two;
    let y = y_squared.sqrt();

    // Taylor series for asin(y) where y is small
    let y2 = y * y;
    let y3 = y2 * y;
    let y5 = y3 * y2;
    let y7 = y5 * y2;
    let y9 = y7 * y2;

    let asin_y = y
        + y3 * f32x8::splat(ASIN_COEFF_3)
        + y5 * f32x8::splat(ASIN_COEFF_5)
        + y7 * f32x8::splat(ASIN_COEFF_7)
        + y9 * f32x8::splat(ASIN_COEFF_9);

    let large_result_abs = frac_pi_2 - two * asin_y;

    // Apply sign
    let sign_mask = x.cmp_lt(f32x8::splat(0.0));
    let large_result = sign_mask.blend(-large_result_abs, large_result_abs);

    // Blend results based on |x| < 0.5
    small_mask.blend(small_result, large_result)
}

// AVX-512 versions (f32x16) - only compiled when target supports it
#[cfg(target_feature = "avx512f")]
mod avx512 {
    use wide::f32x16;

    use super::*;

    #[inline]
    fn normalize_angle_f32x16(x: f32x16) -> f32x16 {
        let two_pi = f32x16::splat(2.0 * PI);
        let inv_two_pi = f32x16::splat(1.0 / (2.0 * PI));
        let n = (x * inv_two_pi).round();
        x - two_pi * n
    }

    #[inline]
    pub fn sin_f32x16(x: f32x16) -> f32x16 {
        let x = normalize_angle_f32x16(x);
        let x2 = x * x;
        let x3 = x2 * x;
        let x5 = x3 * x2;
        let x7 = x5 * x2;
        let x9 = x7 * x2;

        x - x3 * f32x16::splat(SIN_COEFF_3) + x5 * f32x16::splat(SIN_COEFF_5)
            - x7 * f32x16::splat(SIN_COEFF_7)
            + x9 * f32x16::splat(SIN_COEFF_9)
    }

    #[inline]
    pub fn cos_f32x16(x: f32x16) -> f32x16 {
        let x = normalize_angle_f32x16(x);
        let x2 = x * x;
        let x4 = x2 * x2;
        let x6 = x4 * x2;
        let x8 = x6 * x2;

        f32x16::splat(1.0) - x2 * f32x16::splat(COS_COEFF_2) + x4 * f32x16::splat(COS_COEFF_4)
            - x6 * f32x16::splat(COS_COEFF_6)
            + x8 * f32x16::splat(COS_COEFF_8)
    }

    #[inline]
    pub fn asin_f32x16(x: f32x16) -> f32x16 {
        let abs_x = x.abs();
        let threshold = f32x16::splat(0.5);
        let one = f32x16::splat(1.0);
        let two = f32x16::splat(2.0);
        let frac_pi_2 = f32x16::splat(FRAC_PI_2);

        let small_mask = abs_x.cmp_lt(threshold);

        let x2 = x * x;
        let x3 = x2 * x;
        let x5 = x3 * x2;
        let x7 = x5 * x2;
        let x9 = x7 * x2;

        let small_result = x
            + x3 * f32x16::splat(ASIN_COEFF_3)
            + x5 * f32x16::splat(ASIN_COEFF_5)
            + x7 * f32x16::splat(ASIN_COEFF_7)
            + x9 * f32x16::splat(ASIN_COEFF_9);

        let y_squared = (one - abs_x) / two;
        let y = y_squared.sqrt();

        let y2 = y * y;
        let y3 = y2 * y;
        let y5 = y3 * y2;
        let y7 = y5 * y2;
        let y9 = y7 * y2;

        let asin_y = y
            + y3 * f32x16::splat(ASIN_COEFF_3)
            + y5 * f32x16::splat(ASIN_COEFF_5)
            + y7 * f32x16::splat(ASIN_COEFF_7)
            + y9 * f32x16::splat(ASIN_COEFF_9);

        let large_result_abs = frac_pi_2 - two * asin_y;
        let sign_mask = x.cmp_lt(f32x16::splat(0.0));
        let large_result = sign_mask.blend(-large_result_abs, large_result_abs);

        small_mask.blend(small_result, large_result)
    }
}

#[cfg(target_feature = "avx512f")]
pub use avx512::{asin_f32x16, cos_f32x16, sin_f32x16};

#[cfg(test)]
mod tests {
    use super::*;
    use num_traits::Float;

    /// Maximum allowed relative error for trig functions (0.5% for SIMD approx)
    const MAX_RELATIVE_ERROR: f32 = 0.005; // 0.5%
    fn relative_error(actual: f32, expected: f32) -> f32 {
        if expected.abs() < 1e-10 {
            actual.abs()
        } else {
            ((actual - expected) / expected).abs()
        }
    }

    #[test]
    fn test_sin_f32x8_basic_values() {
        let inputs = f32x8::from([
            0.0,
            PI / 6.0,
            PI / 4.0,
            PI / 3.0,
            PI / 2.0,
            PI,
            -PI / 2.0,
            0.1,
        ]);
        let results: [f32; 8] = sin_f32x8(inputs).into();

        let expected = [
            0.0f32.sin(),
            (PI / 6.0).sin(),
            (PI / 4.0).sin(),
            (PI / 3.0).sin(),
            (PI / 2.0).sin(),
            PI.sin(),
            (-PI / 2.0).sin(),
            0.1f32.sin(),
        ];

        for (i, (&result, &exp)) in results.iter().zip(expected.iter()).enumerate() {
            let error = relative_error(result, exp);
            assert!(
                error < MAX_RELATIVE_ERROR || (result - exp).abs() < 5e-4,
                "sin_f32x8 index {}: expected {}, got {}, error {}",
                i,
                exp,
                result,
                error
            );
        }
    }

    #[test]
    fn test_cos_f32x8_basic_values() {
        let inputs = f32x8::from([
            0.0,
            PI / 6.0,
            PI / 4.0,
            PI / 3.0,
            PI / 2.0,
            PI,
            -PI / 2.0,
            0.1,
        ]);
        let results: [f32; 8] = cos_f32x8(inputs).into();

        let expected = [
            0.0f32.cos(),
            (PI / 6.0).cos(),
            (PI / 4.0).cos(),
            (PI / 3.0).cos(),
            (PI / 2.0).cos(),
            PI.cos(),
            (-PI / 2.0).cos(),
            0.1f32.cos(),
        ];

        for (i, (&result, &exp)) in results.iter().zip(expected.iter()).enumerate() {
            let error = relative_error(result, exp);
            assert!(
                error < MAX_RELATIVE_ERROR || (result - exp).abs() < 5e-4,
                "cos_f32x8 index {}: expected {}, got {}, error {}",
                i,
                exp,
                result,
                error
            );
        }
    }

    #[test]
    fn test_asin_f32x8_basic_values() {
        let inputs = f32x8::from([0.0, 0.1, 0.3, 0.5, 0.7, 0.9, 0.99, -0.5]);
        let results: [f32; 8] = asin_f32x8(inputs).into();

        let expected = [
            0.0f32.asin(),
            0.1f32.asin(),
            0.3f32.asin(),
            0.5f32.asin(),
            0.7f32.asin(),
            0.9f32.asin(),
            0.99f32.asin(),
            (-0.5f32).asin(),
        ];

        for (i, (&result, &exp)) in results.iter().zip(expected.iter()).enumerate() {
            let error = relative_error(result, exp);
            assert!(
                error < 0.001 || (result - exp).abs() < 1e-5, // Slightly higher tolerance for asin
                "asin_f32x8 index {}: expected {}, got {}, error {}",
                i,
                exp,
                result,
                error
            );
        }
    }

    #[test]
    fn test_sin_cos_identity() {
        // Test sin²(x) + cos²(x) = 1
        let inputs = f32x8::from([0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, -1.0]);
        let sin_vals = sin_f32x8(inputs);
        let cos_vals = cos_f32x8(inputs);

        let sum: [f32; 8] = (sin_vals * sin_vals + cos_vals * cos_vals).into();

        for (i, &s) in sum.iter().enumerate() {
            assert!(
                (s - 1.0).abs() < 0.003,
                "sin²(x) + cos²(x) should be 1, got {} at index {}",
                s,
                i
            );
        }
    }

    #[test]
    fn test_normalized_angles() {
        // Test that large angles are handled correctly
        let large_angles = f32x8::from([
            10.0 * PI,
            -10.0 * PI,
            100.0,
            -100.0,
            1000.0,
            -1000.0,
            0.0,
            PI,
        ]);

        let sin_results: [f32; 8] = sin_f32x8(large_angles).into();
        let cos_results: [f32; 8] = cos_f32x8(large_angles).into();

        // All results should be in valid range [-1, 1]
        for i in 0..8 {
            assert!(
                sin_results[i] >= -1.0 && sin_results[i] <= 1.0,
                "sin out of range at index {}: {}",
                i,
                sin_results[i]
            );
            assert!(
                cos_results[i] >= -1.0 && cos_results[i] <= 1.0,
                "cos out of range at index {}: {}",
                i,
                cos_results[i]
            );
        }
    }

    #[test]
    fn test_asin_range_boundary() {
        // Test values near the boundary of |x| = 0.5 where we switch methods
        let inputs = f32x8::from([0.49, 0.50, 0.51, 0.499, 0.501, -0.49, -0.50, -0.51]);
        let results: [f32; 8] = asin_f32x8(inputs).into();

        let expected: Vec<f32> = [0.49, 0.50, 0.51, 0.499, 0.501, -0.49, -0.50, -0.51]
            .iter()
            .map(|x| x.asin())
            .collect();

        for (i, (&result, &exp)) in results.iter().zip(expected.iter()).enumerate() {
            assert!(
                (result - exp).abs() < 0.001,
                "asin boundary test index {}: expected {}, got {}",
                i,
                exp,
                result
            );
        }
    }
}
