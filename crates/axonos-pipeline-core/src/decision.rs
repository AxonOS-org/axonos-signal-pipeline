//! Pipeline-terminal classifier decision.

use crate::error::PipelineError;

/// Permille denominator for confidence values.
pub const CONFIDENCE_MAX_PERMILLE: u16 = 1000;

/// Outcome of the reference classifier for one epoch.
///
/// This is the **pipeline-terminal** type. Conversion into the canonical
/// `IntentObservation` wire type (RFC-0006 §4, crate `axonos-intent` in
/// `AxonOS-org/axonos-kernel`) happens at the kernel boundary under
/// consent gating (`axonos-consent`); this crate intentionally does not
/// redefine that type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassifierDecision {
    /// No intent detected, or the classifier abstained.
    NoIntent,
    /// A detected intent class with its confidence.
    Intent {
        /// Application-defined intent class identifier.
        class: u8,
        /// Confidence in permille, `0..=1000`.
        confidence_permille: u16,
    },
}

impl ClassifierDecision {
    /// Validated constructor for an intent decision.
    pub fn intent(class: u8, confidence_permille: u16) -> Result<Self, PipelineError> {
        if confidence_permille > CONFIDENCE_MAX_PERMILLE {
            return Err(PipelineError::InvalidConfidence);
        }
        Ok(Self::Intent {
            class,
            confidence_permille,
        })
    }

    /// True for [`ClassifierDecision::NoIntent`].
    pub fn is_abstain(&self) -> bool {
        matches!(self, Self::NoIntent)
    }
}
