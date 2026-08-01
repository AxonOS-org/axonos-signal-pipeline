#!/usr/bin/env python3
"""CI gate: conformance artifacts must be exactly reproducible.

Re-runs the generator in memory and byte-compares every output against the
committed files; independently re-checks SHA256SUMS and one external FNV
anchor; verifies project-grade metadata. Exit code 0 = green.
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))

import gen_test_vectors as gen  # noqa: E402

FAIL = 0


def check(ok: bool, msg: str) -> None:
    global FAIL
    print(("  PASS  " if ok else "  FAIL  ") + msg)
    if not ok:
        FAIL = 1


def main() -> int:
    # Independent anchor for the generator's own FNV implementation.
    check(gen.fnv1a64(b"") == 0xCBF29CE484222325, "FNV-1a 64 offset-basis anchor")

    expected = gen.build()
    for rel, want in expected.items():
        path = ROOT / rel
        if not path.exists():
            check(False, f"{rel}: missing")
            continue
        got = path.read_bytes()
        check(got == want, f"{rel}: byte-identical to generator output")

    sums_path = ROOT / "vectors" / "SHA256SUMS"
    if sums_path.exists():
        for line in sums_path.read_text().splitlines():
            digest, rel = line.split(maxsplit=1)
            rel = rel.strip()
            actual = hashlib.sha256((ROOT / rel).read_bytes()).hexdigest()
            check(actual == digest, f"SHA256SUMS entry: {rel}")

    vec = json.loads((ROOT / "vectors" / "pipeline-vectors-v0.9.3.json").read_text())
    meta = vec.get("_meta", {})
    check(
        meta.get("maintainer") == "The AxonOS Project <connect@axonos.org>",
        "_meta.maintainer is project-grade",
    )
    check(meta.get("security_contact") == "security@axonos.org", "_meta.security_contact")
    check(meta.get("license") == "CC0-1.0", "_meta.license is CC0-1.0")
    # Derived, never a literal. A hardcoded version here was already stale by
    # one release before anyone noticed, because a validator that repeats a
    # number is one more place for it to drift — and the thing it validates is
    # precisely that numbers do not drift.
    crate_ver = re.search(r'^version = "(.+)"', (ROOT / "Cargo.toml").read_text(), re.M)
    gen_ver = re.search(
        r'^VECTOR_VERSION = "(.+)"', (ROOT / "tools" / "gen_test_vectors.py").read_text(), re.M
    )
    check(crate_ver is not None, "Cargo.toml declares a version")
    check(gen_ver is not None, "the generator declares VECTOR_VERSION")
    if crate_ver and gen_ver:
        check(gen_ver.group(1) == crate_ver.group(1), "generator version tracks the crate")
        check(
            meta.get("vector_version") == crate_ver.group(1),
            "_meta.vector_version tracks the crate",
        )

    print("OK" if FAIL == 0 else "FAILED")
    return FAIL


if __name__ == "__main__":
    raise SystemExit(main())
