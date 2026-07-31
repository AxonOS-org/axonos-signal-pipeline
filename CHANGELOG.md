# Changelog

## [0.9.1] — 2026-07-30

### Fixed
- **A hand-written saturation guard that clippy names better than I did.** The
  Goertzel recurrence bounded its state with an `if`/`else if` pair; `clamp` says
  the same thing in one expression and cannot be got wrong by editing one arm.
  Caught by CI's clippy at 1.97 and invisible to the 1.75 available where this
  was written — the same version gap that produced the 0.8.0 round, now with the
  pattern removed rather than the instance patched.

## [0.9.0] — 2026-07-30

Twelve capabilities, closing every item the roadmap had carried as deferred.
The roadmap row that listed them is gone — not edited, gone: three of the three
things it called planned had already shipped, which made it the most misleading
line in the repository.

### Added — spatial (`spatial.rs`)
1. **Re-referencing**: common average, single channel, and the average of a
   nominated set. A recording is only ever a potential *difference*, so the
   reference is a choice that changes every number downstream; it now happens at
   a declared point instead of wherever a caller reaches for it. CAR documents
   its own known cost: below roughly eight channels the average is dominated by
   whichever channels are active, and that is distortion, not rounding.
2. **Caller-supplied spatial filters** — the shape CSP, xDAWN and Laplacian
   montages all take, including dimension-reducing projections. This crate does
   not *learn* such a matrix; learning one needs labels and would be a decoder,
   which `CLAIMS.md` does not claim. It executes one exactly, so a matrix trained
   anywhere runs here reproducibly.
3. **Per-epoch baseline removal** with the baseline length passed explicitly, so
   the ordering that every implementation gets wrong — measure *before* the
   window of interest, subtract from the whole epoch — is part of the call rather
   than a convention.

### Added — artifact screening (`artifact.rs`)
4. **Findings as independent bits**: saturation, amplitude, slew, flatline,
   drift. `ArtifactFlag` answers "is this clean" with three ordered values and
   its vectors are pinned, so it is untouched. It could not answer the question a
   rejection actually raises, which is *why*: a rail and a loose electrode need
   different responses from whoever is wearing the device. Findings are a set
   because they co-occur — a loose electrode drifts *and* flatlines.
5. **Per-channel screening** of an interleaved block. One loose electrode should
   cost one channel, not the epoch.
6. **A disqualification rule that is argued rather than assumed**: saturation,
   flatline and slew disqualify; amplitude excess and drift are reported and do
   not. A real evoked response can exceed a conservative threshold, and
   discarding it would bias a decoder against exactly the epochs carrying
   information.

### Added — calibration (`calibrate.rs`)
7. **Online adaptation** — the last deferred item. Electrodes settle, gel
   spreads, a subject shifts, and a whitener from the first minute describes a
   head that is no longer there.
8. **The refresh cost is explicit, not hidden in an accessor.** Recomputing is
   fourteen iterations of matrix multiplication, and a real-time chain that pays
   that at an unpredictable moment has no worst case worth stating. Reading the
   whitener is cheap and never recomputes; staleness is readable at any time;
   refreshing is the caller's to schedule. Refresh is driven by observation
   count and **never by a clock**, because a time-based policy would make the
   same input produce different output on a slower machine.

### Added — spectral (`spectral.rs`)
9. **Narrowband power by Goertzel** rather than an FFT. Two of the three
   paradigms this pipeline serves are frequency questions needing a handful of
   specific bins, and four recurrences over a 1024-sample window cost a fraction
   of a 1024-point transform whose 1020 unused bins are discarded.
10. **Coefficient computation at configuration time**, with an integer cosine
    accurate to one count in ~31 700 against the closed form, kept deliberately
    off the sample path. A frequency at or above Nyquist is refused rather than
    given a coefficient that would look like an answer.
11. **A bank for multi-target decisions and a permille power ratio.** The bank
    returns numbers; the argmax is the caller's, because choosing a target is a
    decision and decisions belong above this crate. A ratio with a zero
    denominator returns `None` — an absent measurement is not a large ratio.

### Added — rate (`rate.rs`)
12. **Decimation with an anti-alias guard.** Dropping samples folds everything
    above the new Nyquist into the band that survives, and folded energy is
    indistinguishable from signal once it has landed. `decimate_checked` requires
    the caller to state the band limit they have filtered to and refuses the call
    when the decimated Nyquist does not exceed it — so a decimation appears in
    review with its justification attached rather than as a bare stride. The raw
    form is named `decimate_unfiltered` for what it is.

### Fixed
- **`validate_vectors.py` compared against a hardcoded `"0.8.0"`** and was
  already one release stale before anyone noticed. It now derives the expected
  version from `Cargo.toml` and the generator's declaration and checks the three
  agree — a validator that repeats a number is one more place for it to drift,
  and drift is precisely what it exists to catch. Verified in both directions: a
  deliberately falsified version is rejected.
- The roadmap's `planned` row listed three shipped features.

## [0.8.0] — 2026-07-30

### Fixed
- **The 0.7.0 insertion split a doc comment and hung it on the wrong item.**
  `ZeroCalib`'s documentation began *"Zero-calibration skeleton: accumulate
  session covariances, then finalize a"* — and the rest of that sentence was
  nine hundred lines away, because the new constants had been inserted into the
  middle of the comment. The orphaned first line then documented
  `ALIGN_INTERNAL_SHIFT` instead, so the struct lost its documentation and a
  constant gained a misleading one. Clippy named it exactly
  (`empty_line_after_doc_comment`); nothing else would have. The comment is
  re-joined and its text corrected: `ZeroCalib` is no longer a skeleton, and
  what remains unclaimed is a performance property rather than the mechanism.
- **Eight** `needless_range_loop` findings in code added in 0.7.0, in two
  rounds — and the second round is the interesting one. The first fix passed a
  local `cargo clippy` and CI failed anyway, because the authoring environment
  has clippy **1.75** from a distribution package while CI takes **stable**,
  which was 1.97: twenty-two releases of new lint coverage apart. "Clippy clean
  locally" is therefore not evidence about CI, and treating it as evidence is
  what produced a second red build.

  Rather than guess which remaining instances a newer lint would catch, the
  pattern is removed from this release's code entirely: every `for i in 0..N`
  over a slice is now an iterator, including the pairs CI did *not* flag. Two
  symmetry assertions that each wrote the same nested loop are replaced by one
  `assert_symmetric` helper, which removes the duplication that made the defect
  appear twice.

  The original four `needless_range_loop` warnings — two in the
  inverse-square-root routine, two in its test helpers. Rewritten to iterate
  the thing being indexed, which in the matrix product also says what the
  arithmetic means instead of restating an index twice.
- **The 0.7.0 release bumped one file.** `Cargo.toml` said 0.7.0 while the
  README badge, `CITATION.cff`, the conformance vector filename and its
  `vector_version`, six normative documents, the calibration module header and
  the conformance test header all still said 0.6.0. A version that appears in
  one place and not the others is worse than no version: every other surface
  becomes a quiet lie about which behaviour it describes. Every occurrence is
  now 0.8.0, and the vector file is renamed to match rather than left carrying
  a stale name.
- **The repository carried no attribution at all.** Seventeen Rust files with
  no copyright header, nine documents with no notice, and no `NOTICE` file —
  which Apache-2.0 section 4(d) requires a redistributor to retain, and which
  cannot be retained if it does not exist. Added: SPDX identifiers and a
  copyright line naming **Denis Yermakou** in every source file, a `NOTICE`
  stating authorship, attribution terms, the intellectual-property position and
  the pre-clinical scope, and a signature footer on every document.

### Changed
- Conformance vectors are re-pinned as `vectors/pipeline-vectors-v0.8.0.json`
  with `vector_version: "0.8.0"`. The values are unchanged; the name and the
  declared version now match the release that ships them.
- The README roadmap table records 0.6.0 and 0.7.0 as shipped instead of
  showing 0.6.0 as current — the row had been stale since the aligner landed.

## [0.7.0] — 2026-07-29

### Added
- **`inverse_sqrt_spd`** — the symmetric `R^{-1/2}` this crate has deferred
  since v0.6.0, by Newton–Schulz in integer arithmetic. Multiplication only: no
  eigendecomposition, no matrix square root, no division inside the loop, which
  is what makes it expressible exactly so two implementations agree bit for bit
  rather than approximately. Internal precision `Q24`, result at `WHITEN_SHIFT`
  so it is interchangeable with `whiten_cholesky` and consumable by `align`.
  The iteration count is a constant, never a convergence test: a loop that
  stops when it is satisfied has an input-dependent execution time, and this
  crate lives inside a system that must state a worst case.
- **`ZeroCalib::aligner`** — the symmetric whitener over unlabelled
  observations. Unlabelled is the precise claim: no calibration *session* is
  required, a short stretch of ordinary use still is.
- Eleven tests: closed-form agreement for scalar and diagonal inputs, symmetry
  where the Cholesky factor is triangular, `W R Wᵀ ≈ I`, the documented
  shrinkage trade-off in the documented direction, convergence stability under
  additional iterations, bit-for-bit determinism, and refusal on degenerate
  input rather than a guess.

### Documented
- **What alignment achieves, and what it cannot.** For observation `X = M · s`,
  the symmetric whitener maps the class covariance to `U G_c Uᵀ` where `G_c` is
  subject-independent and `U` is the orthogonal polar factor of `M Σ̄^{1/2}`.
  Alignment therefore reduces inter-subject difference to a **pure rotation**
  and does not remove it — and no whitener satisfying `W R Wᵀ = I` can, since
  they all differ from `R^{-1/2}` by a left orthogonal factor.

  Consequently `CLAIMS.md` still makes no transfer or accuracy claim, and the
  reason is now a proof rather than caution. Whether the residual rotation is
  benign is an empirical property of the recording montage: with a shared 10-20
  layout the mixings are similar and `U` is near identity, which is the regime
  the literature reports; with unconstrained mixing a transferred classifier
  sits at chance, for this whitener and for Cholesky alike.

All notable changes to this project are documented here. The format is based
on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.6.0] - 2026-06-20

Adds deterministic, vector-pinned **calibration** machinery, completing a
three-layer release (features 0.4.0, classifier inference 0.5.0, calibration
0.6.0) that ship together. As with every stage in this crate, these are defined
deterministic transforms: there is **no trained model** and **no** measured
accuracy, latency, power, or clinical claim. Engineering demonstrator, **not a
medical device**.

### Added

- Calibration in `axonos-pipeline-core::calibrate` (`#![no_std]`,
  allocation-free, `#![forbid(unsafe_code)]`):
  - `covariance` — mean-removed channel covariance (integer accumulation).
  - `SessionMean` — running mean of covariance matrices (the session reference).
  - `drift_update` — in-place `Q15` exponential update of a reference.
  - `whiten_cholesky` — fixed-point Cholesky reference whitener `W = L⁻¹` with
    `W R Wᵀ = I` (returns `None` for non-positive-definite input); `align`
    applies it.
  - `ZeroCalib` — typed zero-calibration skeleton (accumulate → finalize
    whitener).
- Conformance vectors `covariance` (`PV-COV-*`) and `whiten_cholesky`
  (`PV-WHIT-*`): the whitener and the exact `align(W, R)` (the `Q16` identity to
  fixed-point error) are pinned. Normative PIPELINE_CONTRACT §12;
  `docs/CALIBRATION.md` rewritten as implemented.

### Deferred

- Symmetric `R^{-1/2}` Euclidean Alignment, online adaptation, and any
  transfer/accuracy claim (`docs/CALIBRATION.md`).

## [0.5.0] - 2026-06-20

Adds deterministic **classifier inference** machinery. Model parameters are
caller-supplied; there is **no trained model** in this repository and **no**
accuracy claim — the only asserted property is determinism. Engineering
demonstrator, **not a medical device**.

### Added

- Classifier inference in `axonos-pipeline-core::classify`:
  - `distance_sq` — saturating squared Euclidean distance.
  - `classify_mdm` — minimum-distance-to-mean decision with margin-based
    confidence and an abstain threshold.
  - `lda_score` / `classify_lda_binary` — linear/LDA score and two-class
    decision with a dead-band abstain region.
- Error variants `DimensionMismatch`, `EmptyClassSet`.
- Conformance vectors `classify_mdm` (`PV-MDM-*`) and `classify_lda`
  (`PV-LDA-*`); normative PIPELINE_CONTRACT §11.

## [0.4.0] - 2026-06-20

Adds deterministic **fixed-point feature extraction**. All features are defined
integer transforms (no floating point on the data path) with **no**
measured-quality claim. Engineering demonstrator, **not a medical device**.

### Added

- Feature extraction in `axonos-pipeline-core::feature`: `variance`,
  `log_variance_q16`, `rms` (standard deviation), `abs_mean`, `zero_crossings`,
  and the `isqrt` / `log2_q16` integer primitives.
- Conformance vectors `feature` (`PV-FEAT-*`), `log2_q16` (`PV-LOG2-*`), `isqrt`
  (`PV-ISQRT-*`); normative PIPELINE_CONTRACT §10.

### Changed

- `FeatureVector<D>` is reframed as a legacy `f32` interop container (outside any
  conformance claim); the deterministic feature path is the integer functions
  above. DSP module docs updated accordingly.

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
  `vectors/pipeline-vectors-v0.6.0.json`, pinning output **and** post-run
  `state_hash`, with matching `tests/conformance.rs` cases and generated data.
- `docs/PIPELINE_CONTRACT.md` §9.3 (DC blocker) and §9.4 (biquad) — normative
  IIR arithmetic and state-hash byte order.
- `docs/DSP_SPEC.md` and `docs/SAFETY_NOTES.md`.

### Changed

- Vector set is now `vector_version` `0.3.0`; the vector file is renamed to
  `pipeline-vectors-v0.6.0.json` (regenerated together with `SHA256SUMS`).
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

[0.6.0]: https://github.com/AxonOS-org/axonos-signal-pipeline/releases/tag/v0.6.0
[0.5.0]: https://github.com/AxonOS-org/axonos-signal-pipeline/releases/tag/v0.5.0
[0.4.0]: https://github.com/AxonOS-org/axonos-signal-pipeline/releases/tag/v0.4.0
[0.3.0]: https://github.com/AxonOS-org/axonos-signal-pipeline/releases/tag/v0.3.0
[0.2.4]: https://github.com/AxonOS-org/axonos-signal-pipeline/releases/tag/v0.2.4
[0.1.0]: https://github.com/AxonOS-org/axonos-signal-pipeline/releases/tag/v0.1.0

---

The AxonOS Project · axonos.org · connect@axonos.org · security@axonos.org · github.com/AxonOS-org
