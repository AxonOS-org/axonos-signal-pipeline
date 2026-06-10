# Privacy Boundary

## The rule

> **Raw neural data must never cross the application boundary.**

Inside the pipeline, stages may stream raw samples. At the boundary that
separates the pipeline from application code, only a typed, consent-bound
decision may pass. Applications never receive raw frames, epochs, or feature
vectors.

## How v0.1.0 enforces it

1. **Sealed marker trait.** `boundary::BoundarySafe` is sealed; only
   `ClassifierDecision` implements it. `RawFrame`, `Epoch`, and `FeatureVector`
   do not, and downstream crates cannot add the impl. `assert_boundary_safe`
   accepts only `BoundarySafe` types. A `compile_fail` doctest demonstrates
   that passing a `RawFrame` does not compile.
2. **Crate-private raw access.** `RawFrame::raw_samples()` is `pub(crate)`:
   in-pipeline stages can read interleaved samples; application code outside
   the crate cannot reach them. The public accessor `sample(t, col)` is
   bounds-checked and element-wise.
3. **Redacted diagnostics.** `RawFrame`'s `Debug` prints header fields and
   `samples: "<redacted>"`. A test asserts raw values never appear in `Debug`
   output, so a stray log line cannot leak signal.
4. **No serialization of raw signal.** This crate has zero dependencies and
   derives no serializer for `RawFrame`. Raw frames cannot be accidentally
   serialized through `serde` from here.

## System-level conversion

At the kernel boundary, `ClassifierDecision` is converted into the canonical
`IntentObservation` wire type (RFC-0006 §4, `axonos-intent`) **under consent
gating** by `axonos-consent`. That interlock — not this crate — is what makes
an outgoing intent *consented*. Keeping a single canonical wire type, owned by
the kernel, prevents a parallel definition from drifting out of sync.

## What this boundary does NOT guarantee

- It does not stop a caller from copying raw samples through their own code
  before classification; it constrains **this crate's** surface, not the host.
- It is **not** consent enforcement. Compile-time type safety and consent are
  complementary: this crate provides the former, `axonos-consent` the latter.
- It makes no anonymization or differential-privacy claim about the contents
  of a `ClassifierDecision`.

---

The AxonOS Project · axonos.org · connect@axonos.org · security@axonos.org · github.com/AxonOS-org
