//! Application-boundary marker: what may leave the pipeline.
//!
//! Privacy rule (`docs/PRIVACY_BOUNDARY.md`): raw signal types never cross
//! the application boundary. Only the pipeline-terminal
//! [`ClassifierDecision`] is boundary-safe; at system level the kernel
//! converts it into the canonical `IntentObservation` (RFC-0006 §4) under
//! consent gating.

use crate::decision::ClassifierDecision;

mod sealed {
    pub trait Sealed {}
}

/// Marker for types permitted to cross the application boundary.
///
/// Sealed: only [`ClassifierDecision`] implements it. `RawFrame`,
/// `Epoch`, and `FeatureVector` intentionally do not and cannot.
pub trait BoundarySafe: sealed::Sealed {}

impl sealed::Sealed for ClassifierDecision {}
impl BoundarySafe for ClassifierDecision {}

/// Compile-time boundary assertion.
///
/// ```
/// use axonos_pipeline_core::{boundary, ClassifierDecision};
/// boundary::assert_boundary_safe(&ClassifierDecision::NoIntent);
/// ```
///
/// Raw frames do not compile across the boundary:
///
/// ```compile_fail
/// use axonos_pipeline_core::{boundary, ChannelMask, RawFrame, SampleRate};
/// let samples = [0i32; 8];
/// let frame = RawFrame::new(0, 0, SampleRate::HZ_250, ChannelMask::first_n(8), &samples)
///     .unwrap();
/// boundary::assert_boundary_safe(&frame); // error: RawFrame is not BoundarySafe
/// ```
pub fn assert_boundary_safe<T: BoundarySafe>(_value: &T) {}
