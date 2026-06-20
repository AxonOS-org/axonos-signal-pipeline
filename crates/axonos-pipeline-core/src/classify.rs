//! Deterministic classifier inference (v0.5.0).
//!
//! **Inference machinery only.** These functions evaluate fixed decision rules
//! (minimum-distance-to-mean and a linear/LDA score) over a feature vector,
//! given model parameters **supplied by the caller**. This crate ships **no
//! trained model**, no learned weights, and makes **no accuracy claim**: the
//! parameters used in tests and vectors are illustrative, and the only property
//! asserted is determinism — identical parameters and input always yield the
//! same [`ClassifierDecision`] (`docs/CLAIMS.md`, `docs/VALIDATION_PLAN.md`).
//!
//! All arithmetic is integer (`i128` accumulation, saturation), so results are
//! bit-exact across platforms (`docs/PIPELINE_CONTRACT.md` §11).

use crate::decision::{ClassifierDecision, CONFIDENCE_MAX_PERMILLE};
use crate::error::PipelineError;

/// Squared Euclidean distance between a feature vector and a class mean.
///
/// # Errors
///
/// [`PipelineError::DimensionMismatch`] if the lengths differ.
pub fn distance_sq(feature: &[i32], mean: &[i32]) -> Result<u64, PipelineError> {
    if feature.len() != mean.len() {
        return Err(PipelineError::DimensionMismatch);
    }
    let mut acc: u128 = 0;
    for (&f, &m) in feature.iter().zip(mean) {
        let d = f as i64 - m as i64;
        acc += (d as i128 * d as i128) as u128;
    }
    Ok(acc.min(u64::MAX as u128) as u64)
}

/// Minimum-distance-to-mean decision over `class_means`.
///
/// Returns the nearest class as a [`ClassifierDecision`]. Confidence is the
/// margin between the nearest and second-nearest squared distances,
/// `1000·(d₂ − d₁)/(d₂ + d₁)` in permille; a tie yields confidence `0`. If the
/// resulting confidence is below `abstain_below_permille`, the decision is
/// [`ClassifierDecision::NoIntent`] (abstain). Class indices must fit `u8`
/// (≤ 256 classes).
///
/// # Errors
///
/// - [`PipelineError::EmptyClassSet`] if `class_means` is empty.
/// - [`PipelineError::DimensionMismatch`] if any mean length differs from
///   `feature`.
/// - [`PipelineError::InvalidConfidence`] is not returned here (confidence is
///   constructed in range).
pub fn classify_mdm(
    feature: &[i32],
    class_means: &[&[i32]],
    abstain_below_permille: u16,
) -> Result<ClassifierDecision, PipelineError> {
    if class_means.is_empty() {
        return Err(PipelineError::EmptyClassSet);
    }
    let mut best: usize = 0;
    let mut best_d: u64 = u64::MAX;
    let mut second_d: u64 = u64::MAX;
    for (i, &m) in class_means.iter().enumerate() {
        let d = distance_sq(feature, m)?;
        if d < best_d {
            second_d = best_d;
            best_d = d;
            best = i;
        } else if d < second_d {
            second_d = d;
        }
    }
    let conf: u16 = if best_d == 0 && second_d == 0 {
        0
    } else {
        let num = (second_d - best_d) as u128 * CONFIDENCE_MAX_PERMILLE as u128;
        let den = (second_d as u128) + (best_d as u128);
        (num / den) as u16
    };
    if conf < abstain_below_permille {
        Ok(ClassifierDecision::NoIntent)
    } else {
        ClassifierDecision::intent(best as u8, conf)
    }
}

/// Linear discriminant score `bias + Σ weights·feature`, saturated to `i64`.
///
/// This is the inference half of an LDA / linear classifier; the projection
/// `weights` and `bias` are caller-supplied model parameters.
///
/// # Errors
///
/// [`PipelineError::DimensionMismatch`] if `feature.len() != weights.len()`.
pub fn lda_score(feature: &[i32], weights: &[i32], bias: i64) -> Result<i64, PipelineError> {
    if feature.len() != weights.len() {
        return Err(PipelineError::DimensionMismatch);
    }
    let mut acc: i128 = bias as i128;
    for (&f, &w) in feature.iter().zip(weights) {
        acc += f as i128 * w as i128;
    }
    Ok(acc.clamp(i64::MIN as i128, i64::MAX as i128) as i64)
}

/// Two-class decision from a linear/LDA score with a dead-band abstain region.
///
/// Class `1` if the score is positive, class `0` if negative. If `|score|` is
/// below `abstain_band`, the decision is [`ClassifierDecision::NoIntent`].
/// Confidence is `1000·|score|/(|score| + band)` in permille, where `band` is
/// `max(abstain_band, 1)`.
///
/// # Errors
///
/// - [`PipelineError::DimensionMismatch`] (see [`lda_score`]).
/// - [`PipelineError::InvalidThreshold`] if `abstain_band` is negative.
pub fn classify_lda_binary(
    feature: &[i32],
    weights: &[i32],
    bias: i64,
    abstain_band: i64,
) -> Result<ClassifierDecision, PipelineError> {
    if abstain_band < 0 {
        return Err(PipelineError::InvalidThreshold);
    }
    let score = lda_score(feature, weights, bias)?;
    let a = score.unsigned_abs();
    if a < abstain_band as u64 {
        return Ok(ClassifierDecision::NoIntent);
    }
    let band = (abstain_band as u64).max(1);
    let conf = ((a as u128 * CONFIDENCE_MAX_PERMILLE as u128) / (a as u128 + band as u128)) as u16;
    let class: u8 = if score >= 0 { 1 } else { 0 };
    ClassifierDecision::intent(class, conf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mdm_picks_nearest_class() {
        let c0 = [0i32, 0];
        let c1 = [100i32, 100];
        let means = [&c0[..], &c1[..]];
        // feature near c1
        let d = classify_mdm(&[90, 95], &means, 0).unwrap();
        assert_eq!(
            d,
            ClassifierDecision::Intent {
                class: 1,
                confidence_permille: match d {
                    ClassifierDecision::Intent {
                        confidence_permille,
                        ..
                    } => confidence_permille,
                    _ => 0,
                }
            }
        );
        if let ClassifierDecision::Intent { class, .. } = d {
            assert_eq!(class, 1);
        }
    }

    #[test]
    fn mdm_abstains_on_tie() {
        let c0 = [0i32, 0];
        let c1 = [100i32, 0];
        let means = [&c0[..], &c1[..]];
        // exactly equidistant → conf 0 → abstain when threshold > 0
        let d = classify_mdm(&[50, 0], &means, 1).unwrap();
        assert_eq!(d, ClassifierDecision::NoIntent);
    }

    #[test]
    fn mdm_errors() {
        assert_eq!(
            classify_mdm(&[1, 2], &[], 0),
            Err(PipelineError::EmptyClassSet)
        );
        let bad = [1i32];
        assert_eq!(
            classify_mdm(&[1, 2], &[&bad[..]], 0),
            Err(PipelineError::DimensionMismatch)
        );
    }

    #[test]
    fn lda_score_and_decision() {
        // score = 0*? ; weights [2,-1], feature [10,5], bias 0 → 20-5 = 15
        assert_eq!(lda_score(&[10, 5], &[2, -1], 0).unwrap(), 15);
        let d = classify_lda_binary(&[10, 5], &[2, -1], 0, 5).unwrap();
        assert!(matches!(d, ClassifierDecision::Intent { class: 1, .. }));
        // within dead-band → abstain
        let z = classify_lda_binary(&[1, 1], &[1, -1], 0, 5).unwrap();
        assert_eq!(z, ClassifierDecision::NoIntent);
        assert_eq!(
            classify_lda_binary(&[1], &[1], 0, -1),
            Err(PipelineError::InvalidThreshold)
        );
    }
}
