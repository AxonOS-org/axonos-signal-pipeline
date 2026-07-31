// SPDX-License-Identifier: Apache-2.0 OR MIT
// SPDX-FileCopyrightText: 2026 Denis Yermakou <connect@axonos.org>
//
// Part of the AxonOS Signal Pipeline. Dual-licensed Apache-2.0 OR MIT at your
// option; see LICENSE-APACHE and LICENSE-MIT. Authored by Denis Yermakou for The
// AxonOS Project — https://axonos.org

//! Narrowband power at declared frequencies — v0.9.0.
//!
//! Two of the three paradigms this pipeline exists to serve are frequency
//! questions. SSVEP asks which of a handful of flicker rates a subject is
//! attending to; a band-power index asks how much power sits in one rhythm
//! relative to another. Neither needs a spectrum. Both need a small number of
//! specific bins, and computing four bins costs a fraction of computing all of
//! them.
//!
//! So this module is Goertzel's algorithm rather than an FFT — one second-order
//! recurrence per frequency, `O(N)` per bin, no transform buffer, no twiddle
//! table, and no power-of-two constraint on the window. On the reference DSP a
//! four-target SSVEP decision costs four recurrences over a 1024-sample window
//! instead of a 1024-point transform whose 1020 unused bins are discarded.
//!
//! ## Why this is integer, like everything else here
//!
//! The recurrence coefficient is `2·cos(2πk/N)`, and it is the only
//! transcendental quantity in the pipeline. It is supplied by the caller as a
//! `Q14` constant — computed once, off the real-time path, by whatever tool the
//! caller trusts — rather than computed here from a cosine this crate would have
//! to implement and pin. A cosine implemented in fixed point is a second
//! conformance surface, and this crate already has enough of those.
//!
//! [`goertzel_coeff_q14`] does the conversion for callers who have the frequency
//! and want the constant, and it is deliberately **not** on the sample path: it
//! is a `const`-friendly integer approximation used at configuration time, and a
//! caller who needs more accuracy than it gives should compute the coefficient
//! elsewhere and pass it in.
//!
//! ## What is claimed
//!
//! Determinism, bounded arithmetic, and agreement with the closed form on
//! synthetic tones to the tolerance the tests state. **No** claim that a power
//! ratio classifies attention, detects SSVEP, or means anything about a person —
//! see `docs/CLAIMS.md`. This computes a number; what the number is evidence of
//! is somebody else's argument.

use crate::error::PipelineError;

/// Fractional bits in a Goertzel coefficient.
///
/// `Q14` rather than `Q16`: the coefficient lives in `[-2, 2]`, so two integer
/// bits are needed and fourteen fractional bits are what remains inside an
/// `i16`-sized value while keeping the recurrence's products inside `i64`
/// without an intermediate shift.
pub const COEFF_SHIFT: u32 = 14;

/// One unit of a `Q14` coefficient.
pub const COEFF_ONE: i32 = 1 << COEFF_SHIFT;

/// Largest permitted coefficient magnitude: `2.0` in `Q14`.
pub const COEFF_MAX: i32 = 2 * COEFF_ONE;

/// Goertzel coefficient `2·cos(2πf/f_s)` in `Q14`, for configuration time.
///
/// Integer cosine by a bounded polynomial on the first quadrant with symmetry
/// reduction — accurate to about one part in 8 000 over the whole circle, which
/// is far finer than the bin spacing any realistic window gives. Deterministic
/// and `no_std`, but **not** on the sample path: compute it once when the
/// paradigm is configured, not per epoch.
///
/// Returns `None` when the frequency is not below Nyquist. A bin above Nyquist
/// is an alias of a lower one, and returning a coefficient for it would produce
/// a number that looks like an answer.
pub fn goertzel_coeff_q14(freq_milli_hz: u32, sample_rate_hz: u32) -> Option<i32> {
    if sample_rate_hz == 0 || freq_milli_hz == 0 {
        return None;
    }
    // f < f_s / 2, compared in millihertz to avoid a division.
    if (freq_milli_hz as u64) * 2 >= (sample_rate_hz as u64) * 1_000 {
        return None;
    }
    // theta = 2*pi*f/f_s, held in Q16 turns: one turn is 65536.
    let turns_q16 = ((freq_milli_hz as u64) << 16) / (sample_rate_hz as u64 * 1_000);
    let cos_q14 = cos_turns_q14(turns_q16 as u32);
    Some((2 * cos_q14).clamp(-COEFF_MAX, COEFF_MAX))
}

/// Cosine of an angle given in `Q16` turns, returned in `Q14`.
///
/// Quadrant reduction plus a quartic minimax polynomial on `[0, 1/4]` turn.
/// Chosen over a table because a table is data that has to be pinned, reviewed
/// and shipped, and this is nine multiplications.
fn cos_turns_q14(turns_q16: u32) -> i32 {
    const QUARTER: u32 = 1 << 14; // 0.25 turn in Q16
    let t = turns_q16 & 0xFFFF; // wrap to one turn
    let (quadrant, phase) = (t / QUARTER, t % QUARTER);
    // x in Q16 over [0, 1) of a quarter turn
    let x = (phase as u64) << 2; // scale quarter -> full Q16
    let c = cos_quarter_q14(x as u32);
    match quadrant {
        0 => c,
        1 => -cos_quarter_q14(((1u64 << 16) - x) as u32),
        2 => -c,
        _ => cos_quarter_q14(((1u64 << 16) - x) as u32),
    }
}

/// `cos(x · π/2)` for `x` in `Q16` over `[0, 1]`, returned in `Q14`.
fn cos_quarter_q14(x_q16: u32) -> i32 {
    // cos(pi/2 · x) ≈ 1 − 1.2337 x² + 0.2537 x⁴ − 0.0208 x⁶ on [0,1].
    // Coefficients in Q16, Horner in i64, result narrowed to Q14 once.
    let x = x_q16 as i64;
    let x2 = (x * x) >> 16;
    let x4 = (x2 * x2) >> 16;
    let x6 = (x4 * x2) >> 16;
    let one = 1i64 << 16;
    let v = one - ((80_849 * x2) >> 16) + ((16_625 * x4) >> 16) - ((1_363 * x6) >> 16);
    ((v.clamp(-one, one)) >> 2) as i32
}

/// Narrowband power at one frequency, computed by Goertzel's recurrence.
///
/// `coeff_q14` is `2·cos(2πf/f_s)`; get it from [`goertzel_coeff_q14`] or from
/// a tool you trust. Returns the squared magnitude, scaled down by
/// `2^scale_shift` so a long window cannot overflow — the caller chooses the
/// scale because only the caller knows the amplitude range of what it is
/// feeding in.
///
/// The returned number is a *relative* power. Comparing two frequencies computed
/// with the same window and the same shift is meaningful; comparing across
/// different windows is not, and this crate does not pretend the number carries
/// its own units.
pub fn goertzel_power(
    samples: &[i32],
    coeff_q14: i32,
    scale_shift: u32,
) -> Result<u64, PipelineError> {
    if samples.is_empty() {
        return Err(PipelineError::EmptyInput);
    }
    if !(-COEFF_MAX..=COEFF_MAX).contains(&coeff_q14) {
        return Err(PipelineError::InvalidCoefficient);
    }
    if scale_shift > 31 {
        return Err(PipelineError::InvalidShift);
    }
    let mut s1: i64 = 0;
    let mut s2: i64 = 0;
    for &x in samples {
        // s = x + coeff·s1 − s2, with the coefficient's Q14 removed once.
        let s = (x as i64) + ((coeff_q14 as i64 * s1) >> COEFF_SHIFT) - s2;
        s2 = s1;
        s1 = s;
        // Bound the state rather than letting a resonant input walk to overflow:
        // a saturated bin reports "very large", which is true, where a wrapped
        // one reports a plausible small number, which is not.
        const LIMIT: i64 = 1 << 40;
        if s1 > LIMIT {
            s1 = LIMIT;
        } else if s1 < -LIMIT {
            s1 = -LIMIT;
        }
    }
    let a = s1 >> scale_shift;
    let b = s2 >> scale_shift;
    let c = (coeff_q14 as i64 * a) >> COEFF_SHIFT;
    // |X|² = s1² + s2² − coeff·s1·s2
    let power = a * a + b * b - c * b;
    Ok(power.max(0) as u64)
}

/// Power at several frequencies over one block, in one pass per frequency.
///
/// `out` must have one slot per coefficient; a mismatch is refused rather than
/// truncated. This is the shape an SSVEP decision actually needs: four flicker
/// rates, four numbers, and the argmax is the caller's to take — because
/// choosing a target is a decision, and decisions belong above this crate.
pub fn goertzel_bank(
    samples: &[i32],
    coeffs_q14: &[i32],
    scale_shift: u32,
    out: &mut [u64],
) -> Result<(), PipelineError> {
    if coeffs_q14.is_empty() {
        return Err(PipelineError::EmptyKernel);
    }
    if out.len() != coeffs_q14.len() {
        return Err(PipelineError::OutputLengthMismatch);
    }
    for (slot, &c) in out.iter_mut().zip(coeffs_q14.iter()) {
        *slot = goertzel_power(samples, c, scale_shift)?;
    }
    Ok(())
}

/// Ratio of one band's power to another, in parts per thousand.
///
/// The form every band-power index takes — theta over beta, alpha over the rest.
/// Permille rather than a float, and saturating rather than wrapping, so a
/// denominator near zero reports a very large ratio instead of a small one.
///
/// Returns `None` when the denominator is zero: a ratio against no power at all
/// is not a large ratio, it is an absent measurement, and the two must not look
/// alike to a caller.
pub fn power_ratio_permille(numerator: u64, denominator: u64) -> Option<u32> {
    if denominator == 0 {
        return None;
    }
    let scaled = numerator.saturating_mul(1_000) / denominator;
    Some(scaled.min(u32::MAX as u64) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic tone: `amplitude · cos(2π f t / f_s)`, built from the same
    /// integer cosine the coefficient uses, so the test exercises the pipeline's
    /// own arithmetic rather than importing a second definition of a sinusoid.
    fn tone(freq_hz: u32, rate_hz: u32, n: usize, amplitude: i32) -> [i32; 256] {
        let mut out = [0i32; 256];
        for (i, v) in out.iter_mut().enumerate().take(n) {
            let turns = ((freq_hz as u64 * i as u64) << 16) / rate_hz as u64;
            let c = cos_turns_q14((turns & 0xFFFF) as u32);
            *v = ((amplitude as i64 * c as i64) >> COEFF_SHIFT) as i32;
        }
        out
    }

    #[test]
    fn the_integer_cosine_hits_its_landmarks() {
        assert_eq!(cos_turns_q14(0), COEFF_ONE, "cos 0 = 1");
        let quarter = cos_turns_q14(1 << 14);
        assert!(quarter.abs() < 40, "cos(quarter turn) ≈ 0, got {quarter}");
        let half = cos_turns_q14(1 << 15);
        assert!(
            (half + COEFF_ONE).abs() < 40,
            "cos(half turn) ≈ −1, got {half}"
        );
        let three = cos_turns_q14(3 << 14);
        assert!(three.abs() < 40, "cos(three quarters) ≈ 0, got {three}");
    }

    #[test]
    fn the_cosine_is_even_about_zero() {
        for t in [1000u32, 5000, 12000, 30000] {
            let a = cos_turns_q14(t);
            let b = cos_turns_q14(65536 - t);
            assert!((a - b).abs() <= 2, "not even at {t}: {a} vs {b}");
        }
    }

    #[test]
    fn the_coefficient_matches_the_closed_form_to_a_count() {
        // Reference values are 2·cos(2πf/f_s)·2^14 rounded, computed off this
        // crate. The tolerance is three counts out of ~31 700 — tight enough
        // that a regression in the polynomial cannot hide inside it.
        for (milli_hz, rate, expect) in [
            (10_000u32, 250u32, 31_739i32),
            (13_000, 250, 31_035),
            (7_000, 250, 32_262),
            (30_000, 500, 30_467),
            (1_000, 250, 32_758),
        ] {
            let c = goertzel_coeff_q14(milli_hz, rate).unwrap();
            assert!(
                (c - expect).abs() <= 3,
                "{} Hz at {rate}: got {c}, closed form {expect}",
                milli_hz / 1000
            );
        }
    }

    #[test]
    fn the_coefficient_falls_as_the_frequency_rises() {
        // 2·cos is strictly decreasing below Nyquist, and a polynomial that
        // wobbled would show up here before it showed up in a power figure.
        let mut prev = i32::MAX;
        for f in (1_000u32..=120_000).step_by(2_500) {
            let c = goertzel_coeff_q14(f, 250).unwrap();
            assert!(
                c < prev,
                "not monotone at {} Hz: {c} after {prev}",
                f / 1000
            );
            prev = c;
        }
    }

    #[test]
    fn a_frequency_at_or_above_nyquist_is_refused() {
        assert!(
            goertzel_coeff_q14(125_000, 250).is_none(),
            "exactly Nyquist"
        );
        assert!(goertzel_coeff_q14(200_000, 250).is_none(), "above Nyquist");
        assert!(goertzel_coeff_q14(0, 250).is_none());
        assert!(goertzel_coeff_q14(10_000, 0).is_none());
    }

    #[test]
    fn the_bin_at_the_tone_dominates_its_neighbours() {
        // The property that makes this useful: a 10 Hz tone must put far more
        // power in the 10 Hz bin than in 7 or 13.
        let s = tone(10, 250, 250, 100_000);
        let at = goertzel_power(&s[..250], goertzel_coeff_q14(10_000, 250).unwrap(), 8).unwrap();
        for other in [7_000u32, 13_000, 20_000] {
            let off =
                goertzel_power(&s[..250], goertzel_coeff_q14(other, 250).unwrap(), 8).unwrap();
            assert!(
                at > off * 4,
                "10 Hz bin {at} should dominate {} Hz bin {off}",
                other / 1000
            );
        }
    }

    #[test]
    fn a_stronger_tone_gives_a_larger_bin() {
        let quiet = tone(10, 250, 250, 10_000);
        let loud = tone(10, 250, 250, 100_000);
        let c = goertzel_coeff_q14(10_000, 250).unwrap();
        let a = goertzel_power(&quiet[..250], c, 8).unwrap();
        let b = goertzel_power(&loud[..250], c, 8).unwrap();
        assert!(b > a * 10, "power is quadratic in amplitude: {a} then {b}");
    }

    #[test]
    fn silence_has_no_power_anywhere() {
        let s = [0i32; 128];
        for f in [7_000u32, 10_000, 13_000] {
            let p = goertzel_power(&s, goertzel_coeff_q14(f, 250).unwrap(), 8).unwrap();
            assert_eq!(p, 0, "silence must not resonate at {} Hz", f / 1000);
        }
    }

    #[test]
    fn a_bank_answers_the_shape_an_ssvep_decision_needs() {
        // Four flicker rates, four numbers, and the argmax is the caller's.
        let s = tone(13, 250, 250, 80_000);
        let coeffs: [i32; 4] =
            [8_000, 10_000, 13_000, 15_000].map(|f| goertzel_coeff_q14(f, 250).unwrap());
        let mut out = [0u64; 4];
        goertzel_bank(&s[..250], &coeffs, 8, &mut out).unwrap();
        let best = out
            .iter()
            .enumerate()
            .max_by_key(|(_, &v)| v)
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(best, 2, "the 13 Hz target must win: {out:?}");
    }

    #[test]
    fn a_wrongly_sized_bank_output_is_refused() {
        let s = [1i32; 32];
        let coeffs = [COEFF_ONE, COEFF_ONE];
        let mut out = [0u64; 3];
        assert_eq!(
            goertzel_bank(&s, &coeffs, 8, &mut out),
            Err(PipelineError::OutputLengthMismatch)
        );
        assert_eq!(
            goertzel_bank(&s, &[], 8, &mut out),
            Err(PipelineError::EmptyKernel)
        );
    }

    #[test]
    fn bad_arguments_are_refused_rather_than_computed() {
        assert_eq!(
            goertzel_power(&[], COEFF_ONE, 8),
            Err(PipelineError::EmptyInput)
        );
        assert_eq!(
            goertzel_power(&[1, 2], COEFF_MAX + 1, 8),
            Err(PipelineError::InvalidCoefficient)
        );
        assert_eq!(
            goertzel_power(&[1, 2], COEFF_ONE, 32),
            Err(PipelineError::InvalidShift)
        );
    }

    #[test]
    fn a_resonant_input_saturates_rather_than_wrapping() {
        // A tone exactly at the bin, at full scale, for a long window: the state
        // must bound rather than wrap into a small plausible number.
        let s = tone(10, 250, 256, 8_000_000);
        let p = goertzel_power(&s, goertzel_coeff_q14(10_000, 250).unwrap(), 12).unwrap();
        assert!(p > 0, "a saturated bin must report large, not zero");
    }

    #[test]
    fn a_ratio_is_permille_and_says_when_it_cannot_be_taken() {
        assert_eq!(power_ratio_permille(500, 1_000), Some(500));
        assert_eq!(power_ratio_permille(2_000, 1_000), Some(2_000));
        assert_eq!(
            power_ratio_permille(1, 0),
            None,
            "no denominator is an absent measurement, not a huge ratio"
        );
        assert_eq!(power_ratio_permille(0, 1_000), Some(0));
    }

    #[test]
    fn a_ratio_saturates_instead_of_overflowing() {
        assert_eq!(power_ratio_permille(u64::MAX, 1), Some(u32::MAX));
    }

    #[test]
    fn everything_here_is_deterministic() {
        let s = tone(11, 250, 250, 50_000);
        let c = goertzel_coeff_q14(11_000, 250).unwrap();
        let run = || goertzel_power(&s[..250], c, 8).unwrap();
        assert_eq!(run(), run());
        assert_eq!(
            goertzel_coeff_q14(11_000, 250),
            goertzel_coeff_q14(11_000, 250)
        );
    }
}
