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
        };
        f.write_str(s)
    }
}
