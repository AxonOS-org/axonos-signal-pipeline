# Calibration (design note — scheduled v0.5.0)

Calibration is **not implemented** in v0.1.0. This note records the intended
contract so the type surface added earlier does not contradict it later.

## Goal

Reduce inter-session and inter-subject covariate shift before classification,
and support a low-friction cold start.

## Intended components (v0.5.0)

- **Euclidean Alignment (EA).** Whiten epoch covariance by the inverse square
  root of the session-mean covariance, so per-session statistics are
  comparable. A pure function of the epochs in scope.
- **Session mean / drift update.** A deterministic running estimate of the
  reference statistic, with an explicit, vector-pinned update rule.
- **ZeroCalib skeleton.** The protocol surface for a minimal- or
  zero-calibration start, expressed as typed states, not as a performance
  claim.

## Constraints carried from the contract

- Every calibration step must be a **pure function of its declared inputs**
  (the v0.5.0 falsifier in [`VALIDATION_PLAN.md`](VALIDATION_PLAN.md)).
- The fixed-point path (v0.3.0) precedes calibration, so alignment runs on
  deterministic features.
- No calibration step may expose raw signal across the application boundary
  ([`PRIVACY_BOUNDARY.md`](PRIVACY_BOUNDARY.md)).

This document will be replaced by a normative section of
[`PIPELINE_CONTRACT.md`](PIPELINE_CONTRACT.md) when v0.5.0 lands.

---

The AxonOS Project · axonos.org · connect@axonos.org · security@axonos.org · github.com/AxonOS-org
