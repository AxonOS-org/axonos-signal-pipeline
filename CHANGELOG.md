# Changelog

All notable changes to this project are documented here. The format is based
on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-06-20

Adds a stateful fixed-point IIR filter bank — a DC blocker, power-line notch,
and band-pass presets — behind conformance vectors that also pin each filter's
post-run state hash. No existing API changes; the feature, classifier, and
calibration stages remain typed placeholders (the roadmap shifts one minor).

### Added

- Stateful fixed-point IIR filters in `axonos-pipeline-core::filter` (single
  channel, `#![no_std]`, allocation-free, `#![forbid(unsafe_code)]`):
  - `DcBlocker` — first-order high-pass DC blocker, `Q15` pole (default 0.995),
    with `step` / `process` / `reset` / `state_hash` and `with_r` validation.
  - `Biquad` — `Q15` Direct-Form-I biquad with the same surface.
  - `NotchMode` (`Hz50` / `Hz60` / `Disabled`) + `notch_coeffs`.
  - `BandpassPreset` (`MotorIntent` / `Attention` / `SafetyWide` / `Disabled`)
    + `bandpass_coeffs`.
  - Tabulated `Q15` coefficients for 250 / 500 / 1000 Hz; unsupported rates are
    rejected. Coefficients are computed offline (RBJ); the core uses no float.
  - `BIQUAD_SHIFT`, `BIQUAD_ONE`, and `BiquadCoeffs` (+ `IDENTITY`).
- `PipelineError` variants: `UnsupportedSampleRate`, `InvalidCoefficient`.
- Conformance vectors `biquad`, `dc_blocker`, and a shared `filter_signal` in
  `vectors/pipeline-vectors-v0.3.0.json`, pinning output **and** post-run
  `state_hash`, with matching `tests/conformance.rs` cases and generated data.
- `docs/PIPELINE_CONTRACT.md` §9.3 (DC blocker) and §9.4 (biquad) — normative
  IIR arithmetic and state-hash byte order.
- `docs/DSP_SPEC.md` and `docs/SAFETY_NOTES.md`.

### Changed

- Vector set is now `vector_version` `0.3.0`; the vector file is renamed to
  `pipeline-vectors-v0.3.0.json` (regenerated together with `SHA256SUMS`).
- Roadmap shifts one minor: fixed-point features → v0.4.0, classifier → v0.5.0,
  calibration → v0.6.0. Docs updated accordingly.

### Notes

- The IIR sections are an **engineering demonstrator** — single second-order
  sections with no certified frequency response and no clinical validation.
- Pre-clinical engineering artifact; **not a medical device**. No accuracy,
  latency, or power figure is claimed.

## [0.2.4] - 2026-06-18

Adds the first deterministic DSP primitives behind conformance vectors, and
fixes the lint that was failing CI.

### Added

- Deterministic integer DSP in `axonos-pipeline-core::dsp`:
  - `remove_mean` — DC (mean) removal; mean truncated toward zero, saturating
    subtraction.
  - `fir` — causal fixed-point FIR engine (i64 accumulator, arithmetic shift,
    round-half-up, i32 saturation); a generic convolution engine with **no**
    filter-design or frequency-response claim.
  - `MAX_FIR_SHIFT` constant.
- DSP error variants on `PipelineError`: `EmptyInput`, `EmptyKernel`,
  `OutputLengthMismatch`, `InvalidShift`.
- DSP conformance vectors (`dc_remove`, `fir`) in
  `vectors/pipeline-vectors-v0.2.4.json`, with matching `tests/conformance.rs`
  cases and generated test data; vector set is now `vector_version` `0.2.4`.
- `docs/PIPELINE_CONTRACT.md` §9 — normative DSP arithmetic.

### Fixed

- Clippy `needless_lifetimes` on `RawFrame::epochs` (lifetime now elided),
  which had been failing `clippy -D warnings` in CI.

### Changed

- README, CLAIMS, LIMITATIONS, and VALIDATION_PLAN updated to describe the
  shipped DSP primitives and to state explicitly that no band-pass, notch, or
  frequency-response behaviour is claimed.

### Notes

- DSP is integer fixed-point and bit-exact; no accuracy, latency, or power
  figure is claimed. The pipeline terminates at `ClassifierDecision`;
  conversion to the canonical `IntentObservation` and its consent gating remain
  in `axonos-kernel` / `axonos-consent`. Pre-clinical engineering artifact;
  not a medical device.

## [0.1.0] - 2026-06-10

Initial release: the type contract and conformance surface for the AxonOS
reference signal pipeline.

### Added

- `axonos-pipeline-core` crate (`#![no_std]`, `#![forbid(unsafe_code)]`,
  zero dependencies) with the typed stage contract:
  - `RawFrame` — validated raw acquisition frame, time-major interleaved,
    column-compacted 24-bit samples; FNV-1a 64 integrity checksum; `Debug`
    redacts sample values.
  - `ChannelMask`, `SampleRate` newtypes.
  - `Epoch` / `EpochIter` deterministic windowing with `ExactSizeIterator`.
  - `artifact_scan` amplitude/saturation screening (pure integer).
  - `FeatureVector<D>` placeholder type.
  - `ClassifierDecision` pipeline-terminal type.
  - Sealed `BoundarySafe` trait — only `ClassifierDecision` may cross the
    application boundary; raw types rejected at compile time.
- Conformance vectors `vectors/pipeline-vectors-v0.1.0.json` and synthetic
  fixture `fixtures/synthetic/frame-0001.json`, generated by
  `tools/gen_test_vectors.py` and integrity-pinned by `vectors/SHA256SUMS`.
- CI gates `tools/validate_vectors.py` (exact reproducibility) and
  `tools/check_hygiene.py` (contact-metadata hygiene).
- Documentation: pipeline contract, claims and evidence levels, limitations,
  privacy boundary, validation plan, calibration design note.

### Notes

- This release implements no DSP, feature extraction, classifier, or
  calibration; those are typed placeholders introduced behind conformance
  vectors on the roadmap (v0.2.0–v0.5.0). No accuracy, latency, or power
  figure is claimed. Pre-clinical engineering artifact; not a medical device.

[0.3.0]: https://github.com/AxonOS-org/axonos-signal-pipeline/releases/tag/v0.3.0
[0.2.4]: https://github.com/AxonOS-org/axonos-signal-pipeline/releases/tag/v0.2.4
[0.1.0]: https://github.com/AxonOS-org/axonos-signal-pipeline/releases/tag/v0.1.0

---

The AxonOS Project · axonos.org · connect@axonos.org · security@axonos.org · github.com/AxonOS-org
