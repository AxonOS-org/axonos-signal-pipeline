#!/usr/bin/env python3
"""Single source of truth for v0.1.0 conformance vectors.

Generates, deterministically and from first principles:
  vectors/pipeline-vectors-v0.1.0.json
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

VECTOR_VERSION = "0.1.0"
DATE = "2026-06-10"

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
"""

    out = OrderedDict()
    out["vectors/pipeline-vectors-v0.1.0.json"] = (
        json.dumps(vectors, indent=2, ensure_ascii=False) + "\n"
    ).encode()
    out["fixtures/synthetic/frame-0001.json"] = (
        json.dumps(fixture, indent=2, ensure_ascii=False) + "\n"
    ).encode()
    out["crates/axonos-pipeline-core/tests/data/vectors.rs"] = data_rs.encode()

    sums = ""
    for rel in ["vectors/pipeline-vectors-v0.1.0.json", "fixtures/synthetic/frame-0001.json"]:
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
