//! Deterministic fixed-point feature extraction (v0.4.0).
//!
//! These are **integer-only**, allocation-free scalar features computed **per
//! channel** over a contiguous sample buffer (the caller de-interleaves a
//! channel out of a [`crate::frame::RawFrame`] / [`crate::epoch::Epoch`]). Like
//! the rest of the data path they are bit-exact across platforms and pinned by
//! conformance vectors (`docs/PIPELINE_CONTRACT.md` §10); no floating point is
//! used. They are **defined deterministic transforms**, not measured-quality
//! descriptors, and carry no accuracy claim (`docs/CLAIMS.md`).
//!
//! Feature outputs are reduced scalars, not raw signal; nonetheless this crate
//! keeps them in-pipeline and only the terminal
//! [`crate::decision::ClassifierDecision`] is boundary-safe
//! (`docs/PRIVACY_BOUNDARY.md`).

use crate::error::PipelineError;

/// Floor integer square root of `x` (no floating point).
///
/// Returns the largest `r` with `r*r <= x`. Used by [`rms`] and by the
/// fixed-point inverse-square-root in [`crate::calibrate`].
///
/// ```
/// use axonos_pipeline_core::feature::isqrt;
/// assert_eq!(isqrt(16), 4);
/// assert_eq!(isqrt(15), 3);
/// assert_eq!(isqrt(0), 0);
/// ```
pub fn isqrt(x: u64) -> u64 {
    if x == 0 {
        return 0;
    }
    let mut bit: u64 = 1 << 62;
    while bit > x {
        bit >>= 2;
    }
    let mut res: u64 = 0;
    let mut n = x;
    while bit != 0 {
        if n >= res + bit {
            n -= res + bit;
            res = (res >> 1) + bit;
        } else {
            res >>= 1;
        }
        bit >>= 2;
    }
    res
}

/// Base-2 logarithm of `x` in `Q16` fixed point (16 fractional bits).
///
/// `log2_q16(x) ≈ round(log2(x) * 65536)` for `x >= 1`. By convention
/// `log2_q16(0) = 0` (the value is clamped, never a panic). The integer part is
/// taken from the most-significant bit; 16 fractional bits are extracted by the
/// classic square-and-compare iteration in `u128`, so it never overflows.
///
/// ```
/// use axonos_pipeline_core::feature::log2_q16;
/// assert_eq!(log2_q16(1), 0);
/// assert_eq!(log2_q16(2), 1 << 16);
/// assert_eq!(log2_q16(256), 8 << 16);
/// ```
pub fn log2_q16(x: u64) -> i32 {
    if x == 0 {
        return 0;
    }
    let int_part = 63 - x.leading_zeros() as i32;
    // Normalise the mantissa to Q32 in [1, 2): value in [2^32, 2^33).
    let mut m: u128 = if int_part <= 32 {
        (x as u128) << (32 - int_part)
    } else {
        (x as u128) >> (int_part - 32)
    };
    let mut result: i32 = int_part << 16;
    let mut bit: i32 = 1 << 15;
    while bit > 0 {
        m = (m * m) >> 32; // square, stay in Q32; now in [2^32, 2^34)
        if m >= (1u128 << 33) {
            result += bit;
            m >>= 1; // back to [1, 2)
        }
        bit >>= 1;
    }
    result
}

fn mean_i64(samples: &[i32]) -> i64 {
    let mut sum: i64 = 0;
    for &x in samples {
        sum += x as i64;
    }
    sum / samples.len() as i64 // len > 0 enforced by callers
}

/// Population variance of `samples` (mean removed), as an unsigned integer.
///
/// `variance = Σ(x − mean)² / N` with `mean` truncated toward zero and the
/// accumulation in `u128` (never overflows). For ADC-count inputs the result
/// fits `u64`. **Not** a measured quantity — a defined transform.
///
/// # Errors
///
/// [`PipelineError::EmptyInput`] if `samples` is empty.
pub fn variance(samples: &[i32]) -> Result<u64, PipelineError> {
    if samples.is_empty() {
        return Err(PipelineError::EmptyInput);
    }
    let mean = mean_i64(samples);
    let mut acc: u128 = 0;
    for &x in samples {
        let d = x as i64 - mean;
        acc += (d as i128 * d as i128) as u128;
    }
    Ok((acc / samples.len() as u128) as u64)
}

/// `Q16` base-2 log of the [`variance`] — the canonical log-variance feature.
///
/// # Errors
///
/// [`PipelineError::EmptyInput`] if `samples` is empty.
pub fn log_variance_q16(samples: &[i32]) -> Result<i32, PipelineError> {
    Ok(log2_q16(variance(samples)?))
}

/// Root-mean-square of the mean-removed signal (i.e. the standard deviation),
/// `isqrt(variance)`.
///
/// # Errors
///
/// [`PipelineError::EmptyInput`] if `samples` is empty.
pub fn rms(samples: &[i32]) -> Result<u32, PipelineError> {
    Ok(isqrt(variance(samples)?) as u32)
}

/// Mean absolute amplitude, `Σ|x| / N`.
///
/// # Errors
///
/// [`PipelineError::EmptyInput`] if `samples` is empty.
pub fn abs_mean(samples: &[i32]) -> Result<u32, PipelineError> {
    if samples.is_empty() {
        return Err(PipelineError::EmptyInput);
    }
    let mut sum: u64 = 0;
    for &x in samples {
        sum += (x as i64).unsigned_abs();
    }
    Ok((sum / samples.len() as u64) as u32)
}

/// Number of strict sign changes between consecutive samples (zeros do not
/// count as crossings).
///
/// ```
/// use axonos_pipeline_core::feature::zero_crossings;
/// assert_eq!(zero_crossings(&[1, -1, 1, -1]), 3);
/// assert_eq!(zero_crossings(&[5, 0, 5]), 0);
/// ```
pub fn zero_crossings(samples: &[i32]) -> u32 {
    let mut count: u32 = 0;
    for w in samples.windows(2) {
        let a = w[0] as i64;
        let b = w[1] as i64;
        if (a < 0 && b > 0) || (a > 0 && b < 0) {
            count += 1;
        }
    }
    count
}

/// Dense feature vector of compile-time dimension `D` (legacy `f32` interop
/// container).
///
/// `f32` is **not** part of any deterministic conformance claim; the
/// vector-pinned, bit-exact features are the integer functions in this module
/// (`variance`, `log_variance_q16`, `rms`, `abs_mean`, `zero_crossings`). This
/// type remains for callers that bridge to floating-point analysis tools off
/// the deterministic path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeatureVector<const D: usize> {
    values: [f32; D],
}

impl<const D: usize> FeatureVector<D> {
    /// Wraps an array of features.
    pub const fn new(values: [f32; D]) -> Self {
        Self { values }
    }

    /// Feature dimension.
    pub const fn len(&self) -> usize {
        D
    }

    /// True iff the dimension is zero.
    pub const fn is_empty(&self) -> bool {
        D == 0
    }

    /// Borrows the features as a slice.
    pub fn as_slice(&self) -> &[f32] {
        &self.values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isqrt_matches_floor() {
        for n in [0u64, 1, 2, 3, 4, 8, 15, 16, 17, 1_000_000, u32::MAX as u64] {
            let r = isqrt(n);
            assert!(r * r <= n, "isqrt({n})={r} too big");
            assert!((r + 1) * (r + 1) > n || (r + 1).checked_mul(r + 1).is_none());
        }
    }

    #[test]
    fn log2_q16_on_powers_and_between() {
        assert_eq!(log2_q16(1), 0);
        assert_eq!(log2_q16(2), 1 << 16);
        assert_eq!(log2_q16(1024), 10 << 16);
        // log2(3) ≈ 1.585 → ~103872 in Q16; allow small fixed-point error.
        let l3 = log2_q16(3);
        assert!((l3 - 103872).abs() < 64, "log2(3) Q16 = {l3}");
    }

    #[test]
    fn variance_of_known_sequence() {
        // [-2,-1,0,1,2] mean 0, Σx²=10, var=2.
        assert_eq!(variance(&[-2, -1, 0, 1, 2]).unwrap(), 2);
        // constant → variance 0.
        assert_eq!(variance(&[7, 7, 7]).unwrap(), 0);
        assert_eq!(variance(&[]), Err(PipelineError::EmptyInput));
    }

    #[test]
    fn rms_absmean_zerocross() {
        assert_eq!(rms(&[-3, 3, -3, 3]).unwrap(), 3); // |dev| = 3
        assert_eq!(abs_mean(&[-4, 4, -4, 4]).unwrap(), 4);
        assert_eq!(zero_crossings(&[1, -1, 1, -1]), 3);
        assert_eq!(zero_crossings(&[2, 3, 4]), 0);
    }
}
