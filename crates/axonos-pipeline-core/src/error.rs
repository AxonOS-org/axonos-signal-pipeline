//! Pipeline error type.

use core::fmt;

/// Errors produced by pipeline-core constructors and stage functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PipelineError {
    /// Channel mask has no enabled channels.
    EmptyChannelMask,
    /// Sample buffer is empty or not a multiple of the enabled channel count.
    SampleLengthMismatch,
    /// Window or hop length of zero.
    InvalidWindow,
    /// Window longer than the samples available per channel.
    WindowTooLarge,
    /// Artifact threshold must be a positive number of ADC counts.
    InvalidThreshold,
    /// Confidence exceeds 1000 permille.
    InvalidConfidence,
    /// A DSP input buffer is empty.
    EmptyInput,
    /// A filter kernel has no coefficients.
    EmptyKernel,
    /// An output buffer length does not match the input length.
    OutputLengthMismatch,
    /// A fixed-point shift amount is out of range (must be `0..=31`).
    InvalidShift,
    /// A sample rate has no tabulated filter design.
    UnsupportedSampleRate,
    /// A fixed-point filter coefficient is outside its valid range.
    InvalidCoefficient,
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::EmptyChannelMask => "channel mask has no enabled channels",
            Self::SampleLengthMismatch => {
                "sample length is zero or not a multiple of enabled channel count"
            }
            Self::InvalidWindow => "window and hop must be non-zero",
            Self::WindowTooLarge => "window exceeds samples per channel",
            Self::InvalidThreshold => "artifact threshold must be positive",
            Self::InvalidConfidence => "confidence exceeds 1000 permille",
            Self::EmptyInput => "DSP input buffer is empty",
            Self::EmptyKernel => "filter kernel has no coefficients",
            Self::OutputLengthMismatch => "output buffer length does not match input length",
            Self::InvalidShift => "fixed-point shift amount must be in 0..=31",
            Self::UnsupportedSampleRate => "sample rate has no tabulated filter design",
            Self::InvalidCoefficient => "fixed-point filter coefficient is out of range",
        };
        f.write_str(s)
    }
}
