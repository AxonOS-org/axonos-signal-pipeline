//! Raw acquisition frame — the privacy-critical entry type.

use crate::checksum::Fnv1a64;
use crate::error::PipelineError;
use crate::mask::ChannelMask;
use crate::rate::SampleRate;
use core::fmt;

/// One block of raw ADC samples.
///
/// Samples are signed 24-bit ADC counts sign-extended into `i32`
/// (unit: **counts**, not volts — analog conversion is an
/// acquisition-layer concern, see `docs/PIPELINE_CONTRACT.md` §4).
/// Storage is time-major interleaved over column-compacted channels:
/// `samples[t * n_channels + column]`.
///
/// `RawFrame` must never cross the application boundary
/// (`docs/PRIVACY_BOUNDARY.md`); it is deliberately not
/// [`crate::boundary::BoundarySafe`], and its `Debug` output redacts
/// sample values so raw signal cannot leak through logs.
pub struct RawFrame<'a> {
    seq: u32,
    timestamp_us: u64,
    rate: SampleRate,
    channels: ChannelMask,
    samples: &'a [i32],
    samples_per_channel: usize,
}

impl<'a> RawFrame<'a> {
    /// Validates and wraps a sample block.
    pub fn new(
        seq: u32,
        timestamp_us: u64,
        rate: SampleRate,
        channels: ChannelMask,
        samples: &'a [i32],
    ) -> Result<Self, PipelineError> {
        let n = channels.enabled_count() as usize;
        if n == 0 {
            return Err(PipelineError::EmptyChannelMask);
        }
        if samples.is_empty() || samples.len() % n != 0 {
            return Err(PipelineError::SampleLengthMismatch);
        }
        Ok(Self {
            seq,
            timestamp_us,
            rate,
            channels,
            samples,
            samples_per_channel: samples.len() / n,
        })
    }

    /// Frame sequence number.
    pub const fn seq(&self) -> u32 {
        self.seq
    }

    /// Acquisition timestamp, microseconds (monotonic source defined by
    /// the acquisition layer).
    pub const fn timestamp_us(&self) -> u64 {
        self.timestamp_us
    }

    /// Sampling rate.
    pub const fn sample_rate(&self) -> SampleRate {
        self.rate
    }

    /// Enabled-channel mask.
    pub const fn channels(&self) -> ChannelMask {
        self.channels
    }

    /// Number of time points per channel.
    pub const fn samples_per_channel(&self) -> usize {
        self.samples_per_channel
    }

    /// Raw sample at time index `t`, storage column `col`.
    pub fn sample(&self, t: usize, col: usize) -> Option<i32> {
        let n = self.channels.enabled_count() as usize;
        if t >= self.samples_per_channel || col >= n {
            return None;
        }
        Some(self.samples[t * n + col])
    }

    /// Interleaved raw samples — intentionally crate-private: stages inside
    /// the pipeline may stream them; application code may not.
    pub(crate) fn raw_samples(&self) -> &'a [i32] {
        self.samples
    }

    /// Deterministic FNV-1a 64 integrity checksum over header and samples,
    /// all fields little-endian, in the order specified by
    /// `docs/PIPELINE_CONTRACT.md` §3.
    pub fn checksum(&self) -> u64 {
        let mut h = Fnv1a64::new();
        h.update(&self.seq.to_le_bytes());
        h.update(&self.timestamp_us.to_le_bytes());
        h.update(&self.rate.hz().to_le_bytes());
        h.update(&self.channels.bits().to_le_bytes());
        h.update(&(self.samples_per_channel as u32).to_le_bytes());
        for &s in self.samples {
            h.update(&s.to_le_bytes());
        }
        h.finish()
    }
}

impl fmt::Debug for RawFrame<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawFrame")
            .field("seq", &self.seq)
            .field("timestamp_us", &self.timestamp_us)
            .field("sample_rate_hz", &self.rate.hz())
            .field("channel_mask", &self.channels.bits())
            .field("samples_per_channel", &self.samples_per_channel)
            .field("samples", &"<redacted>")
            .finish()
    }
}
