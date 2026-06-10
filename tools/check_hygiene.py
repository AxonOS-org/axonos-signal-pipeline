#!/usr/bin/env python3
"""CI gate: no private/prototype contact metadata anywhere in the tree.

Patterns are assembled from fragments so this file never matches itself.
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PATTERNS = ["gma" + "il.com", "deniss" + "martrich", "axonosorg" + "@"]
SKIP_DIRS = {".git", "target", "__pycache__"}
TEXT_EXT = {
    ".rs", ".toml", ".md", ".json", ".yml", ".yaml", ".cff", ".py", ".txt", ""
}


def main() -> int:
    bad = 0
    for path in ROOT.rglob("*"):
        if not path.is_file() or set(path.parts) & SKIP_DIRS:
            continue
        if path.suffix.lower() not in TEXT_EXT:
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        for i, line in enumerate(text.splitlines(), 1):
            low = line.lower()
            for pat in PATTERNS:
                if pat in low:
                    print(f"FAIL {path.relative_to(ROOT)}:{i}: matches private-contact pattern")
                    bad = 1
    print("OK" if bad == 0 else "FAILED")
    return bad


if __name__ == "__main__":
    raise SystemExit(main())
