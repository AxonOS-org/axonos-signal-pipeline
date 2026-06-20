//! # AxonOS Signal Pipeline — core type contract
//!
//! Typed stages of the AxonOS reference BCI signal pipeline:
//!
//! ```text
//! RawFrame -> Epoch -> DSP -> FeatureVector -> ClassifierDecision
//!                                                              |
//!                                  kernel boundary: canonical IntentObservation
//!                                  (RFC-0006 §4, crate `axonos-intent`),
//!                                  consent-gated by `axonos-consent`
//! ```
//!
//! Design rule (see `docs/PRIVACY_BOUNDARY.md`): **raw neural data never
//! crosses the application boundary.** Only the pipeline-terminal
//! [`ClassifierDecision`] is boundary-safe; this crate deliberately does
//! not redefine the canonical `IntentObservation` wire type.
//!
//! This crate is `no_std`, allocation-free, dependency-free, and forbids
//! `unsafe`. It is a pre-clinical engineering artifact, not a medical
//! device, and makes no measured-performance claim (`docs/CLAIMS.md`).
#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod artifact;
pub mod boundary;
pub mod calibrate;
pub mod checksum;
pub mod classify;
pub mod decision;
pub mod dsp;
pub mod epoch;
pub mod error;
pub mod feature;
pub mod filter;
pub mod frame;
pub mod mask;
pub mod rate;

pub use artifact::{artifact_scan, ArtifactFlag, ADC24_MAX, ADC24_MIN};
pub use calibrate::{
    align, covariance, drift_update, whiten_cholesky, SessionMean, ZeroCalib, WHITEN_SHIFT,
};
pub use checksum::{fnv1a_64, Fnv1a64, FNV_OFFSET_BASIS, FNV_PRIME};
pub use classify::{classify_lda_binary, classify_mdm, distance_sq, lda_score};
pub use decision::{ClassifierDecision, CONFIDENCE_MAX_PERMILLE};
pub use dsp::{fir, remove_mean};
pub use epoch::{window_count, Epoch, EpochIter};
pub use error::PipelineError;
pub use feature::{
    abs_mean, isqrt, log2_q16, log_variance_q16, rms, variance, zero_crossings, FeatureVector,
};
pub use filter::{
    bandpass_coeffs, notch_coeffs, BandpassPreset, Biquad, BiquadCoeffs, DcBlocker, NotchMode,
    BIQUAD_ONE, BIQUAD_SHIFT,
};
pub use frame::RawFrame;
pub use mask::ChannelMask;
pub use rate::SampleRate;
