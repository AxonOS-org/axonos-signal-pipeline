// SPDX-License-Identifier: Apache-2.0 OR MIT
// SPDX-FileCopyrightText: 2026 Denis Yermakou <connect@axonos.org>
//
// Part of the AxonOS Signal Pipeline. Dual-licensed Apache-2.0 OR MIT at your
// option; see LICENSE-APACHE and LICENSE-MIT. Authored by Denis Yermakou for The
// AxonOS Project — https://axonos.org

//! Sampling-rate newtype.

use crate::error::PipelineError;

/// Sampling rate in hertz. Guaranteed non-zero by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SampleRate(u32);

impl SampleRate {
    /// 250 samples per second (ADS1299 default in the AxonOS reference stack).
    pub const HZ_250: SampleRate = SampleRate(250);
    /// 500 samples per second.
    pub const HZ_500: SampleRate = SampleRate(500);
    /// 1000 samples per second.
    pub const HZ_1000: SampleRate = SampleRate(1000);

    /// Creates a rate; returns `None` for 0 Hz.
    pub const fn new(hz: u32) -> Option<Self> {
        if hz == 0 {
            None
        } else {
            Some(Self(hz))
        }
    }

    /// Rate in hertz.
    pub const fn hz(self) -> u32 {
        self.0
    }
}

// ── v0.9.0: decimation, and the guard it is useless without ────────────────

/// Decimate an interleaved block by an integer factor, keeping every `factor`-th
/// instant.
///
/// **This does not filter.** Dropping samples folds everything above the new
/// Nyquist back into the band that survives, and folded energy is
/// indistinguishable from signal once it has landed. A caller who decimates
/// unfiltered data has not made it smaller, they have made it wrong in a way
/// that no later stage can detect.
///
/// So this refuses to be used blind: [`decimate_checked`] takes the anti-alias
/// evidence and this raw form is named for what it is. Both are here because
/// a caller who has *already* band-limited — the FIR stage above, or a converter
/// configured to oversample — should not pay for a second filter, and forcing
/// them to would push them to write their own loop with no guard at all.
pub fn decimate_unfiltered(
    samples: &[i32],
    channels: usize,
    factor: usize,
    out: &mut [i32],
) -> Result<usize, PipelineError> {
    if channels == 0 {
        return Err(PipelineError::EmptyChannelMask);
    }
    if factor == 0 {
        return Err(PipelineError::InvalidDecimation);
    }
    if samples.is_empty() || samples.len() % channels != 0 {
        return Err(PipelineError::SampleLengthMismatch);
    }
    let frames = samples.len() / channels;
    let kept = frames.div_ceil(factor);
    if out.len() != kept * channels {
        return Err(PipelineError::OutputLengthMismatch);
    }
    for (k, chunk) in samples.chunks(channels).step_by(factor).enumerate() {
        out[k * channels..(k + 1) * channels].copy_from_slice(chunk);
    }
    Ok(kept)
}

/// Decimate, having stated the band limit that makes it safe.
///
/// `band_limit_milli_hz` is the highest frequency the caller asserts is present
/// after their filtering. The decimated rate's Nyquist must exceed it, or the
/// call is refused — the check that turns a silent corruption into a compile-
/// time-shaped argument at runtime.
///
/// The assertion is the caller's and this crate cannot verify it; what it can do
/// is require the assertion to be *made*, so a decimation appears in review with
/// its justification attached rather than as a bare stride.
pub fn decimate_checked(
    samples: &[i32],
    channels: usize,
    factor: usize,
    rate: SampleRate,
    band_limit_milli_hz: u32,
    out: &mut [i32],
) -> Result<usize, PipelineError> {
    if factor == 0 {
        return Err(PipelineError::InvalidDecimation);
    }
    let new_rate_hz = rate.hz() as u64 / factor as u64;
    if new_rate_hz == 0 {
        return Err(PipelineError::InvalidDecimation);
    }
    // Nyquist of the decimated rate, in millihertz, must be strictly above the
    // declared band limit.
    if new_rate_hz * 1_000 <= (band_limit_milli_hz as u64) * 2 {
        return Err(PipelineError::AliasingRisk);
    }
    decimate_unfiltered(samples, channels, factor, out)
}

/// Number of output instants a decimation will produce.
///
/// Exposed so a caller can size the buffer without reproducing the rounding
/// rule — a buffer sized by a second copy of that rule is a buffer that
/// eventually disagrees with the first.
pub const fn decimated_frames(frames: usize, factor: usize) -> usize {
    if factor == 0 {
        return 0;
    }
    frames.div_ceil(factor)
}

#[cfg(test)]
mod decimate_tests {
    use super::*;

    #[test]
    fn every_nth_instant_survives_with_its_channels_together() {
        // Two channels, six instants. Factor three keeps instants 0 and 3, and
        // must keep both channels of each — an off-by-one here would interleave
        // channel 0 of one instant with channel 1 of another.
        let s = [10i32, 11, 20, 21, 30, 31, 40, 41, 50, 51, 60, 61];
        let mut out = [0i32; 4];
        let kept = decimate_unfiltered(&s, 2, 3, &mut out).unwrap();
        assert_eq!(kept, 2);
        assert_eq!(out, [10, 11, 40, 41]);
    }

    #[test]
    fn a_factor_of_one_is_a_copy() {
        let s = [1i32, 2, 3, 4];
        let mut out = [0i32; 4];
        assert_eq!(decimate_unfiltered(&s, 2, 1, &mut out).unwrap(), 2);
        assert_eq!(out, s);
    }

    #[test]
    fn the_frame_count_rounds_up_and_the_helper_agrees() {
        // Seven instants by three is three kept, not two: the first, fourth and
        // seventh. A buffer sized by floor would truncate the last.
        assert_eq!(decimated_frames(7, 3), 3);
        assert_eq!(decimated_frames(6, 3), 2);
        assert_eq!(decimated_frames(0, 3), 0);
        assert_eq!(decimated_frames(7, 0), 0);
        let s = [0i32; 7];
        let mut out = [0i32; 3];
        assert_eq!(decimate_unfiltered(&s, 1, 3, &mut out).unwrap(), 3);
    }

    #[test]
    fn a_wrongly_sized_output_is_refused_not_truncated() {
        let s = [1i32; 6];
        let mut out = [0i32; 1];
        assert_eq!(
            decimate_unfiltered(&s, 1, 3, &mut out),
            Err(PipelineError::OutputLengthMismatch)
        );
    }

    #[test]
    fn a_zero_factor_is_refused() {
        let s = [1i32; 4];
        let mut out = [0i32; 4];
        assert_eq!(
            decimate_unfiltered(&s, 1, 0, &mut out),
            Err(PipelineError::InvalidDecimation)
        );
    }

    #[test]
    fn the_checked_form_refuses_a_decimation_that_would_alias() {
        // 250 SPS by 5 gives 50 SPS, Nyquist 25 Hz. A caller asserting content
        // to 40 Hz is asking for that content to fold, and gets refused.
        let s = [1i32; 10];
        let mut out = [0i32; 2];
        assert_eq!(
            decimate_checked(&s, 1, 5, SampleRate::new(250).unwrap(), 40_000, &mut out),
            Err(PipelineError::AliasingRisk)
        );
    }

    #[test]
    fn the_checked_form_permits_a_decimation_with_headroom() {
        // Same decimation, but the caller has filtered to 20 Hz: 25 > 20, safe.
        let s = [1i32; 10];
        let mut out = [0i32; 2];
        assert_eq!(
            decimate_checked(&s, 1, 5, SampleRate::new(250).unwrap(), 20_000, &mut out).unwrap(),
            2
        );
    }

    #[test]
    fn nyquist_exactly_is_refused_because_exactly_is_not_below() {
        // 250 by 5 is 50 SPS; a 25 Hz component sits exactly on Nyquist, where
        // it is not resolvable and its phase is lost. Equality is refused.
        let s = [1i32; 10];
        let mut out = [0i32; 2];
        assert_eq!(
            decimate_checked(&s, 1, 5, SampleRate::new(250).unwrap(), 25_000, &mut out),
            Err(PipelineError::AliasingRisk)
        );
    }

    #[test]
    fn decimation_is_deterministic() {
        let s: [i32; 24] = core::array::from_fn(|i| (i as i32) * 7 - 40);
        // Sized through the helper, which is why the helper is public: this
        // test was written with a hand-computed 8 and the call refused it,
        // which is the refusal working — 8 instants by 3 keeps 3, not 2.
        const KEPT: usize = decimated_frames(8, 3);
        let run = || {
            let mut out = [0i32; KEPT * 3];
            decimate_unfiltered(&s, 3, 3, &mut out).unwrap();
            out
        };
        assert_eq!(run(), run());
    }
}
