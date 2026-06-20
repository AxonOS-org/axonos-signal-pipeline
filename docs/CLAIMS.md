# Claims and Evidence Levels (v0.6.0)

AxonOS separates claims by the strength of evidence behind them. This
repository asserts **only** machine-checkable (L1) claims. It does not assert
any measured-performance (L2) or independently-reproduced (L3) claim, and
nothing here should be read as a clinical or regulatory statement.

## Evidence levels

- **L1 — machine-checkable.** Provable or testable in this repository from a
  stock toolchain with no external data: type properties, determinism against
  pinned vectors, build constraints.
- **L2 — measured.** Quantities obtained from measurement on real hardware or
  datasets (latency, jitter, classification accuracy, power). These require
  raw traces and a published method. **None are claimed here.**
- **L3 — independently reproduced.** L2 results reproduced by an independent
  party. **None are claimed here.**

## L1 claims made by this repository

1. **Determinism.** Every behavioural rule in
   [`PIPELINE_CONTRACT.md`](PIPELINE_CONTRACT.md) is pinned by a vector in
   `vectors/pipeline-vectors-v0.6.0.json` and exercised by
   `crates/axonos-pipeline-core/tests/conformance.rs`. The vectors are exactly
   reproducible from `tools/gen_test_vectors.py` (checked by
   `tools/validate_vectors.py` and an integrity manifest).
2. **Sealed application boundary.** Only `ClassifierDecision` implements the
   sealed `BoundarySafe` trait. That raw signal types are rejected at the
   boundary is enforced at compile time, including a `compile_fail` doctest in
   `src/boundary.rs`.
3. **Sample privacy in diagnostics.** `RawFrame`'s `Debug` implementation
   redacts sample values; a test asserts no raw value appears in its output.
4. **Build constraints.** The crate is `#![no_std]` (outside tests),
   `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`, and has **zero**
   dependencies (runtime and dev). CI builds it for a bare-metal target.
5. **Deterministic integer DSP.** The DC mean-removal, fixed-point FIR, and
   stateful IIR filter bank (DC blocker, notch, band-pass) are integer-only
   (defined truncation, arithmetic shift, round-half-up, and saturation) and
   therefore bit-exact across platforms. Their behaviour — including each
   filter's post-run `state_hash` — is pinned by the `dc_remove`, `fir`,
   `biquad`, and `dc_blocker` vectors and reproduced in `tests/conformance.rs`.
6. **Deterministic feature extraction.** The fixed-point feature functions
   (`variance`, `log_variance_q16`, `rms`, `abs_mean`, `zero_crossings`, and the
   `isqrt` / `log2_q16` primitives) are integer-only and bit-exact; their outputs
   are pinned by the `feature`, `log2_q16`, and `isqrt` vectors.
7. **Deterministic classifier inference.** The minimum-distance-to-mean and
   linear/LDA decision rules (`classify_mdm`, `classify_lda_binary`,
   `distance_sq`, `lda_score`) are integer-only; for **caller-supplied** model
   parameters, identical inputs yield an identical `ClassifierDecision`, pinned by
   the `classify_mdm` and `classify_lda` vectors.
8. **Deterministic calibration transforms.** Covariance, session mean, drift
   update, and Cholesky reference whitening (`covariance`, `SessionMean`,
   `drift_update`, `whiten_cholesky`, `align`) are integer/fixed-point; whitening
   satisfies `W R Wᵀ = I` to fixed-point error, pinned (whitener and exact
   alignment result) by the `covariance` and `whiten_cholesky` vectors.

## What is explicitly NOT claimed here

- No classification accuracy of any kind. The classifier provides deterministic
  **inference machinery** (minimum-distance-to-mean, linear/LDA) over
  caller-supplied parameters; there is **no trained model** in this repository,
  and the parameters used in vectors and tests are illustrative. No accuracy,
  separability, or transfer property is claimed.
- No calibration performance. Covariance whitening is **deterministic and
  algebraically verified** (`W R Wᵀ = I`), but no claim is made that it improves
  classification, transfers across sessions, or converges; the symmetric
  `R^{-1/2}` Euclidean Alignment form is deferred ([`CALIBRATION.md`](CALIBRATION.md)).
- No latency, jitter, throughput, or power figure.
- No hardware-compatibility claim beyond "builds for the listed targets".
- No validated or certified filter design. The IIR filter bank (DC blocker,
  notch, band-pass) is **deterministic and vector-pinned**, but its frequency
  response is **not** certified and **not** clinically validated: each preset is
  a single second-order section with engineering-chosen coefficients. `fir`
  remains a generic fixed-point convolution engine. No band-pass, notch, or
  frequency-response performance property is claimed.
- No clinical validation, no patient data, no regulatory clearance. This is a
  pre-clinical engineering artifact, **not a medical device**.

Performance discussion that appears in AxonOS essays or talks is separate
narrative material and is **not** imported into this repository as a claim.
When L2 results exist for the pipeline, they will arrive here as raw traces
plus method under [`VALIDATION_PLAN.md`](VALIDATION_PLAN.md), clearly labelled.

---

The AxonOS Project · axonos.org · connect@axonos.org · security@axonos.org · github.com/AxonOS-org
