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
