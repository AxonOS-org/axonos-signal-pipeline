//! Deterministic calibration primitives (v0.6.0).
//!
//! Fixed-point, allocation-free building blocks for per-session adaptation:
//! channel **covariance**, a running **session mean** of covariances, an
//! exponential **drift update**, and reference **whitening** (Cholesky
//! `W = L⁻¹`, which maps the reference covariance `R` to the identity:
//! `W R Wᵀ = I`). [`ZeroCalib`] ties these into a zero-calibration *skeleton*.
//!
//! These are **defined deterministic transforms**, not a tuned or measured
//! calibration: there is no accuracy, transfer, or convergence claim
//! (`docs/CLAIMS.md`). Whitening is verified only in the algebraic sense
//! (`W R Wᵀ ≈ I`, pinned by vectors and asserted in tests); the symmetric
//! `R^{-1/2}` form of Euclidean Alignment is a documented future refinement
//! (`docs/CALIBRATION.md`). Matrices are `Cᗉ` arrays of `i64`; whitening
//! results are `Q16` fixed point. Inputs must be modestly scaled to stay within
//! `i64` (`docs/PIPELINE_CONTRACT.md` §12).

use crate::error::PipelineError;

/// Fractional bits of the fixed-point whitening matrices (`Q16`).
pub const WHITEN_SHIFT: u32 = 16;
const WHITEN_ONE: i64 = 1 << WHITEN_SHIFT;

/// Channel covariance matrix (mean-removed), `cov[i][j] = Σ(xᵢ−x̄ᵢ)(xⱼ−x̄ⱼ)/N`.
///
/// `channels` are `C` equal-length, non-empty per-channel buffers. Accumulation
/// is in `i128`; the stored result is raw integer (`Q0`).
///
/// # Errors
///
/// - [`PipelineError::DimensionMismatch`] if `channels.len() != C`.
/// - [`PipelineError::EmptyInput`] if any channel is empty.
/// - [`PipelineError::SampleLengthMismatch`] if channels differ in length.
#[allow(clippy::needless_range_loop)]
pub fn covariance<const C: usize>(channels: &[&[i32]]) -> Result<[[i64; C]; C], PipelineError> {
    if channels.len() != C {
        return Err(PipelineError::DimensionMismatch);
    }
    if C == 0 || channels[0].is_empty() {
        return Err(PipelineError::EmptyInput);
    }
    let n = channels[0].len();
    let mut means = [0i64; C];
    for (c, ch) in channels.iter().enumerate() {
        if ch.len() != n {
            return Err(PipelineError::SampleLengthMismatch);
        }
        let mut s = 0i64;
        for &x in ch.iter() {
            s += x as i64;
        }
        means[c] = s / n as i64;
    }
    let mut out = [[0i64; C]; C];
    for i in 0..C {
        for j in i..C {
            let mut acc: i128 = 0;
            for t in 0..n {
                let di = channels[i][t] as i64 - means[i];
                let dj = channels[j][t] as i64 - means[j];
                acc += di as i128 * dj as i128;
            }
            let v = (acc / n as i128) as i64;
            out[i][j] = v;
            out[j][i] = v;
        }
    }
    Ok(out)
}

/// Running mean of covariance matrices (a session reference).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionMean<const C: usize> {
    sum: [[i64; C]; C],
    count: u32,
}

impl<const C: usize> SessionMean<C> {
    /// Empty accumulator.
    pub const fn new() -> Self {
        Self {
            sum: [[0i64; C]; C],
            count: 0,
        }
    }

    /// Folds one covariance matrix into the accumulator.
    #[allow(clippy::needless_range_loop)]
    pub fn add(&mut self, cov: &[[i64; C]; C]) {
        for i in 0..C {
            for j in 0..C {
                self.sum[i][j] += cov[i][j];
            }
        }
        self.count += 1;
    }

    /// Number of matrices accumulated.
    pub const fn count(&self) -> u32 {
        self.count
    }

    /// Elementwise mean, or `None` if nothing has been added.
    #[allow(clippy::needless_range_loop)]
    pub fn mean(&self) -> Option<[[i64; C]; C]> {
        if self.count == 0 {
            return None;
        }
        let mut out = [[0i64; C]; C];
        for i in 0..C {
            for j in 0..C {
                out[i][j] = self.sum[i][j] / self.count as i64;
            }
        }
        Some(out)
    }
}

impl<const C: usize> Default for SessionMean<C> {
    fn default() -> Self {
        Self::new()
    }
}

/// In-place exponential drift update of a `reference` toward `new`:
/// `reference ← reference + α·(new − reference)`, with `α` a `Q15` weight in
/// `0 ≤ α ≤ 1`.
///
/// # Errors
///
/// [`PipelineError::InvalidCoefficient`] unless `0 ≤ alpha_q15 ≤ 32768`.
#[allow(clippy::needless_range_loop)]
pub fn drift_update<const C: usize>(
    reference: &mut [[i64; C]; C],
    new: &[[i64; C]; C],
    alpha_q15: i32,
) -> Result<(), PipelineError> {
    if !(0..=(1 << 15)).contains(&alpha_q15) {
        return Err(PipelineError::InvalidCoefficient);
    }
    let a = alpha_q15 as i64;
    for i in 0..C {
        for j in 0..C {
            let delta = new[i][j] - reference[i][j];
            reference[i][j] += (a * delta) >> 15;
        }
    }
    Ok(())
}

/// Floor integer square root over `u128` (no floating point, no overflow).
fn isqrt_u128(x: u128) -> u128 {
    if x == 0 {
        return 0;
    }
    let mut bit: u128 = 1 << 126;
    while bit > x {
        bit >>= 2;
    }
    let mut res: u128 = 0;
    let mut n = x;
    while bit != 0 {
        if n >= res + bit {
            n -= res + bit;
            res = (res >> 1) + bit;
        } else {
            res >>= 1;
        }
        bit >>= 2;
    }
    res
}

/// `Q16` square root of a non-negative `Q16` value.
#[inline]
fn sqrt_q16(x_q16: i64) -> i64 {
    if x_q16 <= 0 {
        return 0;
    }
    isqrt_u128((x_q16 as u128) << WHITEN_SHIFT) as i64
}

/// Reference whitening via fixed-point Cholesky: returns `W = L⁻¹` in `Q16`
/// such that `W R Wᵀ ≈ I`, or `None` if `r` is not positive-definite.
///
/// `r` is a symmetric covariance matrix (`Q0`); entries must be modest enough
/// that `r[i][j] << 16` fits `i64`. The result whitens the reference to the
/// identity; verify algebraically with [`align`].
#[allow(clippy::needless_range_loop)]
pub fn whiten_cholesky<const C: usize>(r: &[[i64; C]; C]) -> Option<[[i64; C]; C]> {
    // Promote R to Q16.
    let mut a = [[0i64; C]; C];
    for i in 0..C {
        for j in 0..C {
            a[i][j] = r[i][j] << WHITEN_SHIFT;
        }
    }
    // Lower-triangular Cholesky factor L (Q16): A = L Lᵀ.
    let mut l = [[0i64; C]; C];
    for j in 0..C {
        let mut diag = a[j][j];
        for k in 0..j {
            diag -= (l[j][k] * l[j][k]) >> WHITEN_SHIFT;
        }
        if diag <= 0 {
            return None; // not positive-definite
        }
        let ljj = sqrt_q16(diag);
        if ljj == 0 {
            return None;
        }
        l[j][j] = ljj;
        for i in (j + 1)..C {
            let mut s = a[i][j];
            for k in 0..j {
                s -= (l[i][k] * l[j][k]) >> WHITEN_SHIFT;
            }
            l[i][j] = (s << WHITEN_SHIFT) / ljj;
        }
    }
    // W = L⁻¹ by forward substitution on L W = I (Q16).
    let mut w = [[0i64; C]; C];
    for col in 0..C {
        for i in 0..C {
            let mut rhs = if i == col { WHITEN_ONE } else { 0 };
            for k in 0..i {
                rhs -= (l[i][k] * w[k][col]) >> WHITEN_SHIFT;
            }
            if l[i][i] == 0 {
                return None;
            }
            w[i][col] = (rhs << WHITEN_SHIFT) / l[i][i];
        }
    }
    Some(w)
}

/// Applies a `Q16` whitener `w` to a covariance `cov` (`Q0`): returns
/// `W cov Wᵀ` in `Q16`. Aligning the reference with its own whitener yields the
/// `Q16` identity.
#[allow(clippy::needless_range_loop)]
pub fn align<const C: usize>(w: &[[i64; C]; C], cov: &[[i64; C]; C]) -> [[i64; C]; C] {
    // tmp = W · cov  (Q16 · Q0 → Q16)
    let mut tmp = [[0i64; C]; C];
    for i in 0..C {
        for j in 0..C {
            let mut s = 0i64;
            for k in 0..C {
                s += w[i][k] * cov[k][j];
            }
            tmp[i][j] = s;
        }
    }
    // out = tmp · Wᵀ  (Q16 · Q16 → Q32 → Q16)
    let mut out = [[0i64; C]; C];
    for i in 0..C {
        for j in 0..C {
            let mut s = 0i64;
            for k in 0..C {
                s += (tmp[i][k] * w[j][k]) >> WHITEN_SHIFT;
            }
            out[i][j] = s;
        }
    }
    out
}

/// Zero-calibration *skeleton*: accumulate session covariances, then finalize a
/// reference whitener. Structural only — there is no online adaptation or
/// transfer claim (`docs/CALIBRATION.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZeroCalib<const C: usize> {
    mean: SessionMean<C>,
}

impl<const C: usize> ZeroCalib<C> {
    /// New, empty calibrator.
    pub const fn new() -> Self {
        Self {
            mean: SessionMean::new(),
        }
    }

    /// Observes one epoch covariance.
    pub fn observe(&mut self, cov: &[[i64; C]; C]) {
        self.mean.add(cov);
    }

    /// Number of observed covariances.
    pub const fn count(&self) -> u32 {
        self.mean.count()
    }

    /// Finalizes the `Q16` reference whitener, or `None` if no covariance was
    /// observed or the mean is not positive-definite.
    pub fn whitener(&self) -> Option<[[i64; C]; C]> {
        let r = self.mean.mean()?;
        whiten_cholesky(&r)
    }
}

impl<const C: usize> Default for ZeroCalib<C> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // |V[i][j] - target| tolerance in Q16 (~1.5%).
    const TOL: i64 = 1000;

    #[allow(clippy::needless_range_loop)]
    fn assert_identity_q16<const C: usize>(v: &[[i64; C]; C]) {
        for i in 0..C {
            for j in 0..C {
                let target = if i == j { WHITEN_ONE } else { 0 };
                assert!(
                    (v[i][j] - target).abs() < TOL,
                    "V[{i}][{j}]={} not ≈ {target}",
                    v[i][j]
                );
            }
        }
    }

    #[test]
    fn cholesky_whitens_2x2() {
        let r = [[4i64, 1], [1, 3]];
        let w = whiten_cholesky(&r).expect("PD");
        assert_identity_q16(&align(&w, &r));
    }

    #[test]
    fn cholesky_whitens_3x3() {
        // Diagonally dominant SPD.
        let r = [[6i64, 2, 1], [2, 5, 2], [1, 2, 7]];
        let w = whiten_cholesky(&r).expect("PD");
        assert_identity_q16(&align(&w, &r));
    }

    #[test]
    fn non_pd_rejected() {
        let r = [[1i64, 2], [2, 1]]; // indefinite
        assert!(whiten_cholesky(&r).is_none());
    }

    #[test]
    fn covariance_basic_and_session() {
        // ch0 = [-2,-1,1,2] var=2.5→2 ; ch1 = ch0 → cov diag equal, off = same
        let ch0 = [-2i32, -1, 1, 2];
        let ch1 = [-2i32, -1, 1, 2];
        let cov = covariance::<2>(&[&ch0[..], &ch1[..]]).unwrap();
        assert_eq!(cov[0][0], cov[1][1]);
        assert_eq!(cov[0][1], cov[0][0]); // perfectly correlated
        let mut sm = SessionMean::<2>::new();
        sm.add(&cov);
        sm.add(&cov);
        assert_eq!(sm.mean().unwrap(), cov);
        assert_eq!(SessionMean::<2>::new().mean(), None);
    }

    #[test]
    fn drift_moves_toward_target() {
        let mut r = [[100i64, 0], [0, 100]];
        let new = [[200i64, 0], [0, 200]];
        drift_update(&mut r, &new, 1 << 14).unwrap(); // α=0.5
        assert_eq!(r[0][0], 150);
        assert_eq!(
            drift_update(&mut r, &new, 40_000),
            Err(PipelineError::InvalidCoefficient)
        );
    }

    #[test]
    fn zerocalib_skeleton_flow() {
        let r = [[4i64, 1], [1, 3]];
        let mut zc = ZeroCalib::<2>::new();
        assert!(zc.whitener().is_none());
        zc.observe(&r);
        zc.observe(&r);
        assert_eq!(zc.count(), 2);
        let w = zc.whitener().expect("PD");
        assert_identity_q16(&align(&w, &r));
    }
}
