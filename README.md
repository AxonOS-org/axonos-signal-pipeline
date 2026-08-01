<div align="center">

# AxonOS Signal Pipeline

**Reference deterministic BCI signal pipeline for AxonOS** — the executable, vector-pinned path from a raw acquisition frame to a typed, consent-bound decision.

[![ci](https://github.com/AxonOS-org/axonos-signal-pipeline/actions/workflows/ci.yml/badge.svg)](https://github.com/AxonOS-org/axonos-signal-pipeline/actions/workflows/ci.yml)
[![release](https://img.shields.io/badge/release-v0.9.3-6af6ff)](https://github.com/AxonOS-org/axonos-signal-pipeline/releases)
[![license](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue)](#licensing)
[![rust](https://img.shields.io/badge/rust-1.75%2B-dea584)](Cargo.toml)
[![no_std](https://img.shields.io/badge/no__std-yes-2ea44f)](#properties)
[![unsafe](https://img.shields.io/badge/unsafe-forbidden-2ea44f)](#properties)
[![deps](https://img.shields.io/badge/dependencies-0-2ea44f)](#properties)

*Engineering demonstrator — not a medical device, no trained model, no measured-performance claim.*

</div>

---

This repository defines the executable path from a raw acquisition frame to a
typed, consent-bound decision:

```text
RawFrame -> Epoch -> (DSP) -> FeatureVector -> ClassifierDecision
                                                     |
                          kernel boundary: canonical IntentObservation
                          (RFC-0006 §4) under consent gating
```

The design rule is strict:

> **Raw neural data must never cross the application boundary.**

Inside the pipeline, code may stream raw samples. At the boundary, only the
pipeline-terminal [`ClassifierDecision`] is permitted to leave; the AxonOS
kernel converts it into the project's canonical `IntentObservation` wire type
under consent gating. This crate deliberately does **not** redefine that wire
type — see [Relationship to other AxonOS repositories](#relationship-to-other-axonos-repositories).

This repository is **not a clinical system, not a medical device, and not a
claim of measured performance.** It is a reference implementation and test
surface for the AxonOS signal-processing contract. See
[`docs/CLAIMS.md`](docs/CLAIMS.md) for exactly what is and is not asserted.

## Status

`axonos-pipeline-core` v0.9.3 — the **type contract, conformance surface,
deterministic DSP primitives, and a stateful fixed-point IIR filter bank**
(a DC blocker, power-line notch, and band-pass presets, alongside the integer
mean-removal and FIR engines), plus **deterministic fixed-point feature
extraction, classifier inference, and calibration**. Every stage is machinery
pinned by conformance vectors — there is **no trained model** and **no** measured
accuracy, latency, or power figure anywhere in this repository.

| Version | Scope | State |
|---|---|---|
| v0.1.0 | Type contract: `RawFrame`, `Epoch`, `ChannelMask`, `SampleRate`, `ArtifactFlag`, `FeatureVector` (placeholder), `ClassifierDecision`, sealed application boundary, FNV-1a frame checksum, synthetic fixtures, conformance vectors | shipped |
| v0.2.4 | Deterministic integer DSP primitives: DC (mean) removal and a fixed-point FIR engine, with a typed DSP error model and bit-exact `dc_remove` / `fir` conformance vectors | shipped |
| v0.3.0 | Stateful fixed-point IIR filter bank: a DC blocker, power-line notch (50/60 Hz), and band-pass presets (motor-intent / attention / safety-wide) over 250/500/1000 Hz, each with `step` / `process` / `reset` / `state_hash` and pinned `biquad` / `dc_blocker` vectors | shipped |
| v0.4.0 | Deterministic fixed-point features: variance, log-variance, RMS, abs-mean, zero-crossings (+ `isqrt` / `log2_q16` primitives), pinned `feature` / `log2_q16` / `isqrt` vectors | shipped |
| v0.5.0 | Classifier inference: minimum-distance-to-mean and linear/LDA decision rules with confidence and abstain — caller-supplied parameters, **no trained model** — pinned `classify_mdm` / `classify_lda` vectors | shipped |
| v0.6.0 | Calibration machinery: channel covariance, session mean, drift update, Cholesky reference whitening (`W R Wᵀ = I`), pinned `covariance` / `whiten_cholesky` vectors | shipped |
| v0.7.0 | Symmetric `R^{-1/2}` aligner (`inverse_sqrt_spd`, `ZeroCalib::aligner`) by Newton–Schulz in integer arithmetic, fixed iteration count | shipped |
| v0.8.0 | Attribution and release hygiene across every surface; the proof that alignment reduces inter-subject difference to a residual rotation and cannot remove it | shipped |
| **v0.9.2** | Spatial layer: re-referencing (CAR, single, linked-average), caller-supplied spatial filters, per-epoch baseline removal · rich artifact findings as independent bits, per channel · online whitener adaptation with the refresh cost made explicit · narrowband power by Goertzel and band-power ratios | **current** |
| v0.10.0+ | Planned: decimation with anti-alias guard, epoch windowing with overlap, bad-channel interpolation | planned |

Each stage ships only once it is covered by conformance vectors and the
validation gates in [`docs/VALIDATION_PLAN.md`](docs/VALIDATION_PLAN.md). No
stage advertises accuracy, latency, or power figures in this repository.

## Bounded model checking

Seven Kani harnesses, in `crates/axonos-pipeline-core/kani-proofs`. Every one
covers a property whose failure is **silent**; nothing is proved here that a
panic would have caught anyway.

| Harness | Property | Why it must be proved rather than tested |
|:--|:--|:--|
| `pipe_p1` | re-referencing clamps, never wraps | a rail minus a positive mean wraps to a large positive value — which looks like a healthy signal of opposite polarity, and the screener would then report amplitude rather than saturation, the one finding that does not disqualify an epoch |
| `pipe_p2` | the common mode is removed exactly | this is what a re-reference is *for*, and a wrapping implementation loses it quietly |
| `pipe_p3` | a refused baseline writes nothing | a partially applied correction leaves early channels carrying an offset the later ones do not, and every channel comparison afterwards is against a number nobody recorded |
| `pipe_p4` | the identity filter is the identity | the one matrix whose correct answer is known for *all* inputs rather than for the ones someone thought to test |
| `pipe_p5` | spatial filtering saturates on narrowing | wrapping the `i64`→`i32` narrowing reports strong negative activity where there was strong positive activity |
| `pipe_p6` | a power ratio is total | `n × 1000` overflows `u64`, and a wrapped ratio is *small* — so an overwhelming band would report as quiet |
| `pipe_p7` | a power ratio is monotone | every downstream comparison assumes it, and an overflow breaks it silently |

Tests sample the input space; these quantify over it. For arithmetic that is
the difference that matters: a test of a thousand random covariance matrices
says nothing about the thousand-and-first, and the thousand-and-first is what
an electrode produces at the moment it comes loose.

The harnesses type-check without Kani installed, so a signature change breaks
the build here rather than leaving a proof that no longer refers to the code it
was written for.

## Properties

- `#![no_std]`, allocation-free on the data path.
- `#![forbid(unsafe_code)]`.
- **Zero dependencies** (no runtime *or* dev dependencies), so the conformance
  surface is fully reproducible from a stock toolchain.
- Deterministic: every behavioural claim is pinned by a vector in
  [`vectors/`](vectors/) and exercised from
  [`crates/axonos-pipeline-core/tests/conformance.rs`](crates/axonos-pipeline-core/tests/conformance.rs).

## Layout

```text
axonos-signal-pipeline/
├── crates/
│   └── axonos-pipeline-core/   # the typed stage contract (this release)
├── fixtures/
│   └── synthetic/              # deterministic, license-free sample frames
├── vectors/
│   ├── pipeline-vectors-v0.8.0.json
│   └── SHA256SUMS              # integrity manifest for the vector artifacts
├── tools/                      # Python (stdlib-only) generator + CI gates
└── docs/                       # contract, claims, limitations, boundary, plan
```

## Quick start

```bash
# Rust: build, lint, and run the conformance tests
cargo test --workspace

# Conformance artifacts must be exactly reproducible from their generator
python3 tools/validate_vectors.py

# Repository hygiene (no private contact metadata)
python3 tools/check_hygiene.py
```

`axonos-pipeline-core` also builds for bare-metal targets, e.g.:

```bash
rustup target add thumbv7em-none-eabihf
cargo build -p axonos-pipeline-core --target thumbv7em-none-eabihf
```

## Conformance vectors

[`vectors/pipeline-vectors-v0.9.3.json`](vectors/pipeline-vectors-v0.9.3.json)
is the language-neutral definition of v0.8.0 behaviour: FNV-1a anchors, the
fixture frame checksum, window-count cases, artifact-scan cases, channel-mask
column mappings, the DSP cases (`dc_remove`, `fir`), and the stateful IIR
filter cases (`biquad`, `dc_blocker`) over a shared `filter_signal`. It is
produced by
[`tools/gen_test_vectors.py`](tools/gen_test_vectors.py), which is the single
source of truth; the Rust test data in
`crates/axonos-pipeline-core/tests/data/vectors.rs` is generated from the same
run and kept byte-identical by CI.

**Atomic-update rule.** Any change to a vector or fixture requires re-running
the generator and committing *all* of its outputs — including a regenerated
[`vectors/SHA256SUMS`](vectors/SHA256SUMS) — in the **same commit**. CI fails
otherwise. The `vector_version` is the version of the vector set and is
independent of the crate version.

## Relationship to other AxonOS repositories

| Concern | Where it lives |
|---|---|
| Canonical `IntentObservation` wire type (RFC-0006 §4) | `axonos-intent` crate in [`AxonOS-org/axonos-kernel`](https://github.com/AxonOS-org/axonos-kernel) |
| Consent gating of intent at the boundary | [`AxonOS-org/axonos-consent`](https://github.com/AxonOS-org/axonos-consent) |
| Application / host SDK consuming `IntentObservation` | [`AxonOS-org/axonos-sdk`](https://github.com/AxonOS-org/axonos-sdk) |
| Normative specifications | [`AxonOS-org/axonos-rfcs`](https://github.com/AxonOS-org/axonos-rfcs), [`AxonOS-org/axonos-standard`](https://github.com/AxonOS-org/axonos-standard) |

This repository owns the **signal → decision** path only. It stops at
`ClassifierDecision`; the kernel owns the consent-gated conversion to the
canonical `IntentObservation`. Keeping a single wire type avoids divergence —
the pipeline is one organ of the larger system, not a parallel one.

## Licensing

- **Code** (everything under `crates/` and `tools/`): dual-licensed
  **Apache-2.0 OR MIT** — see [`LICENSE-APACHE`](LICENSE-APACHE) and
  [`LICENSE-MIT`](LICENSE-MIT).
- **Conformance vectors and synthetic fixtures** (`vectors/`, `fixtures/`):
  dedicated to the public domain under **CC0-1.0** — see
  [`vectors/README.md`](vectors/README.md).

## Security

Please report vulnerabilities privately per [`SECURITY.md`](SECURITY.md)
(security@axonos.org). Do not open public issues for security reports.

---

The AxonOS Project · axonos.org · connect@axonos.org · security@axonos.org · github.com/AxonOS-org

---

<sub>**AxonOS Signal Pipeline v0.9.3** · © 2026 Denis Yermakou · Apache-2.0 OR MIT ·
authored for [The AxonOS Project](https://axonos.org) · see [NOTICE](NOTICE)
for attribution terms · connect@axonos.org</sub>
