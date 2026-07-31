# Calibration (v0.9.0)

Calibration reduces inter-session covariate shift before classification. As of
v0.8.0 the **deterministic machinery** is implemented and vector-pinned in
`src/calibrate.rs`. As with the rest of the crate, these are **defined
transforms** with no accuracy, transfer, or convergence claim
([`CLAIMS.md`](CLAIMS.md)).

## Implemented (v0.9.0)

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

## The symmetric root, and what alignment actually achieves

`inverse_sqrt_spd` (v0.7.0) computes the symmetric `R^{-1/2}` this document
previously deferred. It is the whitener Euclidean Alignment specifies, and it
is not the Cholesky factor: both satisfy `W R Wᵀ = I`, but every whitener
satisfying that identity equals `R^{-1/2}` up to a **left orthogonal factor**,
so the choice decides which frame the whitened data lands in.

### The result that bounds the claim

Let a subject's observation be `X = M · s` for an unknown mixing `M`, with
class covariance `Σ_c` in source space and `Σ̄` the mean over classes. Write
`P = M Σ̄^{1/2}`, so the reference covariance is `R̄ = P Pᵀ` and the class
covariance is `M Σ_c Mᵀ = P G_c Pᵀ` with

```
G_c = Σ̄^{-1/2} Σ_c Σ̄^{-1/2}          — subject-independent
```

Applying the symmetric whitener and using the polar decomposition
`P = (P Pᵀ)^{1/2} U` with `U` orthogonal:

```
R̄^{-1/2} (P G_c Pᵀ) R̄^{-1/2} = U G_c Uᵀ
```

**Alignment reduces the difference between subjects to a pure rotation.** It
does not remove it. `U` is the orthogonal polar factor of `M Σ̄^{1/2}` and is
subject-specific; the Cholesky whitener leaves a different orthogonal factor by
the same argument. Neither is rotation-free, and no whitener defined by
`W R Wᵀ = I` can be.

Verified numerically against the closed form: the identity above holds to
`1e-7` for random SPD inputs, and the residual factor is orthogonal to `1e-8`.

### What follows, and what does not

Whether the residual rotation is benign is an **empirical property of the
recording montage**, not a theorem. Where subjects share electrode positions —
a fixed 10-20 layout — the mixing matrices are similar and `U` is near the
identity, which is the regime the alignment literature reports success in.
Where the mixings are arbitrary, `U` is arbitrary and alignment provides no
transfer at all: a simulation with unconstrained random mixing puts a
transferred classifier at chance, for both whiteners.

So this crate ships the mechanism and no performance claim. Establishing that
`U` is small enough on real hardware needs real recordings from real subjects
through the same montage, which is an experiment, not an implementation.

---

<sub>**AxonOS Signal Pipeline v0.9.1** · © 2026 Denis Yermakou · Apache-2.0 OR MIT ·
authored for [The AxonOS Project](https://axonos.org) · see [NOTICE](../NOTICE)
for attribution terms · connect@axonos.org</sub>
