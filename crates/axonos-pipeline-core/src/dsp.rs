//! Deterministic integer DSP primitives (v0.2.0).
//!
//! These stages are **fixed-point / integer-only** by design. The crate's
//! contract is bit-exact reproducibility across platforms and against the
//! language-neutral conformance vectors (`docs/CLAIMS.md`); floating point
//! would not reproduce bit-for-bit between a generator oracle and every
//! target, so the deterministic data path stays in integer arithmetic. The
//! fixed-point feature path now lives in [`crate::feature`]; the `f32`
//! [`crate::feature::FeatureVector`] remains a legacy interop container outside
//! any conformance claim (see `docs/VALIDATION_PLAN.md`).
//!
//! All functions are allocation-free and operate **per channel** on a
//! contiguous sample buffer: the caller supplies the input samples and an
//! output buffer of equal length. De-interleaving a channel out of a
//! [`crate::frame::RawFrame`] is the acquisition layer's responsibility.
//!
//! Outputs of these stages are ordinary in-pipeline sample buffers; like
//! [`crate::frame::RawFrame`] they are **not** boundary-safe and must not
//! cross the application boundary (`docs/PRIVACY_BOUNDARY.md`).
//!
//! Arithmetic is defined normatively in `docs/PIPELINE_CONTRACT.md` §9.

use crate::error::PipelineError;

/// Largest fixed-point right-shift permitted by [`fir`].
pub const MAX_FIR_SHIFT: u32 = 31;

/// Saturating narrow of an `i64` into the `i32` sample range.
#[inline]
fn saturate_i32(value: i64) -> i32 {
    if value > i32::MAX as i64 {
        i32::MAX
    } else if value < i32::MIN as i64 {
        i32::MIN
    } else {
        value as i32
    }
}

/// Removes the integer mean (DC offset) of `input`, writing the centred
/// samples to `out` and returning the mean that was removed.
///
/// The mean is `sum(input) / input.len()` with the division **truncated
/// toward zero** (`docs/PIPELINE_CONTRACT.md` §9.1). The subtraction is
/// performed in `i64` and saturated back into the `i32` sample range, so it
/// never overflows or panics.
///
/// # Errors
///
/// - [`PipelineError::EmptyInput`] if `input` is empty.
/// - [`PipelineError::OutputLengthMismatch`] if `out.len() != input.len()`.
///
/// ```
/// use axonos_pipeline_core::remove_mean;
/// let input = [10, 20, 30, 40];
/// let mut out = [0i32; 4];
/// let mean = remove_mean(&input, &mut out).unwrap();
/// assert_eq!(mean, 25);
/// assert_eq!(out, [-15, -5, 5, 15]);
/// ```
pub fn remove_mean(input: &[i32], out: &mut [i32]) -> Result<i32, PipelineError> {
    if input.is_empty() {
        return Err(PipelineError::EmptyInput);
    }
    if out.len() != input.len() {
        return Err(PipelineError::OutputLengthMismatch);
    }
    let mut sum: i64 = 0;
    for &x in input {
        sum += x as i64;
    }
    // input.len() > 0, so division is well-defined; `/` truncates toward zero.
    let mean: i64 = sum / input.len() as i64;
    for (dst, &x) in out.iter_mut().zip(input) {
        *dst = saturate_i32(x as i64 - mean);
    }
    Ok(saturate_i32(mean))
}

/// Applies a causal finite-impulse-response filter to `input`, writing the
/// result to `out`.
///
/// For output index `n`, `y[n] = (Σ coeffs[k] * input[n - k]) >> shift`,
/// where samples before the start of the buffer are treated as zero (zero
/// initial state). The accumulator is `i64`; the right shift is arithmetic
/// and applies round-half-up via a `1 << (shift - 1)` bias when `shift >= 1`
/// (`docs/PIPELINE_CONTRACT.md` §9.2). Each output is saturated into the
/// `i32` sample range.
///
/// The caller owns coefficient design; `coeffs` are raw fixed-point taps and
/// `shift` is the fractional scaling. This function makes **no** claim about
/// frequency response — it is the deterministic convolution engine, not a
/// validated band-pass or notch design (`docs/CLAIMS.md`).
///
/// # Errors
///
/// - [`PipelineError::EmptyInput`] if `input` is empty.
/// - [`PipelineError::EmptyKernel`] if `coeffs` is empty.
/// - [`PipelineError::OutputLengthMismatch`] if `out.len() != input.len()`.
/// - [`PipelineError::InvalidShift`] if `shift > MAX_FIR_SHIFT`.
///
/// To avoid `i64` accumulator overflow, keep
/// `coeffs.len() * max|coeffs| * max|input|` below `2^63`.
///
/// ```
/// use axonos_pipeline_core::fir;
/// // 4-tap moving average: sum of the last four samples divided by 4.
/// let input = [4, 8, 12, 16];
/// let mut out = [0i32; 4];
/// fir(&input, &[1, 1, 1, 1], 2, &mut out).unwrap();
/// assert_eq!(out, [1, 3, 6, 10]);
/// ```
pub fn fir(
    input: &[i32],
    coeffs: &[i32],
    shift: u32,
    out: &mut [i32],
) -> Result<(), PipelineError> {
    if input.is_empty() {
        return Err(PipelineError::EmptyInput);
    }
    if coeffs.is_empty() {
        return Err(PipelineError::EmptyKernel);
    }
    if out.len() != input.len() {
        return Err(PipelineError::OutputLengthMismatch);
    }
    if shift > MAX_FIR_SHIFT {
        return Err(PipelineError::InvalidShift);
    }
    let bias: i64 = if shift >= 1 { 1i64 << (shift - 1) } else { 0 };
    for n in 0..input.len() {
        let mut acc: i64 = 0;
        for (k, &c) in coeffs.iter().enumerate() {
            if n >= k {
                acc += c as i64 * input[n - k] as i64;
            }
        }
        let y = if shift >= 1 {
            (acc + bias) >> shift
        } else {
            acc
        };
        out[n] = saturate_i32(y);
    }
    Ok(())
}
