# Validation Plan

Validation in AxonOS is gated by **evidence level** (see
[`CLAIMS.md`](CLAIMS.md)). A capability ships only when its gate is met, and
its claim is labelled with the evidence that backs it. This plan states the
gate for each roadmap version and the falsifiers that would invalidate it.

## Standing gates (every release)

- `cargo fmt --all -- --check` clean.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `cargo test --workspace` green (conformance + doctests, including the
  `compile_fail` boundary doctest).
- `cargo build -p axonos-pipeline-core --target thumbv7em-none-eabihf` (no_std)
  succeeds.
- `cargo doc` with `-D warnings`.
- `python3 tools/validate_vectors.py` passes: generator output is
  byte-identical to the committed vectors, fixtures, and generated Rust; the
  `SHA256SUMS` manifest verifies.
- `python3 tools/check_hygiene.py` passes: no private/prototype contact
  metadata in the tree.

## Per-version gates

| Version | New capability | Gate (evidence) | Falsifier |
|---|---|---|---|
| **v0.1.0** | Type contract + conformance vectors | L1: all vectors reproduced; boundary sealed; build constraints hold | A toolchain that reproduces the generator but disagrees with a committed vector; a downstream impl of `BoundarySafe` for a raw type compiling |
| **v0.2.4** (met) | DSP primitives: DC (mean) removal, fixed-point FIR | L1: `dc_remove` / `fir` vectors reproduced bit-for-bit (integer arithmetic, defined rounding/saturation) | A DSP output diverging from its pinned vector on any conformant build |
| **v0.3.0** (met) | Stateful fixed-point IIR filter bank (DC blocker, notch, band-pass) | L1: `biquad` / `dc_blocker` vectors reproduced bit-for-bit; post-run `state_hash` pinned; unsupported sample rates rejected | A filter output or `state_hash` diverging from its pinned vector on any conformant build |
| **v0.4.0** (met) | Features (fixed-point): variance, log-variance, RMS, abs-mean, zero-crossings | L1: `feature` / `log2_q16` / `isqrt` vectors reproduced bit-for-bit; no floating point on the data path | A feature value differing across two conformant builds for identical input |
| **v0.5.0** (met) | Classifier inference (MDM / linear-LDA) with confidence + abstain | L1: `classify_mdm` / `classify_lda` decision vectors reproduced for fixed (caller-supplied) parameters and inputs; abstain pinned | Identical parameters + input producing two different `ClassifierDecision`s |
| **v0.6.0** (met) | Calibration: covariance, session mean, drift update, Cholesky whitening, ZeroCalib skeleton | L1: `covariance` / `whiten_cholesky` vectors reproduced; `align(W,R)` pinned as the `Q16` identity to fixed-point error | A calibration step that is not a pure function of its declared inputs |
| **v0.7.0** (met) | Symmetric `R^{-1/2}` aligner by Newton–Schulz in integer arithmetic, fixed iteration count | L1: symmetry to fixed-point rounding; `W R Wᵀ = I` for the regularised input; closed-form agreement on scalar and diagonal inputs; bit-for-bit determinism; refusal on degenerate input | A result that is not symmetric, or that differs between two runs on identical input, or an iteration count that varies with the data |
| **v0.8.0** (met) | Release hygiene and attribution across every surface; the bound on what alignment can achieve | L1: the version is identical on every surface and checked by script, not by memory; SPDX and copyright present in every source file; `NOTICE` present. The alignment bound is L1 as algebra: `R̄^{-1/2}(P G_c Pᵀ)R̄^{-1/2} = U G_c Uᵀ` with `U` orthogonal, verified numerically to 1e-7 | A surface still naming an older version; a source file with no attribution; a claim of transfer or accuracy anywhere in the crate |

## Moving to L2 (measured)

When the pipeline runs on real hardware or datasets, any latency, jitter,
accuracy, or power figure enters this repository as **L2**, accompanied by:

- raw traces or dataset references and exact preprocessing,
- the measurement method and environment,
- a clear label distinguishing it from L1 claims.

Until then, no such number is asserted here. L3 (independent reproduction) is
tracked separately and is never assumed.

---

The AxonOS Project · axonos.org · connect@axonos.org · security@axonos.org · github.com/AxonOS-org

---

<sub>**AxonOS Signal Pipeline v0.11.0** · © 2026 Denis Yermakou · Apache-2.0 OR MIT ·
authored for [The AxonOS Project](https://axonos.org) · see [NOTICE](../NOTICE)
for attribution terms · connect@axonos.org</sub>
