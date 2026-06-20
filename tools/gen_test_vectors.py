#!/usr/bin/env python3
"""Single source of truth for v0.2.4 conformance vectors.

Generates, deterministically and from first principles:
  vectors/pipeline-vectors-v0.2.4.json
  fixtures/synthetic/frame-0001.json
  crates/axonos-pipeline-core/tests/data/vectors.rs
  vectors/SHA256SUMS   (covers the JSON artifacts above)

The arithmetic here mirrors the normative definitions in
docs/PIPELINE_CONTRACT.md; the Rust crate is tested against these values.
Editing any vector means re-running this tool and committing ALL outputs
together with the regenerated SHA256SUMS (atomic-update rule).
"""

from __future__ import annotations

import hashlib
import json
import struct
from collections import OrderedDict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3
MASK64 = (1 << 64) - 1

VECTOR_VERSION = "0.6.0"
DATE = "2026-06-20"

# Fixture frame parameters (LCG: Numerical Recipes constants).
SEED = 0x000A0510
LCG_A, LCG_C, LCG_MOD = 1664525, 1013904223, 1 << 32
N_CHANNELS = 8
SAMPLES_PER_CHANNEL = 16
SEQ = 7
TIMESTAMP_US = 1711540800000000
RATE_HZ = 250
CHANNEL_MASK = 0x00FF
EPOCH_WINDOW, EPOCH_HOP = 4, 4
ADC24_MAX, ADC24_MIN = 8_388_607, -8_388_608

I32_MAX, I32_MIN = 2_147_483_647, -2_147_483_648


def fnv1a64(data: bytes) -> int:
    h = FNV_OFFSET
    for b in data:
        h = ((h ^ b) * FNV_PRIME) & MASK64
    return h


def lcg_samples(seed: int, n: int) -> list[int]:
    s, out = seed, []
    for _ in range(n):
        s = (LCG_A * s + LCG_C) % LCG_MOD
        out.append(((s >> 8) % 16001) - 8000)
    return out


def frame_checksum(seq, ts_us, rate_hz, mask, spc, samples) -> int:
    buf = bytearray()
    buf += struct.pack("<I", seq)
    buf += struct.pack("<Q", ts_us)
    buf += struct.pack("<I", rate_hz)
    buf += struct.pack("<H", mask)
    buf += struct.pack("<I", spc)
    for s in samples:
        buf += struct.pack("<i", s)
    return fnv1a64(bytes(buf))


def window_count(n: int, w: int, h: int):
    if w == 0 or h == 0 or w > n:
        return None
    return (n - w) // h + 1


# --- DSP oracle (bit-exact mirror of crates/axonos-pipeline-core/src/dsp.rs) ---


def sat_i32(value: int) -> int:
    """Saturating narrow into the i32 sample range."""
    return max(I32_MIN, min(I32_MAX, value))


def trunc_div(a: int, b: int) -> int:
    """Integer division truncated toward zero (matches Rust `i64 /`)."""
    q = abs(a) // abs(b)
    return -q if (a < 0) != (b < 0) else q


def remove_mean(xs: list[int]):
    """DC (mean) removal: returns (mean_removed, centred_samples)."""
    mean = trunc_div(sum(xs), len(xs))
    return mean, [sat_i32(x - mean) for x in xs]


def fir(xs: list[int], coeffs: list[int], shift: int) -> list[int]:
    """Causal FIR, zero initial state, i64 accumulator, round-half-up, saturated."""
    bias = (1 << (shift - 1)) if shift >= 1 else 0
    out = []
    for n in range(len(xs)):
        acc = 0
        for k, c in enumerate(coeffs):
            if n >= k:
                acc += c * xs[n - k]
        y = (acc + bias) >> shift if shift >= 1 else acc
        out.append(sat_i32(y))
    return out


# --- IIR filter oracle (bit-exact mirror of crates/.../src/filter.rs) ---

BIQUAD_SHIFT = 15
BIQUAD_BIAS = 1 << (BIQUAD_SHIFT - 1)
DC_R_DEFAULT = 32604  # Q15 of 0.995


def biquad_process(coeffs, xs):
    """Direct-Form-I biquad; returns (output, final_state=(x1,x2,y1,y2))."""
    b0, b1, b2, a1, a2 = coeffs
    x1 = x2 = y1 = y2 = 0
    out = []
    for x in xs:
        acc = b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2
        y = sat_i32((acc + BIQUAD_BIAS) >> BIQUAD_SHIFT)
        x2, x1 = x1, x
        y2, y1 = y1, y
        out.append(y)
    return out, (x1, x2, y1, y2)


def biquad_state_hash(coeffs, state):
    """FNV-1a 64 over [b0,b1,b2,a1,a2,x1,x2,y1,y2] as little-endian i32."""
    buf = b"".join(struct.pack("<i", v) for v in (*coeffs, *state))
    return fnv1a64(buf)


def dc_process(r, xs):
    """First-order DC blocker; returns (output, final_state=(x1,y1))."""
    x1 = y1 = 0
    out = []
    for x in xs:
        acc = ((x - x1) << BIQUAD_SHIFT) + r * y1
        y = sat_i32((acc + BIQUAD_BIAS) >> BIQUAD_SHIFT)
        x1, y1 = x, y
        out.append(y)
    return out, (x1, y1)


def dc_state_hash(r, state):
    """FNV-1a 64 over [r, x1, y1] as little-endian i32."""
    buf = b"".join(struct.pack("<i", v) for v in (r, *state))
    return fnv1a64(buf)


# Q15 coefficient tables — identical to src/filter.rs (RBJ cookbook, computed
# offline). Keyed by design label.
BIQUAD_DESIGNS = {
    "identity": (32768, 0, 0, 0, 0),
    "notch_50hz_250": (32257, -19936, 32257, -19936, 31745),
    "notch_50hz_500": (32450, -52505, 32450, -52505, 32132),
    "notch_60hz_250": (32232, -4048, 32232, -4048, 31696),
    "bandpass_motor_250": (6957, 0, -6957, -47759, 18854),
    "bandpass_attention_250": (2980, 0, -2980, -58676, 26809),
    "bandpass_safetywide_250": (10747, 0, -10747, -43487, 11274),
}

# Shared deterministic filter test signal in ADC counts: a +120000 DC offset
# (exercises the DC blocker) plus impulses and a swing (exercise the biquads).
FILTER_SIGNAL = [
    120_000, 120_000, 1_120_000, 120_000, 120_000, -380_000, -380_000, 620_000,
    620_000, 120_000, 920_000, -680_000, 320_000, -180_000, 120_000, 1_620_000,
    -1_380_000, 120_000, 120_000, 420_000, -220_000, 120_000, 770_000, -530_000,
    120_000, 120_000, 220_000, -3_040_00, 120_000, 8_388_607, -8_388_608, 120_000,
]

FRAME_RATES = (250, 500)  # rate annotation only; coefficients embed the design


FNV_ANCHORS = [b"", b"a", b"axonos", b"AxonOS Signal Pipeline"]

WINDOW_CASES = [
    (16, 16, 1),
    (16, 8, 4),
    (16, 4, 4),
    (1000, 250, 125),
    (16, 32, 8),
    (16, 0, 1),
    (16, 4, 0),
]

ARTIFACT_CASES = [
    ([0, 100, -100, 950], 1000, 0),
    ([0, 1500, -2, 3], 1000, 1),
    ([ADC24_MAX, 0, 1], 1000, 2),
    ([2000, ADC24_MIN], 1000, 2),
    ([-1001, 0], 1000, 1),
]

MASK_CASES = [
    (0x00FF, 0, 0),
    (0x00FF, 7, 7),
    (0x00FF, 8, None),
    (0x00A5, 0, 0),
    (0x00A5, 2, 1),
    (0x00A5, 5, 2),
    (0x00A5, 7, 3),
    (0x00A5, 1, None),
]

# DC removal: chosen to exercise truncation-toward-zero (asymmetry vs floor),
# single element, all-zero, negative mean, and i32 saturation.
DC_CASES = [
    [10, 20, 30, 40],
    [-7, 0],
    [5],
    [0, 0, 0, 0],
    [-100, -100, -100],
    [I32_MAX, I32_MAX, I32_MIN],
]

# FIR: moving-average rounding, identity, signed-shift on negatives, mixed
# coefficients, and accumulator overflow forcing i32 saturation.
FIR_CASES = [
    ([4, 8, 12, 16], [1, 1, 1, 1], 2),
    ([1, 2, 3], [1], 0),
    ([-4, -8, -12, -16], [1, 1, 1, 1], 2),
    ([100, -100, 50], [2, -1], 1),
    ([2000], [2_000_000], 0),
]


# --- Feature oracle (bit-exact mirror of crates/.../src/feature.rs) ---


def isqrt_py(x: int) -> int:
    return int(x) ** 0  # placeholder, replaced below


def feat_isqrt(x: int) -> int:
    import math
    return math.isqrt(x)  # floor sqrt; matches the bit-by-bit i64 routine


def feat_log2_q16(x: int) -> int:
    if x == 0:
        return 0
    int_part = x.bit_length() - 1
    if int_part <= 32:
        m = x << (32 - int_part)
    else:
        m = x >> (int_part - 32)
    result = int_part << 16
    bit = 1 << 15
    while bit > 0:
        m = (m * m) >> 32
        if m >= (1 << 33):
            result += bit
            m >>= 1
        bit >>= 1
    return result


def feat_variance(xs: list[int]) -> int:
    n = len(xs)
    mean = trunc_div(sum(xs), n)
    acc = sum((x - mean) ** 2 for x in xs)
    return acc // n  # acc >= 0


def feat_log_variance_q16(xs: list[int]) -> int:
    return feat_log2_q16(feat_variance(xs))


def feat_rms(xs: list[int]) -> int:
    return feat_isqrt(feat_variance(xs))


def feat_abs_mean(xs: list[int]) -> int:
    n = len(xs)
    return sum(abs(x) for x in xs) // n


def feat_zero_crossings(xs: list[int]) -> int:
    c = 0
    for a, b in zip(xs, xs[1:]):
        if (a < 0 and b > 0) or (a > 0 and b < 0):
            c += 1
    return c


# --- Classifier oracle (bit-exact mirror of crates/.../src/classify.rs) ---

CONF_MAX = 1000
U64_MAX = (1 << 64) - 1
I64_MIN, I64_MAX = -(1 << 63), (1 << 63) - 1


def cls_distance_sq(feature, mean):
    acc = sum((f - m) ** 2 for f, m in zip(feature, mean))
    return min(acc, U64_MAX)


def cls_classify_mdm(feature, class_means, abstain):
    best, best_d, second_d = 0, U64_MAX, U64_MAX
    for i, m in enumerate(class_means):
        d = cls_distance_sq(feature, m)
        if d < best_d:
            second_d, best_d, best = best_d, d, i
        elif d < second_d:
            second_d = d
    if best_d == 0 and second_d == 0:
        conf = 0
    else:
        conf = ((second_d - best_d) * CONF_MAX) // (second_d + best_d)
    conf &= 0xFFFF
    if conf < abstain:
        return ("NoIntent",)
    return ("Intent", best & 0xFF, conf)


def cls_lda_score(feature, weights, bias):
    acc = bias + sum(f * w for f, w in zip(feature, weights))
    return max(I64_MIN, min(I64_MAX, acc))


def cls_classify_lda_binary(feature, weights, bias, band):
    score = cls_lda_score(feature, weights, bias)
    a = abs(score)
    if a < band:
        return ("NoIntent",), score
    b = max(band, 1)
    conf = ((a * CONF_MAX) // (a + b)) & 0xFFFF
    cls = 1 if score >= 0 else 0
    return ("Intent", cls, conf), score


# --- Calibration oracle (bit-exact mirror of crates/.../src/calibrate.rs) ---

WSHIFT = 16
WONE = 1 << WSHIFT


def cal_covariance(channels):
    C = len(channels)
    n = len(channels[0])
    means = [trunc_div(sum(ch), n) for ch in channels]
    out = [[0] * C for _ in range(C)]
    for i in range(C):
        for j in range(i, C):
            acc = sum(
                (channels[i][t] - means[i]) * (channels[j][t] - means[j])
                for t in range(n)
            )
            v = trunc_div(acc, n)
            out[i][j] = v
            out[j][i] = v
    return out


def cal_sqrt_q16(x):
    if x <= 0:
        return 0
    import math
    return math.isqrt(x << WSHIFT)


def cal_whiten_cholesky(r):
    C = len(r)
    a = [[r[i][j] << WSHIFT for j in range(C)] for i in range(C)]
    l = [[0] * C for _ in range(C)]
    for j in range(C):
        diag = a[j][j]
        for k in range(j):
            diag -= (l[j][k] * l[j][k]) >> WSHIFT
        if diag <= 0:
            return None
        ljj = cal_sqrt_q16(diag)
        if ljj == 0:
            return None
        l[j][j] = ljj
        for i in range(j + 1, C):
            sm = a[i][j]
            for k in range(j):
                sm -= (l[i][k] * l[j][k]) >> WSHIFT
            l[i][j] = trunc_div(sm << WSHIFT, ljj)
    w = [[0] * C for _ in range(C)]
    for col in range(C):
        for i in range(C):
            rhs = WONE if i == col else 0
            for k in range(i):
                rhs -= (l[i][k] * w[k][col]) >> WSHIFT
            if l[i][i] == 0:
                return None
            w[i][col] = trunc_div(rhs << WSHIFT, l[i][i])
    return w


def cal_align(w, cov):
    C = len(w)
    tmp = [[sum(w[i][k] * cov[k][j] for k in range(C)) for j in range(C)] for i in range(C)]
    out = [[sum((tmp[i][k] * w[j][k]) >> WSHIFT for k in range(C)) for j in range(C)] for i in range(C)]
    return out


# Deterministic feature/classifier/calibration test inputs.
FEATURE_SIGNALS = [
    [-2, -1, 0, 1, 2],
    [100, -100, 100, -100, 100, -100],
    [0, 0, 0, 0],
    [8_388_607, -8_388_608, 0, 1000, -1000],
    [5, 5, 5, 5, 5],
    [-3, 7, -3, 7, -3, 7, -3],
]
LOG2_DIRECT = [0, 1, 2, 3, 256, 1000, 1 << 40]
ISQRT_DIRECT = [0, 1, 2, 15, 16, 255, 256, 1_000_000, (1 << 48) - 1]

MDM_CASES = [
    ([90, 95], [[0, 0], [100, 100]], 0),
    ([50, 0], [[0, 0], [100, 0]], 1),
    ([10, 10, 10], [[0, 0, 0], [20, 20, 20], [10, 10, 11]], 100),
    ([5, 5], [[5, 5]], 0),
    ([1000, -1000], [[0, 0], [900, -900], [-900, 900]], 50),
]
LDA_CASES = [
    ([10, 5], [2, -1], 0, 5),
    ([1, 1], [1, -1], 0, 5),
    ([-3, 2], [4, 1], -10, 3),
    ([100, 100, 100], [1, 1, 1], -250, 20),
]

COV_2CH = [[-2, -1, 1, 2], [-2, -1, 1, 2]]
COV_3CH = [[10, -10, 10, -10], [5, 5, -5, -5], [1, 2, 3, 4]]
WHITEN_2X2 = [[4, 1], [1, 3]]
WHITEN_3X3 = [[6, 2, 1], [2, 5, 2], [1, 2, 7]]
WHITEN_NONPD = [[1, 2], [2, 1]]


def build() -> "OrderedDict[str, bytes]":
    samples = lcg_samples(SEED, N_CHANNELS * SAMPLES_PER_CHANNEL)
    checksum = frame_checksum(
        SEQ, TIMESTAMP_US, RATE_HZ, CHANNEL_MASK, SAMPLES_PER_CHANNEL, samples
    )

    # Filter vectors: run each design / the DC blocker once over FILTER_SIGNAL.
    filter_biquad = []
    for label, coeffs in BIQUAD_DESIGNS.items():
        out, state = biquad_process(coeffs, FILTER_SIGNAL)
        filter_biquad.append((label, coeffs, out, biquad_state_hash(coeffs, state)))
    dc_out, dc_state = dc_process(DC_R_DEFAULT, FILTER_SIGNAL)
    dc_hash = dc_state_hash(DC_R_DEFAULT, dc_state)

    fixture = OrderedDict(
        [
            (
                "_meta",
                OrderedDict(
                    [
                        ("title", "Synthetic acquisition frame fixture 0001"),
                        ("producer", "axonos-signal-pipeline tools/gen_test_vectors.py"),
                        ("maintainer", "The AxonOS Project <connect@axonos.org>"),
                        ("license", "CC0-1.0"),
                        ("synthetic", True),
                        (
                            "generator",
                            OrderedDict(
                                [
                                    ("kind", "LCG (Numerical Recipes)"),
                                    ("seed", SEED),
                                    ("a", LCG_A),
                                    ("c", LCG_C),
                                    ("mod", "2^32"),
                                    ("sample", "((state >> 8) % 16001) - 8000"),
                                ]
                            ),
                        ),
                    ]
                ),
            ),
            (
                "frame",
                OrderedDict(
                    [
                        ("seq", SEQ),
                        ("timestamp_us", TIMESTAMP_US),
                        ("sample_rate_hz", RATE_HZ),
                        ("channel_mask", CHANNEL_MASK),
                        ("samples_per_channel", SAMPLES_PER_CHANNEL),
                        ("storage", "time-major interleaved, column-compacted"),
                        ("unit", "ADC counts (24-bit, sign-extended)"),
                        ("samples", samples),
                    ]
                ),
            ),
        ]
    )

    vectors = OrderedDict()
    vectors["_meta"] = OrderedDict(
        [
            ("title", "AxonOS Signal Pipeline — Conformance Test Vectors"),
            ("artifact", "pipeline-core type-contract conformance"),
            ("vector_version", VECTOR_VERSION),
            ("producer", "axonos-signal-pipeline"),
            ("consumer", "any implementation of docs/PIPELINE_CONTRACT.md"),
            ("maintainer", "The AxonOS Project <connect@axonos.org>"),
            ("security_contact", "security@axonos.org"),
            ("license", "CC0-1.0"),
            ("status", "pre-clinical engineering artifact; not a medical device"),
            ("date", DATE),
            (
                "notes",
                [
                    "Consumed by crates/axonos-pipeline-core/tests/conformance.rs via the",
                    "generated tests/data/vectors.rs (kept in sync by tools/validate_vectors.py).",
                    "vector_version is the version of THIS vector set and is independent of",
                    "the crate version. Any change to vectors or fixtures requires re-running",
                    "tools/gen_test_vectors.py and committing all outputs together with the",
                    "regenerated vectors/SHA256SUMS (atomic-update rule).",
                    "DSP sections (dc_remove, fir) are integer fixed-point and bit-exact;",
                    "see docs/PIPELINE_CONTRACT.md §9.",
                    "Stateful IIR sections (dc_blocker, biquad) run over filter_signal;",
                    "expected output and the post-run state_hash are pinned, see §9.3-§9.4.",
                ],
            ),
        ]
    )
    vectors["fnv1a64"] = [
        OrderedDict(
            [
                ("id", f"PV-FNV-{i + 1:03d}"),
                ("input_utf8", a.decode("ascii")),
                ("digest", f"0x{fnv1a64(a):016x}"),
            ]
        )
        for i, a in enumerate(FNV_ANCHORS)
    ]
    vectors["frame_checksum"] = [
        OrderedDict(
            [
                ("id", "PV-FRAME-001"),
                ("fixture", "fixtures/synthetic/frame-0001.json"),
                ("algorithm", "docs/PIPELINE_CONTRACT.md §3 (FNV-1a 64, little-endian)"),
                ("digest", f"0x{checksum:016x}"),
            ]
        )
    ]
    vectors["window_count"] = [
        OrderedDict(
            [
                ("id", f"PV-WIN-{i + 1:03d}"),
                ("samples_per_channel", n),
                ("window", w),
                ("hop", h),
                ("expected", window_count(n, w, h)),
            ]
        )
        for i, (n, w, h) in enumerate(WINDOW_CASES)
    ]
    vectors["artifact_scan"] = [
        OrderedDict(
            [
                ("id", f"PV-ART-{i + 1:03d}"),
                ("samples", s),
                ("threshold_counts", t),
                ("expected", ["clean", "amplitude_exceeded", "saturated"][code]),
            ]
        )
        for i, (s, t, code) in enumerate(ARTIFACT_CASES)
    ]
    vectors["mask_column_of"] = [
        OrderedDict(
            [
                ("id", f"PV-MASK-{i + 1:03d}"),
                ("mask_bits", f"0x{bits:04x}"),
                ("channel", ch),
                ("expected_column", col),
            ]
        )
        for i, (bits, ch, col) in enumerate(MASK_CASES)
    ]
    vectors["dc_remove"] = [
        OrderedDict(
            [
                ("id", f"PV-DC-{i + 1:03d}"),
                ("input", xs),
                ("mean_removed", remove_mean(xs)[0]),
                ("expected", remove_mean(xs)[1]),
            ]
        )
        for i, xs in enumerate(DC_CASES)
    ]
    vectors["fir"] = [
        OrderedDict(
            [
                ("id", f"PV-FIR-{i + 1:03d}"),
                ("input", xs),
                ("coeffs", c),
                ("shift", sh),
                ("expected", fir(xs, c, sh)),
            ]
        )
        for i, (xs, c, sh) in enumerate(FIR_CASES)
    ]
    vectors["filter_signal"] = FILTER_SIGNAL
    vectors["biquad"] = [
        OrderedDict(
            [
                ("id", f"PV-BIQ-{i + 1:03d}"),
                ("design", label),
                ("coeffs_q15", list(coeffs)),
                ("input", "see filter_signal"),
                ("expected", out),
                ("state_hash", f"0x{shash:016x}"),
            ]
        )
        for i, (label, coeffs, out, shash) in enumerate(filter_biquad)
    ]
    vectors["dc_blocker"] = [
        OrderedDict(
            [
                ("id", "PV-DCB-001"),
                ("r_q15", DC_R_DEFAULT),
                ("input", "see filter_signal"),
                ("expected", dc_out),
                ("state_hash", f"0x{dc_hash:016x}"),
            ]
        )
    ]

    def _dec_json(dec):
        if dec[0] == "NoIntent":
            return "NoIntent"
        return f"Intent(class={dec[1]}, confidence_permille={dec[2]})"

    vectors["feature"] = [
        OrderedDict(
            [
                ("id", f"PV-FEAT-{i + 1:03d}"),
                ("signal", sig),
                ("variance", feat_variance(sig)),
                ("log_variance_q16", feat_log_variance_q16(sig)),
                ("rms", feat_rms(sig)),
                ("abs_mean", feat_abs_mean(sig)),
                ("zero_crossings", feat_zero_crossings(sig)),
            ]
        )
        for i, sig in enumerate(FEATURE_SIGNALS)
    ]
    vectors["log2_q16"] = [
        OrderedDict(
            [("id", f"PV-LOG2-{i + 1:03d}"), ("input", x), ("expected_q16", feat_log2_q16(x))]
        )
        for i, x in enumerate(LOG2_DIRECT)
    ]
    vectors["isqrt"] = [
        OrderedDict(
            [("id", f"PV-ISQRT-{i + 1:03d}"), ("input", x), ("expected", feat_isqrt(x))]
        )
        for i, x in enumerate(ISQRT_DIRECT)
    ]
    vectors["classify_mdm"] = [
        OrderedDict(
            [
                ("id", f"PV-MDM-{i + 1:03d}"),
                ("feature", feat),
                ("class_means", means),
                ("abstain_below_permille", ab),
                ("expected", _dec_json(cls_classify_mdm(feat, means, ab))),
            ]
        )
        for i, (feat, means, ab) in enumerate(MDM_CASES)
    ]
    vectors["classify_lda"] = [
        OrderedDict(
            [
                ("id", f"PV-LDA-{i + 1:03d}"),
                ("feature", feat),
                ("weights", w),
                ("bias", b),
                ("score", cls_lda_score(feat, w, b)),
                ("abstain_band", band),
                ("expected", _dec_json(cls_classify_lda_binary(feat, w, b, band)[0])),
            ]
        )
        for i, (feat, w, b, band) in enumerate(LDA_CASES)
    ]
    vectors["covariance"] = [
        OrderedDict(
            [("id", "PV-COV-001"), ("channels", COV_2CH), ("expected", cal_covariance(COV_2CH))]
        ),
        OrderedDict(
            [("id", "PV-COV-002"), ("channels", COV_3CH), ("expected", cal_covariance(COV_3CH))]
        ),
    ]
    _w2 = cal_whiten_cholesky(WHITEN_2X2)
    _w3 = cal_whiten_cholesky(WHITEN_3X3)
    vectors["whiten_cholesky"] = [
        OrderedDict(
            [
                ("id", "PV-WHIT-001"),
                ("reference", WHITEN_2X2),
                ("whitener_q16", _w2),
                ("align_q16", cal_align(_w2, WHITEN_2X2)),
                ("note", "align(W,R) ~ identity*65536 (Q16); exact value pinned"),
            ]
        ),
        OrderedDict(
            [
                ("id", "PV-WHIT-002"),
                ("reference", WHITEN_3X3),
                ("whitener_q16", _w3),
                ("align_q16", cal_align(_w3, WHITEN_3X3)),
            ]
        ),
        OrderedDict(
            [
                ("id", "PV-WHIT-003"),
                ("reference", WHITEN_NONPD),
                ("whitener_q16", None),
                ("note", "not positive-definite -> None"),
            ]
        ),
    ]
    def rs_i32_array(vals):
        lines, row = [], []
        for v in vals:
            row.append(str(v))
            if len(row) == 12:
                lines.append("    " + ", ".join(row) + ",")
                row = []
        if row:
            lines.append("    " + ", ".join(row) + ",")
        return "\n".join(lines)

    def rs_opt(v):
        return "None" if v is None else f"Some({v})"

    def rs_slice(vals):
        return "&[" + ", ".join(str(v) for v in vals) + "]"

    fnv_rows = "\n".join(
        f'    (b"{a.decode("ascii")}", 0x{fnv1a64(a):016x}),' for a in FNV_ANCHORS
    )
    win_rows = "\n".join(
        f"    ({n}usize, {w}usize, {h}usize, {rs_opt(window_count(n, w, h))}),"
        for (n, w, h) in WINDOW_CASES
    )
    art_rows = "\n".join(
        f"    (&[{', '.join(map(str, s))}], {t}, {code}),"
        for (s, t, code) in ARTIFACT_CASES
    )
    mask_rows = "\n".join(
        f"    (0x{bits:04x}, {ch}, {rs_opt(col)})," for (bits, ch, col) in MASK_CASES
    )
    dc_rows = "\n".join(
        f"    ({rs_slice(xs)}, {remove_mean(xs)[0]}, {rs_slice(remove_mean(xs)[1])}),"
        for xs in DC_CASES
    )
    fir_rows = "\n".join(
        f"    ({rs_slice(xs)}, {rs_slice(c)}, {sh}, {rs_slice(fir(xs, c, sh))}),"
        for (xs, c, sh) in FIR_CASES
    )
    filter_signal_rs = rs_i32_array(FILTER_SIGNAL)
    biquad_rows = "\n".join(
        f'    ("{label}", [{", ".join(map(str, coeffs))}], '
        f"{rs_slice(out)}, 0x{shash:016x}),"
        for (label, coeffs, out, shash) in filter_biquad
    )
    dc_blocker_rows = f"    ({DC_R_DEFAULT}, {rs_slice(dc_out)}, 0x{dc_hash:016x}),"

    def rs_mat(m):
        return "[" + ", ".join("[" + ", ".join(str(v) for v in row) + "]" for row in m) + "]"

    def rs_sos(lol):
        return "&[" + ", ".join(rs_slice(x) for x in lol) + "]"

    def _dec_rs(dec):
        return "(-1, 0)" if dec[0] == "NoIntent" else f"({dec[1]}, {dec[2]})"

    feature_rows = "\n".join(
        f"    ({rs_slice(s)}, {feat_variance(s)}, {feat_log_variance_q16(s)}, "
        f"{feat_rms(s)}, {feat_abs_mean(s)}, {feat_zero_crossings(s)}),"
        for s in FEATURE_SIGNALS
    )
    log2_rows = "\n".join(f"    ({x}, {feat_log2_q16(x)})," for x in LOG2_DIRECT)
    isqrt_rows = "\n".join(f"    ({x}, {feat_isqrt(x)})," for x in ISQRT_DIRECT)
    mdm_rows = "\n".join(
        f"    ({rs_slice(feat)}, {rs_sos(means)}, {ab}, "
        f"{_dec_rs(cls_classify_mdm(feat, means, ab))[1:-1]}),"
        for (feat, means, ab) in MDM_CASES
    )
    lda_rows = "\n".join(
        f"    ({rs_slice(feat)}, {rs_slice(w)}, {b}, {cls_lda_score(feat, w, b)}, {band}, "
        f"{_dec_rs(cls_classify_lda_binary(feat, w, b, band)[0])[1:-1]}),"
        for (feat, w, b, band) in LDA_CASES
    )
    cov2 = cal_covariance(COV_2CH)
    cov3 = cal_covariance(COV_3CH)
    w2 = cal_whiten_cholesky(WHITEN_2X2)
    w3 = cal_whiten_cholesky(WHITEN_3X3)
    al2 = cal_align(w2, WHITEN_2X2)
    al3 = cal_align(w3, WHITEN_3X3)
    data_rs = f"""// @generated by tools/gen_test_vectors.py from
// vectors/pipeline-vectors-v{VECTOR_VERSION}.json — DO NOT EDIT.
// CI re-generates and diffs this file (tools/validate_vectors.py).

const FNV_VECTORS: &[(&[u8], u64)] = &[
{fnv_rows}
];

const FIXTURE_SEQ: u32 = {SEQ};
const FIXTURE_TIMESTAMP_US: u64 = {TIMESTAMP_US};
const FIXTURE_RATE_HZ: u32 = {RATE_HZ};
const FIXTURE_CHANNEL_MASK: u16 = 0x{CHANNEL_MASK:04x};
const FIXTURE_CHANNEL_COUNT: usize = {N_CHANNELS};
const FIXTURE_SAMPLES_PER_CHANNEL: usize = {SAMPLES_PER_CHANNEL};
const FIXTURE_FRAME_CHECKSUM: u64 = 0x{checksum:016x};
const FIXTURE_SAMPLES: &[i32] = &[
{rs_i32_array(samples)}
];

const EPOCH_CASE_WINDOW: usize = {EPOCH_WINDOW};
const EPOCH_CASE_HOP: usize = {EPOCH_HOP};

const WINDOW_CASES: &[(usize, usize, usize, Option<usize>)] = &[
{win_rows}
];

const ARTIFACT_CASES: &[(&[i32], i32, u8)] = &[
{art_rows}
];

const MASK_CASES: &[(u16, u8, Option<usize>)] = &[
{mask_rows}
];

type DcCase = (&'static [i32], i32, &'static [i32]);
type FirCase = (&'static [i32], &'static [i32], u32, &'static [i32]);

const DC_CASES: &[DcCase] = &[
{dc_rows}
];

const FIR_CASES: &[FirCase] = &[
{fir_rows}
];

const FILTER_SIGNAL: &[i32] = &[
{filter_signal_rs}
];

type BiquadCase = (&'static str, [i32; 5], &'static [i32], u64);
type DcBlockerCase = (i32, &'static [i32], u64);

const BIQUAD_CASES: &[BiquadCase] = &[
{biquad_rows}
];

const DC_BLOCKER_CASES: &[DcBlockerCase] = &[
{dc_blocker_rows}
];

type FeatureCase = (&'static [i32], u64, i32, u32, u32, u32);
const FEATURE_CASES: &[FeatureCase] = &[
{feature_rows}
];

const LOG2_CASES: &[(u64, i32)] = &[
{log2_rows}
];

const ISQRT_CASES: &[(u64, u64)] = &[
{isqrt_rows}
];

type MdmCase = (&'static [i32], &'static [&'static [i32]], u16, i32, u16);
const MDM_CASES: &[MdmCase] = &[
{mdm_rows}
];

type LdaCase = (&'static [i32], &'static [i32], i64, i64, i64, i32, u16);
const LDA_CASES: &[LdaCase] = &[
{lda_rows}
];

const COV_2CH_CH: &[&[i32]] = {rs_sos(COV_2CH)};
const COV_2CH_EXPECT: [[i64; 2]; 2] = {rs_mat(cov2)};
const COV_3CH_CH: &[&[i32]] = {rs_sos(COV_3CH)};
const COV_3CH_EXPECT: [[i64; 3]; 3] = {rs_mat(cov3)};

const WHITEN_2X2_R: [[i64; 2]; 2] = {rs_mat(WHITEN_2X2)};
const WHITEN_2X2_W: [[i64; 2]; 2] = {rs_mat(w2)};
const WHITEN_2X2_ALIGN: [[i64; 2]; 2] = {rs_mat(al2)};
const WHITEN_3X3_R: [[i64; 3]; 3] = {rs_mat(WHITEN_3X3)};
const WHITEN_3X3_W: [[i64; 3]; 3] = {rs_mat(w3)};
const WHITEN_3X3_ALIGN: [[i64; 3]; 3] = {rs_mat(al3)};
const WHITEN_NONPD_R: [[i64; 2]; 2] = {rs_mat(WHITEN_NONPD)};
"""

    out = OrderedDict()
    out[f"vectors/pipeline-vectors-v{VECTOR_VERSION}.json"] = (
        json.dumps(vectors, indent=2, ensure_ascii=False) + "\n"
    ).encode()
    out["fixtures/synthetic/frame-0001.json"] = (
        json.dumps(fixture, indent=2, ensure_ascii=False) + "\n"
    ).encode()
    out["crates/axonos-pipeline-core/tests/data/vectors.rs"] = data_rs.encode()

    sums = ""
    for rel in [
        f"vectors/pipeline-vectors-v{VECTOR_VERSION}.json",
        "fixtures/synthetic/frame-0001.json",
    ]:
        sums += hashlib.sha256(out[rel]).hexdigest() + "  " + rel + "\n"
    out["vectors/SHA256SUMS"] = sums.encode()
    return out


def main() -> None:
    for rel, data in build().items():
        path = ROOT / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        print(f"wrote {rel} ({len(data)} bytes)")


if __name__ == "__main__":
    main()
