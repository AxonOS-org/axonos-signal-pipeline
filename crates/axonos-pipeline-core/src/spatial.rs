// SPDX-License-Identifier: Apache-2.0 OR MIT
// SPDX-FileCopyrightText: 2026 Denis Yermakou <connect@axonos.org>
//
// Part of the AxonOS Signal Pipeline. Dual-licensed Apache-2.0 OR MIT at your
// option; see LICENSE-APACHE and LICENSE-MIT. Authored by Denis Yermakou for The
// AxonOS Project — https://axonos.org

//! Spatial operations across channels — v0.9.0.
//!
//! Everything before this module works down a single channel at a time. Scalp
//! potentials are not single-channel quantities: every electrode measures the
//! same sources through a different mixture, and the first thing any usable
//! decoder does is recombine them. This module is that recombination, and it
//! closes the "spatial filtering" entry the roadmap has carried as deferred
//! since v0.1.0.
//!
//! Two kinds of operation, deliberately separated because they answer different
//! questions:
//!
//! **Re-referencing** removes what every electrode shares. A recording is only
//! ever a potential *difference*, and the reference is a choice — a mastoid, an
//! average, a neighbour. Changing it changes every number downstream, so it
//! belongs at a declared point in the chain rather than wherever a caller
//! happens to do it.
//!
//! **Spatial filtering** applies a caller-supplied matrix, the shape CSP, xDAWN
//! and Laplacian montages all take. This crate does not *learn* such a matrix:
//! that requires labelled data and would be a decoder, which
//! [`docs/CLAIMS.md`](../docs/CLAIMS.md) does not claim. It applies one, exactly
//! and reproducibly, so a matrix trained anywhere can be executed here.
//!
//! ## Arithmetic
//!
//! Integer throughout, `Q16` for filter coefficients, `i64` accumulation. No
//! floating point, so two implementations agree bit for bit rather than
//! approximately — the same discipline as the rest of the crate and the reason
//! the conformance vectors mean anything.
//!
//! Interleaving matches the rest of the pipeline: samples arrive as
//! `[t0c0, t0c1, …, t1c0, …]`, so a frame is a row-major matrix of `frames`
//! rows by `channels` columns.

use crate::error::PipelineError;

/// Fractional bits in a spatial filter coefficient.
pub const SPATIAL_SHIFT: u32 = 16;

/// One unit of a `Q16` coefficient.
pub const SPATIAL_ONE: i32 = 1 << SPATIAL_SHIFT;

/// Which shared component a re-reference removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reference {
    /// Common average reference: subtract the mean across all enabled channels
    /// at each instant.
    ///
    /// The workhorse of scalp EEG and the correct default when no electrode is
    /// privileged. It has a known cost that is worth stating rather than
    /// discovering: with few channels the average is dominated by whichever
    /// channels are active, so CAR mixes signal into its own reference. Below
    /// roughly eight channels this is a real distortion, not a rounding error.
    CommonAverage,
    /// Subtract one nominated channel from all others.
    ///
    /// The column index is into the *enabled* channels, not the physical
    /// electrode number — the same convention [`crate::mask::ChannelMask`] uses,
    /// so a disabled electrode cannot silently become the reference.
    SingleChannel(usize),
    /// Subtract the mean of a nominated set of channels.
    ///
    /// A linked-mastoid reference is this with two columns. Columns outside the
    /// enabled set are refused rather than ignored: a reference silently
    /// computed from fewer electrodes than the caller named is a different
    /// reference.
    Average(&'static [usize]),
}

/// Re-reference a block in place.
///
/// Integer mean with truncation toward zero, applied identically to every
/// channel at each instant — so the operation removes exactly the shared
/// component and introduces no per-channel bias. Saturating subtraction: a
/// re-reference must not wrap a sample that was already near a rail.
pub fn rereference(
    samples: &mut [i32],
    channels: usize,
    reference: Reference,
) -> Result<(), PipelineError> {
    if channels == 0 {
        return Err(PipelineError::EmptyChannelMask);
    }
    if samples.is_empty() || samples.len() % channels != 0 {
        return Err(PipelineError::SampleLengthMismatch);
    }
    match reference {
        Reference::SingleChannel(c) if c >= channels => {
            return Err(PipelineError::DimensionMismatch)
        }
        Reference::Average(cols) => {
            if cols.is_empty() {
                return Err(PipelineError::EmptyChannelMask);
            }
            if cols.iter().any(|&c| c >= channels) {
                return Err(PipelineError::DimensionMismatch);
            }
        }
        _ => {}
    }

    // chunks_mut rather than an index: the block *is* a sequence of instants,
    // and saying so lets the compiler drop the bounds checks that arithmetic
    // indexing keeps.
    for row in samples.chunks_mut(channels) {
        let refv: i64 = match reference {
            Reference::CommonAverage => {
                let sum: i64 = row.iter().map(|&s| s as i64).sum();
                sum / channels as i64
            }
            Reference::SingleChannel(c) => row[c] as i64,
            Reference::Average(cols) => {
                let sum: i64 = cols.iter().map(|&c| row[c] as i64).sum();
                sum / cols.len() as i64
            }
        };
        for s in row.iter_mut() {
            *s = (*s as i64 - refv).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        }
    }
    Ok(())
}

/// A caller-supplied spatial filter: `out[c] = Σ_k w[c][k] · in[k]`.
///
/// Row-major, `Q16`, `OUT` output components from `IN` input channels. Const
/// dimensions rather than slices so a dimension mismatch is a compile error
/// where it can be, and so the type carries no allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpatialFilter<const OUT: usize, const IN: usize> {
    weights: [[i32; IN]; OUT],
}

impl<const OUT: usize, const IN: usize> SpatialFilter<OUT, IN> {
    /// Wrap a weight matrix. `Q16`, row-major, output-major.
    pub const fn new(weights: [[i32; IN]; OUT]) -> Self {
        Self { weights }
    }

    /// The identity, for the degenerate case and for tests that need a filter
    /// which provably changes nothing.
    pub fn identity() -> Self {
        let mut w = [[0i32; IN]; OUT];
        for (i, row) in w.iter_mut().enumerate() {
            if i < IN {
                row[i] = SPATIAL_ONE;
            }
        }
        Self::new(w)
    }

    /// Common-average-reference expressed as a matrix, for callers that prefer
    /// one spatial stage to two.
    ///
    /// Exact when `IN` divides `SPATIAL_ONE` and rounded otherwise, which is the
    /// honest trade for keeping the coefficient integral: at `IN = 3` the
    /// diagonal is `1 − 21845/65536` rather than exactly `2/3`. Where exactness
    /// matters, use [`Reference::CommonAverage`], whose integer mean does not go
    /// through a coefficient at all.
    pub fn common_average() -> Self {
        let share = SPATIAL_ONE / IN as i32;
        let mut w = [[-share; IN]; OUT];
        for (i, row) in w.iter_mut().enumerate() {
            if i < IN {
                row[i] = SPATIAL_ONE - share;
            }
        }
        Self::new(w)
    }

    /// The weight matrix.
    pub const fn weights(&self) -> &[[i32; IN]; OUT] {
        &self.weights
    }

    /// Apply to one instant: `IN` channels in, `OUT` components out.
    ///
    /// Accumulates in `i64` and saturates on the way out, so a filter with large
    /// coefficients degrades to a rail rather than wrapping into a plausible
    /// wrong number.
    pub fn apply_frame(&self, input: &[i32; IN]) -> [i32; OUT] {
        let mut out = [0i32; OUT];
        for (o, row) in out.iter_mut().zip(self.weights.iter()) {
            let mut acc: i64 = 0;
            for (&w, &x) in row.iter().zip(input.iter()) {
                acc += w as i64 * x as i64;
            }
            let scaled = acc >> SPATIAL_SHIFT;
            *o = scaled.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        }
        out
    }

    /// Apply across an interleaved block, writing `OUT` components per instant.
    ///
    /// `out` must hold exactly `frames × OUT`; a mismatch is refused rather than
    /// truncated, because a caller who sized the buffer wrongly has a different
    /// bug than the one a truncated result would show them.
    pub fn apply_block(&self, input: &[i32], out: &mut [i32]) -> Result<(), PipelineError> {
        if IN == 0 || OUT == 0 {
            return Err(PipelineError::EmptyChannelMask);
        }
        if input.is_empty() || input.len() % IN != 0 {
            return Err(PipelineError::SampleLengthMismatch);
        }
        let frames = input.len() / IN;
        if out.len() != frames * OUT {
            return Err(PipelineError::OutputLengthMismatch);
        }
        let mut buf = [0i32; IN];
        for (chunk, slot) in input.chunks(IN).zip(out.chunks_mut(OUT)) {
            buf.copy_from_slice(chunk);
            slot.copy_from_slice(&self.apply_frame(&buf));
        }
        Ok(())
    }
}

/// Remove a per-channel baseline measured over the first `baseline_frames`
/// instants of the block.
///
/// Every epoch-based paradigm needs this and nearly every implementation gets
/// the ordering wrong: the baseline must be measured *before* the window of
/// interest and subtracted from the whole epoch, not measured over the epoch it
/// corrects. Passing the baseline length explicitly makes that ordering part of
/// the call rather than a convention.
///
/// Refuses a baseline longer than the block: a correction computed from samples
/// that do not exist is not a correction.
pub fn remove_baseline(
    samples: &mut [i32],
    channels: usize,
    baseline_frames: usize,
) -> Result<(), PipelineError> {
    if channels == 0 {
        return Err(PipelineError::EmptyChannelMask);
    }
    if samples.is_empty() || samples.len() % channels != 0 {
        return Err(PipelineError::SampleLengthMismatch);
    }
    if baseline_frames == 0 {
        return Err(PipelineError::InvalidWindow);
    }
    let frames = samples.len() / channels;
    if baseline_frames > frames {
        return Err(PipelineError::WindowTooLarge);
    }
    for c in 0..channels {
        let mut sum: i64 = 0;
        for f in 0..baseline_frames {
            sum += samples[f * channels + c] as i64;
        }
        let base = sum / baseline_frames as i64;
        for f in 0..frames {
            let i = f * channels + c;
            samples[i] = (samples[i] as i64 - base).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_average_removes_what_every_channel_shares() {
        // A common-mode offset of 100 on all three channels, plus a distinct
        // per-channel signal. CAR must delete the first and keep the second.
        let mut s = [110i32, 90, 100, 210, 190, 200];
        rereference(&mut s, 3, Reference::CommonAverage).unwrap();
        assert_eq!(s, [10, -10, 0, 10, -10, 0]);
    }

    #[test]
    fn a_single_channel_reference_zeroes_itself() {
        let mut s = [10i32, 20, 30];
        rereference(&mut s, 3, Reference::SingleChannel(1)).unwrap();
        assert_eq!(s, [-10, 0, 10]);
    }

    #[test]
    fn an_average_reference_over_two_channels_is_a_linked_mastoid() {
        const PAIR: &[usize] = &[0, 2];
        let mut s = [100i32, 50, 200];
        rereference(&mut s, 3, Reference::Average(PAIR)).unwrap();
        // mean of channels 0 and 2 is 150
        assert_eq!(s, [-50, -100, 50]);
    }

    #[test]
    fn a_reference_outside_the_enabled_set_is_refused() {
        let mut s = [1i32, 2, 3];
        assert_eq!(
            rereference(&mut s, 3, Reference::SingleChannel(3)),
            Err(PipelineError::DimensionMismatch)
        );
        const BAD: &[usize] = &[0, 9];
        assert_eq!(
            rereference(&mut s, 3, Reference::Average(BAD)),
            Err(PipelineError::DimensionMismatch)
        );
        assert_eq!(s, [1, 2, 3], "a refused re-reference must not have written");
    }

    #[test]
    fn re_referencing_saturates_rather_than_wrapping() {
        let mut s = [i32::MIN, 1000, 1000];
        rereference(&mut s, 3, Reference::SingleChannel(1)).unwrap();
        assert_eq!(
            s[0],
            i32::MIN,
            "must clamp, not wrap into a plausible value"
        );
    }

    #[test]
    fn a_ragged_block_is_refused() {
        let mut s = [1i32, 2, 3, 4, 5];
        assert_eq!(
            rereference(&mut s, 3, Reference::CommonAverage),
            Err(PipelineError::SampleLengthMismatch)
        );
    }

    #[test]
    fn the_identity_filter_changes_nothing() {
        let f = SpatialFilter::<3, 3>::identity();
        assert_eq!(f.apply_frame(&[7, -9, 1234]), [7, -9, 1234]);
    }

    #[test]
    fn a_spatial_filter_mixes_as_declared() {
        // out0 = in0 + in1, out1 = in0 − in1
        let f =
            SpatialFilter::<2, 2>::new([[SPATIAL_ONE, SPATIAL_ONE], [SPATIAL_ONE, -SPATIAL_ONE]]);
        assert_eq!(f.apply_frame(&[300, 100]), [400, 200]);
    }

    #[test]
    fn a_filter_may_reduce_dimension() {
        // Four electrodes into one component, which is what a trained CSP
        // projection does and what the pipeline had no way to express.
        let q = SPATIAL_ONE / 4;
        let f = SpatialFilter::<1, 4>::new([[q, q, q, q]]);
        assert_eq!(f.apply_frame(&[400, 400, 400, 400]), [400]);
    }

    #[test]
    fn a_block_is_filtered_instant_by_instant() {
        let f =
            SpatialFilter::<2, 2>::new([[SPATIAL_ONE, SPATIAL_ONE], [SPATIAL_ONE, -SPATIAL_ONE]]);
        let input = [300i32, 100, 50, 20];
        let mut out = [0i32; 4];
        f.apply_block(&input, &mut out).unwrap();
        assert_eq!(out, [400, 200, 70, 30]);
    }

    #[test]
    fn a_wrongly_sized_output_is_refused_not_truncated() {
        let f = SpatialFilter::<2, 2>::identity();
        let input = [1i32, 2, 3, 4];
        let mut out = [0i32; 3];
        assert_eq!(
            f.apply_block(&input, &mut out),
            Err(PipelineError::OutputLengthMismatch)
        );
    }

    #[test]
    fn the_matrix_form_of_car_agrees_with_the_integer_form_where_it_can() {
        // IN = 4 divides Q16 exactly, so the two paths must agree to the last
        // count. Where IN does not divide it they differ, which is why the
        // matrix form documents itself as the rounded one.
        let f = SpatialFilter::<4, 4>::common_average();
        let frame = [110i32, 90, 100, 100];
        let via_matrix = f.apply_frame(&frame);
        let mut via_integer = frame;
        rereference(&mut via_integer, 4, Reference::CommonAverage).unwrap();
        assert_eq!(via_matrix, via_integer);
    }

    #[test]
    fn a_spatial_filter_saturates_rather_than_wrapping() {
        let big = SpatialFilter::<1, 1>::new([[SPATIAL_ONE * 4]]);
        assert_eq!(big.apply_frame(&[i32::MAX]), [i32::MAX]);
    }

    #[test]
    fn baseline_removal_uses_only_the_declared_prefix() {
        // Two channels. The first four instants are the baseline; the rest is
        // the response and must not influence its own correction.
        let mut s = [
            10i32, 100, 10, 100, 10, 100, 10, 100, // baseline: 10 and 100
            50, 300, 60, 400, // response
        ];
        remove_baseline(&mut s, 2, 4).unwrap();
        assert_eq!(&s[..8], &[0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&s[8..], &[40, 200, 50, 300]);
    }

    #[test]
    fn a_baseline_longer_than_the_block_is_refused() {
        let mut s = [1i32, 2, 3, 4];
        assert_eq!(
            remove_baseline(&mut s, 2, 3),
            Err(PipelineError::WindowTooLarge)
        );
        assert_eq!(
            s,
            [1, 2, 3, 4],
            "a refused correction must not have written"
        );
    }

    #[test]
    fn a_zero_length_baseline_is_refused() {
        let mut s = [1i32, 2];
        assert_eq!(
            remove_baseline(&mut s, 2, 0),
            Err(PipelineError::InvalidWindow)
        );
    }

    #[test]
    fn every_spatial_operation_is_deterministic() {
        let run = || {
            let mut s = [113i32, -97, 1004, 7, -3, 55];
            rereference(&mut s, 3, Reference::CommonAverage).unwrap();
            remove_baseline(&mut s, 3, 1).unwrap();
            let f = SpatialFilter::<2, 3>::new([
                [SPATIAL_ONE, -SPATIAL_ONE / 2, 0],
                [0, SPATIAL_ONE / 3, SPATIAL_ONE],
            ]);
            let mut out = [0i32; 4];
            f.apply_block(&s, &mut out).unwrap();
            out
        };
        assert_eq!(run(), run());
    }
}
