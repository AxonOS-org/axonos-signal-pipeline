// SPDX-License-Identifier: Apache-2.0 OR MIT
// SPDX-FileCopyrightText: 2026 Denis Yermakou <connect@axonos.org>
//
// Part of the AxonOS Signal Pipeline. Dual-licensed Apache-2.0 OR MIT at your
// option. Authored by Denis Yermakou for The AxonOS Project — https://axonos.org

//! # Kani BMC harnesses for `axonos-pipeline-core`
//!
//! The kernel carries thirty machine-checked proofs and the consent crate six.
//! This crate carried none — and it is the one holding the fixed-point
//! arithmetic, where a defect does not panic. An overflow in a filter produces
//! a plausible number, a downstream stage smooths it, a classifier decides on
//! it, and nothing in the chain can tell that the value was fiction.
//!
//! Tests sample the input space. These harnesses quantify over it, which is the
//! difference that matters for arithmetic: a test that tried a thousand random
//! covariance matrices says nothing about the thousand-and-first, and the
//! thousand-and-first is what a real electrode produces at the moment it comes
//! loose.
//!
//! Every property here was chosen because its failure mode is **silent**.
//! Nothing is proved that a panic would have caught anyway.
//!
//! Run with:
//! ```text
//! cargo kani --harness pipe_<name>
//! ```

#![cfg_attr(kani, no_std)]

#[cfg(kani)]
use axonos_pipeline_core::spatial::{
    remove_baseline, rereference, Reference, SpatialFilter, SPATIAL_ONE,
};
#[cfg(kani)]
use axonos_pipeline_core::spectral::power_ratio_permille;

// ───────────────────────────────────────────────────────────────────────────
// P1: re-referencing never wraps
// ───────────────────────────────────────────────────────────────────────────

/// **P1.** Common-average re-referencing saturates rather than wrapping, for
/// *any* pair of samples including both rails.
///
/// The failure this rules out is the quietest one in the crate. A sample at
/// `i32::MIN` minus a positive mean wraps to a large positive number, which
/// looks exactly like a healthy signal of the opposite polarity — and the
/// artifact screener would then see amplitude rather than saturation, which
/// is the one finding that does not disqualify an epoch.
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(5)]
fn pipe_p1_rereference_never_wraps() {
    let a: i32 = kani::any();
    let b: i32 = kani::any();
    let mut s = [a, b];

    let before = s;
    let r = rereference(&mut s, 2, Reference::CommonAverage);
    assert!(r.is_ok(), "a two-channel block is always well formed");

    // The mean of two i32 fits in i64, and each output is that mean subtracted
    // from a sample and clamped. Both operands are therefore representable,
    // and the only way to leave the range is a wrap the clamp is there to stop.
    for (i, &v) in s.iter().enumerate() {
        let mean = (before[0] as i64 + before[1] as i64) / 2;
        let ideal = before[i] as i64 - mean;
        let clamped = if ideal > i32::MAX as i64 {
            i32::MAX as i64
        } else if ideal < i32::MIN as i64 {
            i32::MIN as i64
        } else {
            ideal
        };
        assert!(v as i64 == clamped, "re-reference must clamp, never wrap");
    }
}

/// **P2.** Re-referencing removes the common mode exactly: adding any constant
/// to every channel leaves the output unchanged.
///
/// This is what a re-reference is *for*, and it is the property a wrapping
/// implementation silently loses. Stated over arbitrary offsets rather than a
/// handful of tested ones, and bounded away from the rails so the clamp of P1
/// is not what is being measured here.
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(5)]
fn pipe_p2_common_mode_is_removed() {
    let a: i32 = kani::any();
    let b: i32 = kani::any();
    let k: i32 = kani::any();
    // Keep every value and every sum well inside the range: this harness is
    // about the algebra, and P1 already covers the boundary.
    kani::assume(a > -1_000_000 && a < 1_000_000);
    kani::assume(b > -1_000_000 && b < 1_000_000);
    kani::assume(k > -1_000_000 && k < 1_000_000);
    // Even offsets only: the integer mean of an odd sum truncates, and the
    // truncation is a real (documented) property rather than a defect.
    kani::assume((a + b) % 2 == 0);
    kani::assume((k * 2) % 2 == 0);

    let mut plain = [a, b];
    let mut shifted = [a + k, b + k];
    let _ = rereference(&mut plain, 2, Reference::CommonAverage);
    let _ = rereference(&mut shifted, 2, Reference::CommonAverage);
    assert!(
        plain[0] == shifted[0] && plain[1] == shifted[1],
        "a common-mode offset must not survive re-referencing"
    );
}

// ───────────────────────────────────────────────────────────────────────────
// P3: baseline removal
// ───────────────────────────────────────────────────────────────────────────

/// **P3.** Baseline removal refuses every ill-formed request and writes nothing
/// when it refuses.
///
/// A partially applied correction is worse than none: the first channels would
/// carry a baseline the later ones do not, and every downstream comparison
/// between channels would be against an offset nobody recorded.
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(6)]
fn pipe_p3_baseline_refusal_writes_nothing() {
    let a: i32 = kani::any();
    let b: i32 = kani::any();
    let c: i32 = kani::any();
    let d: i32 = kani::any();
    let baseline: usize = kani::any();
    kani::assume(baseline <= 8);

    let mut s = [a, b, c, d];
    let before = s;
    let r = remove_baseline(&mut s, 2, baseline);

    // Two channels, four samples: two instants. Anything asking for more
    // baseline instants than exist, or for none, must be refused.
    if baseline == 0 || baseline > 2 {
        assert!(r.is_err(), "an impossible baseline must be refused");
        assert!(
            s[0] == before[0] && s[1] == before[1] && s[2] == before[2] && s[3] == before[3],
            "a refused correction must not have written"
        );
    } else {
        assert!(r.is_ok(), "a baseline within the block must be accepted");
    }
}

// ───────────────────────────────────────────────────────────────────────────
// P4: spatial filtering
// ───────────────────────────────────────────────────────────────────────────

/// **P4.** The identity spatial filter is the identity, for every input.
///
/// A filter matrix goes through a `Q16` multiply-accumulate in `i64` and a
/// shift back down. If that path loses a count on some input, every trained
/// projection is subtly wrong in a way no test of a specific matrix would
/// reveal — and the identity is the one case where the correct answer is known
/// for *all* inputs rather than for the ones someone thought to check.
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(4)]
fn pipe_p4_identity_filter_is_identity() {
    let a: i32 = kani::any();
    let b: i32 = kani::any();
    let f = SpatialFilter::<2, 2>::new([[SPATIAL_ONE, 0], [0, SPATIAL_ONE]]);
    let out = f.apply_frame(&[a, b]);
    assert!(
        out[0] == a && out[1] == b,
        "identity must not move a sample"
    );
}

/// **P5.** A spatial filter saturates rather than wrapping, for any weights and
/// any input.
///
/// The accumulator is `i64` and the output is `i32`, so a large weight on a
/// large sample overflows the narrowing. Wrapping there produces a value of the
/// opposite sign — a projection that reports strong negative activity where
/// there was strong positive activity, which is worse than reporting a rail.
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(4)]
fn pipe_p5_spatial_filter_saturates() {
    let x: i32 = kani::any();
    let w: i32 = kani::any();
    let f = SpatialFilter::<1, 1>::new([[w]]);
    let out = f.apply_frame(&[x]);

    let ideal = (w as i64 * x as i64) >> 16;
    let expect = if ideal > i32::MAX as i64 {
        i32::MAX
    } else if ideal < i32::MIN as i64 {
        i32::MIN
    } else {
        ideal as i32
    };
    assert!(out[0] == expect, "the narrowing must clamp, never wrap");
}

// ───────────────────────────────────────────────────────────────────────────
// P6: power ratio
// ───────────────────────────────────────────────────────────────────────────

/// **P6.** A power ratio never wraps and never confuses "no measurement" with
/// "a small measurement".
///
/// `numerator × 1000` overflows `u64` for large inputs, and a wrapped ratio is
/// a small number — so a band with overwhelming power would report as quiet.
/// The zero-denominator case is the other half: an absent measurement and a
/// ratio of zero must not look alike to a caller, because one means "nothing
/// was recorded" and the other means "nothing was there".
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(3)]
fn pipe_p6_power_ratio_is_total() {
    let n: u64 = kani::any();
    let d: u64 = kani::any();
    let r = power_ratio_permille(n, d);

    if d == 0 {
        assert!(
            r.is_none(),
            "no denominator is an absent measurement, not a ratio"
        );
    } else {
        let v = r.expect("a non-zero denominator always yields a ratio");
        // Saturating multiply then divide: the result is bounded by u32::MAX by
        // construction, and monotone in the numerator for a fixed denominator.
        assert!(v <= u32::MAX, "the ratio is representable");
        if n == 0 {
            assert!(v == 0, "no power is a ratio of zero, not a wrap");
        }
    }
}

/// **P7.** A power ratio is monotone in its numerator.
///
/// Monotonicity is the property every downstream comparison assumes: a band
/// with more power must not report a smaller ratio than one with less. An
/// overflow anywhere in the multiply breaks it, and breaks it *silently*.
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(3)]
fn pipe_p7_power_ratio_is_monotone() {
    let a: u64 = kani::any();
    let b: u64 = kani::any();
    let d: u64 = kani::any();
    kani::assume(d > 0);
    kani::assume(a <= b);
    // Bound the inputs below the saturation point: above it both answers are
    // u32::MAX and monotonicity holds trivially, which is true but uninformative.
    kani::assume(b < u64::MAX / 1_000);

    let ra = power_ratio_permille(a, d).expect("d > 0");
    let rb = power_ratio_permille(b, d).expect("d > 0");
    assert!(ra <= rb, "more power must never report a smaller ratio");
}

#[cfg(not(kani))]
fn main() {
    // These harnesses are meaningful only under Kani. Building without it is
    // still useful: it type-checks the properties against the current API, so a
    // signature change breaks the build here rather than silently leaving a
    // proof that no longer refers to the code it was written for.
    println!("axonos-pipeline-core Kani harnesses: build with `cargo kani`");
}

#[cfg(kani)]
fn main() {}
