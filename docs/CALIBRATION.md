# Calibration (v0.6.0)

Calibration reduces inter-session covariate shift before classification. As of
v0.6.0 the **deterministic machinery** is implemented and vector-pinned in
`src/calibrate.rs`. As with the rest of the crate, these are **defined
transforms** with no accuracy, transfer, or convergence claim
([`CLAIMS.md`](CLAIMS.md)).

## Implemented (v0.6.0)

- **Channel covariance** (`covariance`). Mean-removed
  `cov[i][j] = Σ(xᵢ−x̄ᵢ)(xⱼ−x̄ⱼ)/N`, integer accumulation. Pinned by the
  `covariance` vectors.
- **Session mean** (`SessionMean`). Deterministic running mean of covariance
  matrices — the session reference statistic.
- **Drift update** (`drift_update`). In-place exponential update
  `reference ← reference + α·(new − reference)` with a `Q15` weight; rejects
  out-of-range α.
- **Reference whitening** (`whiten_cholesky`). Fixed-point Cholesky `W = L⁻¹`
  such that `W R Wᵀ = I` — it maps the reference covariance to the identity,
  which is the core of alignment. Returns `None` for non-positive-definite
  input. Verified **algebraically**: the `whiten_cholesky` vectors pin `W` and
  the exact `align(W, R)` result, which is the `Q16` identity to within
  fixed-point error.
- **ZeroCalib skeleton** (`ZeroCalib`). The typed flow that accumulates session
  covariances and finalizes a reference whitener. Structural only.

## Deliberately deferred

- **Symmetric `R^{-1/2}` Euclidean Alignment.** Whitening here uses the Cholesky
  factor `L⁻¹`, a valid whitener that differs from the symmetric `R^{-1/2}` form
  of EA by a rotation. The symmetric form (and the Riemannian-mean reference it
  is usually paired with) is the next refinement; it needs a fixed-point
  symmetric `R^{-1/2}` routine.
- **Online adaptation and any transfer/accuracy claim.** ZeroCalib is a
  skeleton, not a tuned cold start; no convergence or accuracy property is
  asserted (that would be an L2 claim — none are made, [`CLAIMS.md`](CLAIMS.md)).

## Constraints carried from the contract

- Every calibration step is a **pure function of its declared inputs** (the
  v0.6.0 falsifier in [`VALIDATION_PLAN.md`](VALIDATION_PLAN.md)).
- The fixed-point feature path (v0.4.0) precedes calibration, so alignment runs
  on deterministic features.
- No calibration step exposes raw signal across the application boundary
  ([`PRIVACY_BOUNDARY.md`](PRIVACY_BOUNDARY.md)).

The normative surface is [`PIPELINE_CONTRACT.md`](PIPELINE_CONTRACT.md) §12.

---

The AxonOS Project · axonos.org · connect@axonos.org · security@axonos.org · github.com/AxonOS-org
