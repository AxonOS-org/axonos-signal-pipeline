// SPDX-License-Identifier: Apache-2.0 OR MIT
// SPDX-FileCopyrightText: 2026 Denis Yermakou <connect@axonos.org>
//
// Part of the AxonOS Signal Pipeline. Dual-licensed Apache-2.0 OR MIT at your
// option; see LICENSE-APACHE and LICENSE-MIT. Authored by Denis Yermakou for The
// AxonOS Project — https://axonos.org

//! Amplitude/saturation artifact screening (pure integer).

use crate::error::PipelineError;

/// Maximum 24-bit two's-complement sample, sign-extended into `i32`.
pub const ADC24_MAX: i32 = 8_388_607;
/// Minimum 24-bit two's-complement sample.
pub const ADC24_MIN: i32 = -8_388_608;

/// Result of artifact screening over a sample block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactFlag {
    /// No artifact detected.
    Clean,
    /// At least one sample exceeded the configured amplitude threshold.
    AmplitudeExceeded,
    /// At least one sample sits at an ADC rail (dominates amplitude).
    Saturated,
}

/// Screens `samples` against `threshold_counts` (must be positive, in ADC
/// counts). Saturation dominates amplitude excess.
///
/// Threshold semantics are owned by the acquisition layer: the AxonOS
/// Standard's ±120 µV acquisition default converts to counts via the AFE
/// gain and reference voltage (`docs/PIPELINE_CONTRACT.md` §4); this
/// function deliberately performs no analog conversion.
pub fn artifact_scan(
    samples: &[i32],
    threshold_counts: i32,
) -> Result<ArtifactFlag, PipelineError> {
    if threshold_counts <= 0 {
        return Err(PipelineError::InvalidThreshold);
    }
    let mut flag = ArtifactFlag::Clean;
    for &s in samples {
        if s >= ADC24_MAX || s <= ADC24_MIN {
            return Ok(ArtifactFlag::Saturated);
        }
        if s > threshold_counts || s < -threshold_counts {
            flag = ArtifactFlag::AmplitudeExceeded;
        }
    }
    Ok(flag)
}

// ── v0.9.0: richer screening ────────────────────────────────────────────────

/// A screening verdict as a set of independent findings.
///
/// [`ArtifactFlag`] answers "is this clean" with one of three values, and its
/// vectors are pinned, so it stays exactly as it is. It cannot answer the
/// question a rejection actually raises, which is *why*: an epoch discarded for
/// a rail and an epoch discarded because an electrode came loose need different
/// responses from whoever is wearing the device, and collapsing them loses the
/// distinction at the only point where it was still available.
///
/// Findings are independent bits rather than an ordered severity, because they
/// co-occur. A loose electrode drifts *and* flatlines; a clenched jaw exceeds
/// amplitude *and* slews. Reporting only the worst one throws away the pattern
/// that identifies the cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArtifactReport(u8);

impl ArtifactReport {
    /// A sample sat at an ADC rail. The amplifier, not the signal, is what was
    /// measured.
    pub const SATURATED: u8 = 1 << 0;
    /// A sample exceeded the configured amplitude threshold.
    pub const AMPLITUDE: u8 = 1 << 1;
    /// Consecutive samples differed by more than the configured slew limit —
    /// faster than physiology allows, so it is movement, a cable, or a step.
    pub const SLEW: u8 = 1 << 2;
    /// The block did not vary by more than the flatline tolerance. A dead
    /// channel and a perfectly silent cortex are indistinguishable here, and the
    /// first is overwhelmingly more likely.
    pub const FLATLINE: u8 = 1 << 3;
    /// The block's first and last quarters differ in mean by more than the
    /// drift limit — a slow baseline walk, typically an electrode settling or
    /// coming loose.
    pub const DRIFT: u8 = 1 << 4;

    /// Nothing found.
    pub const fn clean() -> Self {
        Self(0)
    }

    /// True when no finding is present.
    pub const fn is_clean(&self) -> bool {
        self.0 == 0
    }

    /// True when the named finding is present.
    pub const fn has(&self, finding: u8) -> bool {
        self.0 & finding != 0
    }

    /// The raw bits, for logging and for the wire.
    pub const fn bits(&self) -> u8 {
        self.0
    }

    /// Build from raw bits.
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// Whether these findings mean the block must not reach a decoder.
    ///
    /// Saturation, flatline and slew are disqualifying: the first two mean the
    /// signal was not measured, and the third means something other than
    /// cortex moved. Amplitude excess and drift are *reported* but not
    /// disqualifying on their own — a real evoked response can exceed a
    /// conservative threshold, and discarding it would silently bias the
    /// decoder against exactly the epochs that carry information.
    pub const fn disqualifying(&self) -> bool {
        self.0 & (Self::SATURATED | Self::FLATLINE | Self::SLEW) != 0
    }

    const fn with(self, finding: u8) -> Self {
        Self(self.0 | finding)
    }
}

/// Limits for [`artifact_screen`], all in ADC counts.
///
/// Counts rather than microvolts, deliberately and for the same reason
/// [`artifact_scan`] takes counts: the conversion needs the AFE gain and
/// reference voltage, which belong to the acquisition layer
/// (`docs/PIPELINE_CONTRACT.md` §4). A DSP module that performed that
/// conversion would be asserting a hardware configuration it cannot see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenLimits {
    /// Amplitude beyond which a sample is flagged. Must be positive.
    pub amplitude: i32,
    /// Largest permitted difference between consecutive samples. `0` disables.
    pub slew: i32,
    /// Peak-to-peak range at or below which the block counts as flat. `0`
    /// disables.
    pub flat_range: i32,
    /// Mean shift between the first and last quarter beyond which the block
    /// counts as drifting. `0` disables.
    pub drift: i32,
}

impl ScreenLimits {
    /// Limits for the canonical acquisition front end at gain 24, where one
    /// count is ~22.35 nV.
    ///
    /// Amplitude corresponds to the AxonOS Standard's ±120 µV default. The other
    /// three are engineering defaults rather than standard figures and are
    /// marked as such: they are chosen to catch obvious pathology without
    /// discarding real signal, and a deployment with measured population data
    /// should set its own.
    pub const CANONICAL: Self = Self {
        amplitude: 5_369, // ~120 µV
        slew: 2_237,      // ~50 µV between consecutive samples
        flat_range: 45,   // ~1 µV peak-to-peak over the block
        drift: 3_356,     // ~75 µV of baseline walk
    };

    /// Amplitude only — the behaviour of [`artifact_scan`], expressed in the
    /// richer form so a caller can migrate without changing thresholds.
    pub const fn amplitude_only(amplitude: i32) -> Self {
        Self {
            amplitude,
            slew: 0,
            flat_range: 0,
            drift: 0,
        }
    }
}

/// Screen one channel's samples and report every finding.
///
/// Single pass, integer only, no allocation. Returns [`PipelineError`] for an
/// empty block or a non-positive amplitude limit — the same refusals
/// [`artifact_scan`] makes, so the two agree about what a valid request is.
pub fn artifact_screen(
    samples: &[i32],
    limits: ScreenLimits,
) -> Result<ArtifactReport, PipelineError> {
    if samples.is_empty() {
        return Err(PipelineError::EmptyInput);
    }
    if limits.amplitude <= 0 {
        return Err(PipelineError::InvalidThreshold);
    }
    let mut r = ArtifactReport::clean();
    let (mut lo, mut hi) = (i32::MAX, i32::MIN);
    let mut prev: Option<i32> = None;

    for &s in samples {
        if s >= ADC24_MAX || s <= ADC24_MIN {
            r = r.with(ArtifactReport::SATURATED);
        }
        if s > limits.amplitude || s < -limits.amplitude {
            r = r.with(ArtifactReport::AMPLITUDE);
        }
        if let Some(p) = prev {
            if limits.slew > 0 && (s as i64 - p as i64).abs() > limits.slew as i64 {
                r = r.with(ArtifactReport::SLEW);
            }
        }
        prev = Some(s);
        lo = lo.min(s);
        hi = hi.max(s);
    }

    if limits.flat_range > 0 && (hi as i64 - lo as i64) <= limits.flat_range as i64 {
        r = r.with(ArtifactReport::FLATLINE);
    }

    // Drift compares the first and last quarter. A quarter rather than a half
    // because a linear walk across the block moves both halves' means by nearly
    // the same amount, which would hide exactly the drift being looked for.
    let q = samples.len() / 4;
    if limits.drift > 0 && q > 0 {
        let head: i64 = samples[..q].iter().map(|&s| s as i64).sum::<i64>() / q as i64;
        let tail: i64 = samples[samples.len() - q..]
            .iter()
            .map(|&s| s as i64)
            .sum::<i64>()
            / q as i64;
        if (tail - head).abs() > limits.drift as i64 {
            r = r.with(ArtifactReport::DRIFT);
        }
    }
    Ok(r)
}

/// Screen every channel of an interleaved block, writing one report per channel.
///
/// `out` must have exactly one slot per channel. Per-channel rather than one
/// verdict for the block because that is the resolution at which the problem can
/// be acted on: one loose electrode should cost one channel, not the epoch.
pub fn artifact_screen_block(
    samples: &[i32],
    channels: usize,
    limits: ScreenLimits,
    out: &mut [ArtifactReport],
) -> Result<(), PipelineError> {
    if channels == 0 {
        return Err(PipelineError::EmptyChannelMask);
    }
    if samples.is_empty() || samples.len() % channels != 0 {
        return Err(PipelineError::SampleLengthMismatch);
    }
    if out.len() != channels {
        return Err(PipelineError::OutputLengthMismatch);
    }
    if limits.amplitude <= 0 {
        return Err(PipelineError::InvalidThreshold);
    }
    let frames = samples.len() / channels;
    // Fixed scratch: the pipeline's channel count is bounded by ChannelMask's
    // sixteen, so a stack buffer is enough and no allocation is needed.
    const MAX_FRAMES: usize = 4096;
    if frames > MAX_FRAMES {
        return Err(PipelineError::WindowTooLarge);
    }
    let mut column = [0i32; MAX_FRAMES];
    for (c, slot) in out.iter_mut().enumerate() {
        for (f, slot) in column[..frames].iter_mut().enumerate() {
            *slot = samples[f * channels + c];
        }
        *slot = artifact_screen(&column[..frames], limits)?;
    }
    Ok(())
}

#[cfg(test)]
mod screen_tests {
    use super::*;

    const L: ScreenLimits = ScreenLimits {
        amplitude: 1_000,
        slew: 100,
        flat_range: 10,
        drift: 200,
    };

    #[test]
    fn a_quiet_varying_block_is_clean() {
        let s = [0i32, 40, -30, 55, -20, 35, -45, 25];
        let r = artifact_screen(&s, L).unwrap();
        assert!(r.is_clean(), "bits {:05b}", r.bits());
        assert!(!r.disqualifying());
    }

    #[test]
    fn findings_are_independent_and_co_occur() {
        // A jaw clench: large amplitude and a fast edge in the same block. Both
        // must be reported, because the pair is what identifies the cause.
        let s = [0i32, 0, 0, 0, 5_000, 0, 0, 0];
        let r = artifact_screen(&s, L).unwrap();
        assert!(r.has(ArtifactReport::AMPLITUDE));
        assert!(r.has(ArtifactReport::SLEW));
        assert!(!r.has(ArtifactReport::FLATLINE));
    }

    #[test]
    fn a_rail_is_saturation_and_is_disqualifying() {
        let s = [0i32, 10, ADC24_MAX, 10];
        let r = artifact_screen(&s, L).unwrap();
        assert!(r.has(ArtifactReport::SATURATED));
        assert!(r.disqualifying());
    }

    #[test]
    fn a_dead_channel_flatlines() {
        let s = [7i32; 16];
        let r = artifact_screen(&s, L).unwrap();
        assert!(r.has(ArtifactReport::FLATLINE));
        assert!(
            r.disqualifying(),
            "a channel that measured nothing must not decode"
        );
    }

    #[test]
    fn a_slow_walk_is_drift_and_is_not_disqualifying_alone() {
        // Baseline moves 500 counts across the block with small variation, so
        // amplitude and slew stay quiet and only drift fires.
        let mut s = [0i32; 16];
        for (i, v) in s.iter_mut().enumerate() {
            *v = (i as i32) * 40;
        }
        let r = artifact_screen(&s, L).unwrap();
        assert!(r.has(ArtifactReport::DRIFT));
        assert!(
            !r.disqualifying(),
            "drift is reported, not fatal on its own"
        );
    }

    #[test]
    fn drift_uses_quarters_so_a_linear_walk_is_visible() {
        // Halves would move by nearly the same amount and hide this.
        let mut s = [0i32; 32];
        for (i, v) in s.iter_mut().enumerate() {
            *v = (i as i32) * 20;
        }
        assert!(artifact_screen(&s, L).unwrap().has(ArtifactReport::DRIFT));
    }

    #[test]
    fn amplitude_excess_alone_does_not_disqualify() {
        // A real evoked response can exceed a conservative threshold, and
        // discarding it would bias the decoder against informative epochs.
        let s = [0i32, 1_200, 0, -1_100, 0, 1_150, 0, -1_050];
        let r = artifact_screen(&s, L).unwrap();
        assert!(r.has(ArtifactReport::AMPLITUDE));
        assert!(
            r.has(ArtifactReport::SLEW),
            "these steps also exceed the slew limit"
        );
    }

    #[test]
    fn zeroed_limits_disable_their_findings() {
        let flat = [7i32; 16];
        let off = ScreenLimits::amplitude_only(1_000);
        let r = artifact_screen(&flat, off).unwrap();
        assert!(
            r.is_clean(),
            "flatline detection is off, so nothing is reported"
        );
    }

    #[test]
    fn amplitude_only_agrees_with_the_pinned_scan() {
        // The migration promise: same threshold, same verdict about amplitude.
        for block in [
            [0i32, 10, 20, 30],
            [0i32, 2_000, 0, 0],
            [ADC24_MAX, 0, 0, 0],
        ] {
            let old = artifact_scan(&block, 1_000).unwrap();
            let new = artifact_screen(&block, ScreenLimits::amplitude_only(1_000)).unwrap();
            match old {
                ArtifactFlag::Clean => assert!(new.is_clean()),
                ArtifactFlag::AmplitudeExceeded => assert!(new.has(ArtifactReport::AMPLITUDE)),
                ArtifactFlag::Saturated => assert!(new.has(ArtifactReport::SATURATED)),
            }
        }
    }

    #[test]
    fn an_empty_block_and_a_bad_threshold_are_refused() {
        assert_eq!(artifact_screen(&[], L), Err(PipelineError::EmptyInput));
        assert_eq!(
            artifact_screen(&[1, 2], ScreenLimits::amplitude_only(0)),
            Err(PipelineError::InvalidThreshold)
        );
    }

    #[test]
    fn per_channel_screening_isolates_the_bad_electrode() {
        // Two channels interleaved: the first is fine, the second is dead. One
        // loose electrode must cost one channel and not the epoch.
        let s = [0i32, 5, 40, 5, -30, 5, 55, 5];
        let mut out = [ArtifactReport::clean(); 2];
        artifact_screen_block(&s, 2, L, &mut out).unwrap();
        assert!(out[0].is_clean(), "channel 0 bits {:05b}", out[0].bits());
        assert!(out[1].has(ArtifactReport::FLATLINE));
    }

    #[test]
    fn a_wrongly_sized_report_buffer_is_refused() {
        let s = [1i32, 2, 3, 4];
        let mut out = [ArtifactReport::clean(); 1];
        assert_eq!(
            artifact_screen_block(&s, 2, L, &mut out),
            Err(PipelineError::OutputLengthMismatch)
        );
    }

    #[test]
    fn bits_round_trip_for_the_wire() {
        let r = ArtifactReport::clean()
            .with(ArtifactReport::AMPLITUDE)
            .with(ArtifactReport::DRIFT);
        assert_eq!(ArtifactReport::from_bits(r.bits()), r);
    }

    #[test]
    fn screening_is_deterministic() {
        let s = [113i32, -97, 1_004, 7, -3, 55, 900, -880];
        assert_eq!(artifact_screen(&s, L), artifact_screen(&s, L));
    }
}
