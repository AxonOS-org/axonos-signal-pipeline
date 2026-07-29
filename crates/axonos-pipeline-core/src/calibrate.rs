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

/// Fractional bits used *inside* the inverse-square-root iteration.
///
/// Higher than [`WHITEN_SHIFT`] on purpose. The iteration squares its operands
/// repeatedly, so precision lost early is amplified; at `Q16` the result
/// deviates from the target by ~8·10⁻³ relative, at `Q24` by ~2·10⁻⁵. The
/// result is returned at `WHITEN_SHIFT` so it is interchangeable with
/// [`whiten_cholesky`] and consumable by [`align`].
pub const ALIGN_INTERNAL_SHIFT: u32 = 24;

/// Newton–Schulz iterations performed. Fixed, never data-dependent.
///
/// The count is a constant rather than a convergence test because a loop that
/// stops when it is satisfied has an execution time that depends on its input,
/// and this crate exists inside a system that must state a worst case. The
/// iteration converges quadratically and plateaus by ten on the conditioning
/// this shrinkage admits; fourteen is that with margin.
pub const ALIGN_ITERATIONS: u32 = 14;

/// Default diagonal shrinkage, in parts per million of the mean eigenvalue.
///
/// `R' = R + εI` with `ε = shrinkage · tr(R) / (10⁶ · C)`. Two things depend on
/// it, in opposite directions: convergence needs the smallest eigenvalue bounded
/// away from zero, and fidelity to the *unregularised* `R` wants ε as small as
/// possible. Measured cost at `C = 8` and condition number 100:
///
/// | shrinkage | `max‖W R Wᵀ − I‖` |
/// |----------:|------------------:|
/// |     0.2 % |             0.043 |
/// |     1.0 % |             0.159 |
/// |     5.0 % |             0.353 |
///
/// The default is deliberately small; a caller working with ill-conditioned
/// covariance should raise it and accept the cost knowingly.
pub const DEFAULT_SHRINKAGE_PPM: u32 = 2_000;

/// Symmetric inverse square root `R^{-1/2}` of a positive-definite covariance,
/// in `Q16` fixed point.
///
/// This is the whitener Euclidean Alignment specifies, and the one
/// [`whiten_cholesky`] is not. Both satisfy `W R Wᵀ = I`; they differ in that
/// the Cholesky factor is triangular and this one is symmetric. Every whitener
/// satisfying `W R Wᵀ = I` equals `R^{-1/2}` up to a left orthogonal factor, so
/// the choice determines *which* frame the whitened data lands in — see
/// `docs/CALIBRATION.md` for what that does and, importantly, does not settle.
///
/// # Method
///
/// Newton–Schulz, which needs only matrix multiplication — no eigendecomposition,
/// no square root of a matrix, no division inside the loop:
///
/// ```text
/// A  = (R + εI) / tr(R + εI)          so that every eigenvalue lies in (0, 1]
/// Y₀ = A,  Z₀ = I
/// T    = (3I − ZY) / 2
/// Y    ← Y T,   Z ← T Z               Y → A^{1/2},  Z → A^{-1/2}
/// R^{-1/2} = Z / √tr(R + εI)
/// ```
///
/// Multiplication-only matters here for more than speed: it is what makes the
/// routine expressible in exact integer arithmetic, so two implementations
/// agree bit for bit rather than approximately.
///
/// # Guarantees
///
/// Deterministic, allocation-free, and constant-time in the sense that matters
/// for a real-time budget: the iteration count does not depend on the data.
/// The result is symmetric to within fixed-point rounding.
///
/// # What is not claimed
///
/// That this improves classification, transfers across subjects or sessions, or
/// converges to anything about a person. It is a defined transform, verified
/// algebraically. See `docs/CLAIMS.md`.
///
/// Returns `None` when `R` has a non-positive trace — the case where no
/// covariance was observed, or the input is not a covariance at all.
pub fn inverse_sqrt_spd<const C: usize>(
    r: &[[i64; C]; C],
    shrinkage_ppm: u32,
    iterations: u32,
) -> Option<[[i64; C]; C]> {
    const fn one(shift: u32) -> i128 {
        1i128 << shift
    }
    let q = ALIGN_INTERNAL_SHIFT;
    let unit = one(q);

    let mut trace: i128 = 0;
    for i in 0..C {
        trace += r[i][i] as i128;
    }
    if trace <= 0 {
        return None;
    }
    // ε as a fraction of the mean eigenvalue, so shrinkage means the same thing
    // whatever the input is scaled by.
    let eps = (trace * shrinkage_ppm as i128) / (1_000_000i128 * C as i128);

    let mut reg = [[0i128; C]; C];
    let mut s: i128 = 0;
    for i in 0..C {
        for j in 0..C {
            reg[i][j] = r[i][j] as i128 + if i == j { eps } else { 0 };
        }
        s += reg[i][i];
    }
    if s <= 0 {
        return None;
    }

    // A = reg / s, in Q(ALIGN_INTERNAL_SHIFT). Every eigenvalue of a positive
    // semi-definite matrix is at most its trace, so this places the spectrum in
    // (0, 1] and the iteration is in its convergent region.
    let mut y = [[0i128; C]; C];
    for i in 0..C {
        for j in 0..C {
            y[i][j] = (reg[i][j] << q) / s;
        }
    }
    let mut z = [[0i128; C]; C];
    for (i, row) in z.iter_mut().enumerate() {
        row[i] = unit;
    }

    let mul = |a: &[[i128; C]; C], b: &[[i128; C]; C]| -> [[i128; C]; C] {
        let mut out = [[0i128; C]; C];
        for i in 0..C {
            for j in 0..C {
                let mut acc: i128 = 0;
                for k in 0..C {
                    acc += a[i][k] * b[k][j];
                }
                out[i][j] = acc >> q;
            }
        }
        out
    };

    for _ in 0..iterations {
        let zy = mul(&z, &y);
        let mut tm = [[0i128; C]; C];
        for i in 0..C {
            for j in 0..C {
                let three_i = if i == j { 3 * unit } else { 0 };
                tm[i][j] = (three_i - zy[i][j]) / 2;
            }
        }
        y = mul(&y, &tm);
        z = mul(&tm, &z);
    }

    // R^{-1/2} = Z / √s. Computed as Z · (1/√s) with the reciprocal formed at
    // 2q fractional bits so the division does not eat the result's precision.
    let root = isqrt_i128(s << (2 * q));
    if root <= 0 {
        return None;
    }
    let inv_root = (1i128 << (2 * q)) / root; // 1/√s at Q(q)

    let mut out = [[0i64; C]; C];
    let down = q - WHITEN_SHIFT;
    for i in 0..C {
        for j in 0..C {
            let v = (z[i][j] * inv_root) >> (q + down);
            out[i][j] = v as i64;
        }
    }
    Some(out)
}

/// Integer square root by Newton's method. Deterministic and terminating: the
/// sequence is strictly decreasing until it reaches the floor of the root.
fn isqrt_i128(x: i128) -> i128 {
    if x <= 0 {
        return 0;
    }
    let mut r = x;
    let mut y = (r + 1) / 2;
    while y < r {
        r = y;
        y = (r + x / r) / 2;
    }
    r
}

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

    /// Finalizes the `Q16` reference whitener by Cholesky, or `None` if no
    /// covariance was observed or the mean is not positive-definite.
    ///
    /// Retained because it is cheaper and its output is pinned by existing
    /// vectors. For alignment use [`ZeroCalib::aligner`], which produces the
    /// symmetric whitener Euclidean Alignment specifies.
    pub fn whitener(&self) -> Option<[[i64; C]; C]> {
        let r = self.mean.mean()?;
        whiten_cholesky(&r)
    }

    /// Finalizes the symmetric `Q16` aligner `R̄^{-1/2}` over the observed
    /// covariances, or `None` if none were observed.
    ///
    /// This closes the gap the module has documented since v0.6.0: the
    /// zero-calibration path needs the symmetric root, not the Cholesky factor,
    /// because every whitener satisfying `W R Wᵀ = I` differs from `R^{-1/2}`
    /// by a left orthogonal factor and that factor decides which frame the data
    /// lands in.
    ///
    /// Observations are **unlabelled**: this is what "no calibration session"
    /// means here, and it is a narrower statement than "no data". A short
    /// stretch of ordinary use is still required before `count()` is enough to
    /// estimate `R̄`.
    pub fn aligner(&self, shrinkage_ppm: u32) -> Option<[[i64; C]; C]> {
        let r = self.mean.mean()?;
        inverse_sqrt_spd(&r, shrinkage_ppm, ALIGN_ITERATIONS)
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

#[cfg(test)]
mod inverse_sqrt_tests {
    use super::*;

    const Q: i64 = 1 << WHITEN_SHIFT;

    /// `W R Wᵀ`, in Q0, for checking against the identity.
    fn round_trip<const C: usize>(w: &[[i64; C]; C], r: &[[i64; C]; C]) -> [[f64; C]; C] {
        let wf = |x: i64| x as f64 / Q as f64;
        let mut out = [[0.0f64; C]; C];
        for i in 0..C {
            for j in 0..C {
                let mut s = 0.0;
                for k in 0..C {
                    for l in 0..C {
                        s += wf(w[i][k]) * r[k][l] as f64 * wf(w[j][l]);
                    }
                }
                out[i][j] = s;
            }
        }
        out
    }

    fn max_dev_from_identity<const C: usize>(m: &[[f64; C]; C]) -> f64 {
        let mut worst = 0.0f64;
        for i in 0..C {
            for j in 0..C {
                let target = if i == j { 1.0 } else { 0.0 };
                worst = worst.max((m[i][j] - target).abs());
            }
        }
        worst
    }

    #[test]
    fn the_identity_is_its_own_inverse_square_root() {
        let r = [[1000i64, 0], [0, 1000]];
        let w = inverse_sqrt_spd(&r, 0, ALIGN_ITERATIONS).unwrap();
        // R^{-1/2} = 1/sqrt(1000) ≈ 0.031623 → Q16 ≈ 2072
        assert!((w[0][0] - 2072).abs() <= 2, "got {}", w[0][0]);
        assert_eq!(w[0][1], 0);
        assert_eq!(w[0][0], w[1][1]);
    }

    #[test]
    fn a_diagonal_matrix_gives_element_wise_inverse_roots() {
        let r = [[400i64, 0, 0], [0, 900, 0], [0, 0, 2500]];
        let w = inverse_sqrt_spd(&r, 0, ALIGN_ITERATIONS).unwrap();
        for (i, expect) in [1.0 / 20.0, 1.0 / 30.0, 1.0 / 50.0].iter().enumerate() {
            let got = w[i][i] as f64 / Q as f64;
            assert!((got - expect).abs() < 2e-3, "row {i}: {got} vs {expect}");
        }
    }

    #[test]
    fn the_result_is_symmetric_which_the_cholesky_factor_is_not() {
        let r = [[2500i64, 900, 300], [900, 1600, 400], [300, 400, 1200]];
        let sym = inverse_sqrt_spd(&r, DEFAULT_SHRINKAGE_PPM, ALIGN_ITERATIONS).unwrap();
        for i in 0..3 {
            for j in 0..3 {
                assert!(
                    (sym[i][j] - sym[j][i]).abs() <= 2,
                    "asymmetric at {i},{j}: {} vs {}",
                    sym[i][j],
                    sym[j][i]
                );
            }
        }
        let chol = whiten_cholesky(&r).unwrap();
        let triangular = chol[0][1] == 0 && chol[0][2] == 0 && chol[1][2] == 0;
        assert!(
            triangular,
            "the Cholesky whitener is triangular, and so not this"
        );
    }

    #[test]
    fn it_whitens_what_it_was_given() {
        let r = [[2500i64, 900, 300], [900, 1600, 400], [300, 400, 1200]];
        let w = inverse_sqrt_spd(&r, DEFAULT_SHRINKAGE_PPM, ALIGN_ITERATIONS).unwrap();
        let dev = max_dev_from_identity(&round_trip(&w, &r));
        assert!(dev < 0.05, "W R Wᵀ deviates from I by {dev}");
    }

    #[test]
    fn shrinkage_trades_fidelity_for_conditioning_in_the_documented_direction() {
        let r = [[10_000i64, 9_800, 0], [9_800, 10_000, 0], [0, 0, 40]];
        let tight = inverse_sqrt_spd(&r, 200, ALIGN_ITERATIONS).unwrap();
        let loose = inverse_sqrt_spd(&r, 100_000, ALIGN_ITERATIONS).unwrap();
        let d_tight = max_dev_from_identity(&round_trip(&tight, &r));
        let d_loose = max_dev_from_identity(&round_trip(&loose, &r));
        assert!(
            d_tight < d_loose,
            "less shrinkage must whiten the original matrix more closely: {d_tight} vs {d_loose}"
        );
    }

    #[test]
    fn the_iteration_count_does_not_depend_on_the_data() {
        // The loop is fixed, so a WCET can be stated. Two very differently
        // conditioned inputs must both be accepted at the same count.
        let easy = [[1000i64, 0], [0, 1000]];
        let hard = [[10_000i64, 9_900], [9_900, 10_000]];
        assert!(inverse_sqrt_spd(&easy, DEFAULT_SHRINKAGE_PPM, ALIGN_ITERATIONS).is_some());
        assert!(inverse_sqrt_spd(&hard, DEFAULT_SHRINKAGE_PPM, ALIGN_ITERATIONS).is_some());
    }

    #[test]
    fn more_iterations_do_not_move_a_converged_result() {
        let r = [[2500i64, 900], [900, 1600]];
        let a = inverse_sqrt_spd(&r, DEFAULT_SHRINKAGE_PPM, ALIGN_ITERATIONS).unwrap();
        let b = inverse_sqrt_spd(&r, DEFAULT_SHRINKAGE_PPM, ALIGN_ITERATIONS + 8).unwrap();
        for i in 0..2 {
            for j in 0..2 {
                assert!((a[i][j] - b[i][j]).abs() <= 1, "not converged at {i},{j}");
            }
        }
    }

    #[test]
    fn it_is_bit_for_bit_deterministic() {
        let r = [[2500i64, 900, 300], [900, 1600, 400], [300, 400, 1200]];
        let a = inverse_sqrt_spd(&r, DEFAULT_SHRINKAGE_PPM, ALIGN_ITERATIONS);
        let b = inverse_sqrt_spd(&r, DEFAULT_SHRINKAGE_PPM, ALIGN_ITERATIONS);
        assert_eq!(a, b);
    }

    #[test]
    fn a_degenerate_input_is_refused_rather_than_guessed() {
        let zero = [[0i64; 3]; 3];
        assert!(inverse_sqrt_spd(&zero, DEFAULT_SHRINKAGE_PPM, ALIGN_ITERATIONS).is_none());
        let negative = [[-100i64, 0], [0, -100]];
        assert!(inverse_sqrt_spd(&negative, DEFAULT_SHRINKAGE_PPM, ALIGN_ITERATIONS).is_none());
    }

    #[test]
    fn zerocalib_produces_an_aligner_from_unlabelled_observations() {
        let mut zc = ZeroCalib::<3>::new();
        assert!(
            zc.aligner(DEFAULT_SHRINKAGE_PPM).is_none(),
            "nothing observed yet"
        );
        for scale in [1i64, 2, 3, 4] {
            let cov = [
                [2500 * scale, 900 * scale, 300 * scale],
                [900 * scale, 1600 * scale, 400 * scale],
                [300 * scale, 400 * scale, 1200 * scale],
            ];
            zc.observe(&cov);
        }
        assert_eq!(zc.count(), 4);
        let w = zc.aligner(DEFAULT_SHRINKAGE_PPM).unwrap();
        for i in 0..3 {
            for j in 0..3 {
                assert!((w[i][j] - w[j][i]).abs() <= 2, "aligner must be symmetric");
            }
        }
    }

    #[test]
    fn the_aligner_and_the_cholesky_whitener_both_whiten_and_still_differ() {
        // Both satisfy W R Wᵀ = I; they are not the same matrix, and the
        // difference is the residual orthogonal factor documented in
        // docs/CALIBRATION.md.
        let r = [[2500i64, 900, 300], [900, 1600, 400], [300, 400, 1200]];
        let sym = inverse_sqrt_spd(&r, 200, ALIGN_ITERATIONS).unwrap();
        let chol = whiten_cholesky(&r).unwrap();
        assert!(max_dev_from_identity(&round_trip(&sym, &r)) < 0.05);
        assert!(max_dev_from_identity(&round_trip(&chol, &r)) < 0.05);
        let mut differs = false;
        for i in 0..3 {
            for j in 0..3 {
                if (sym[i][j] - chol[i][j]).abs() > 16 {
                    differs = true;
                }
            }
        }
        assert!(differs, "the two whiteners must not coincide");
    }
}
