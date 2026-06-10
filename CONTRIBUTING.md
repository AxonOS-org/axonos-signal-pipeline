# Contributing to AxonOS Signal Pipeline

Thanks for your interest. This repository is the reference signal-pipeline
contract for AxonOS; contributions are held to the project's evidence
discipline (`docs/CLAIMS.md`).

## Ground rules

- **Honest claims only.** Do not add accuracy, latency, jitter, or power
  numbers to this repository. Measured (L2) results arrive as raw traces plus
  method under `docs/VALIDATION_PLAN.md`, clearly labelled — never as bare
  assertions. This repository asserts only machine-checkable (L1) claims.
- **Not a medical device.** Nothing here is clinical or regulatory guidance.
- **One canonical wire type.** Do not redefine `IntentObservation`; it is owned
  by `axonos-intent` in `axonos-kernel` (RFC-0006 §4). The pipeline stops at
  `ClassifierDecision`.

## Before you open a pull request

Run the standing gates locally (they mirror CI):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p axonos-pipeline-core --target thumbv7em-none-eabihf
python3 tools/validate_vectors.py
python3 tools/check_hygiene.py
```

## Changing conformance vectors

Vectors and fixtures are **generated**. Never hand-edit the JSON or the
generated `crates/axonos-pipeline-core/tests/data/vectors.rs`. To change them:

1. edit `tools/gen_test_vectors.py` (the single source of truth);
2. run `python3 tools/gen_test_vectors.py`;
3. commit **all** regenerated outputs together — including a regenerated
   `vectors/SHA256SUMS` — in the **same commit** (atomic-update rule). CI
   fails on any out-of-sync or unpinned change.

## Tooling

CI helper tools are intentionally written in **Python (standard library
only)**, committed and reviewable. They require no third-party packages and no
shell scripts in the repository.

## Licensing of contributions

By contributing you agree that your code is licensed **Apache-2.0 OR MIT** and
that data contributions (vectors, fixtures) are dedicated under **CC0-1.0**,
consistent with this repository's licensing.

---

The AxonOS Project · axonos.org · connect@axonos.org · security@axonos.org · github.com/AxonOS-org
