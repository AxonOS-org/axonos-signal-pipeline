# Claims and Evidence Levels (v0.1.0)

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
   `vectors/pipeline-vectors-v0.1.0.json` and exercised by
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

## What is explicitly NOT claimed here

- No classification accuracy of any kind. The v0.1.0 classifier type is a
  decision **container**; there is no trained model in this repository.
- No latency, jitter, throughput, or power figure.
- No hardware-compatibility claim beyond "builds for the listed targets".
- No clinical validation, no patient data, no regulatory clearance. This is a
  pre-clinical engineering artifact, **not a medical device**.

Performance discussion that appears in AxonOS essays or talks is separate
narrative material and is **not** imported into this repository as a claim.
When L2 results exist for the pipeline, they will arrive here as raw traces
plus method under [`VALIDATION_PLAN.md`](VALIDATION_PLAN.md), clearly labelled.

---

The AxonOS Project · axonos.org · connect@axonos.org · security@axonos.org · github.com/AxonOS-org
