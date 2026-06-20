# Safety Notes

## What this is

`axonos-signal-pipeline` is an **engineering demonstrator** for deterministic,
auditable fixed-point DSP. Read this before drawing any operational conclusion
from it.

- It is **not a medical device**.
- It is **not for diagnosis**.
- It is **not for treatment**.
- It is **not for stimulation control**.
- It is **not for patient use**.
- It requires **no real neural data**: every fixture is synthetic
  ([`LIMITATIONS.md`](LIMITATIONS.md)).
- It makes **no measured-performance claim** (no accuracy, latency, jitter, or
  power figures) — see [`CLAIMS.md`](CLAIMS.md).

The DSP filters (DC blocker, notch, band-pass) are single second-order sections
with engineering-chosen coefficients and **no certified frequency response and
no clinical validation** ([`DSP_SPEC.md`](DSP_SPEC.md)).

## Safety-oriented design properties

- `#![no_std]` (outside tests), `#![forbid(unsafe_code)]`, zero dependencies.
- **Saturating** integer arithmetic on every DSP output — no wrapping overflow,
  no panic on malformed input, no division by zero, bounded loops only.
- **No raw signal output by default.** Only the terminal `ClassifierDecision`
  is boundary-safe; raw and intermediate buffers cannot cross the application
  boundary through this crate's API ([`PRIVACY_BOUNDARY.md`](PRIVACY_BOUNDARY.md)).
- **Deterministic and replayable.** Every behavioural rule is pinned by a
  conformance vector and reproduced bit-for-bit; filters also pin a
  `state_hash` ([`VALIDATION_PLAN.md`](VALIDATION_PLAN.md)).
- **No network, no telemetry, no accounts, no analytics, no cloud, no hidden
  runtime behaviour.**

## Security posture (summary)

The threat model and reporting process are in
[`../SECURITY.md`](../SECURITY.md); the boundary guarantees and their limits are
in [`PRIVACY_BOUNDARY.md`](PRIVACY_BOUNDARY.md). In brief, the relevant risks
and the mitigations this crate provides:

| Risk | Mitigation in this crate |
|---|---|
| Raw-signal leakage across the boundary | Sealed `BoundarySafe`; redacted `Debug`; only `ClassifierDecision` crosses |
| Feature-inversion / reconstruction | No feature path ships yet; output is reduced, never raw |
| Artifact spoofing | Deterministic, documented detection rules; thresholds are explicit constants/config |
| Replay tampering | Bit-exact vectors + integrity manifest + per-filter `state_hash` |
| Malicious / malformed sample stream | Length and rate validation; explicit typed errors; no panic |
| Denial of service via invalid sample rate | Unsupported rates rejected with `UnsupportedSampleRate`, never run undesigned |
| Integer overflow | `i64` accumulation, saturation into `i32`, no unchecked arithmetic |

These are engineering mitigations within a demonstrator, **not** a security or
clinical certification.

---

The AxonOS Project · axonos.org · connect@axonos.org · security@axonos.org · github.com/AxonOS-org
