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

VECTOR_VERSION = "0.2.4"
DATE = "2026-06-18"

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


def build() -> "OrderedDict[str, bytes]":
    samples = lcg_samples(SEED, N_CHANNELS * SAMPLES_PER_CHANNEL)
    checksum = frame_checksum(
        SEQ, TIMESTAMP_US, RATE_HZ, CHANNEL_MASK, SAMPLES_PER_CHANNEL, samples
    )

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
