#!/usr/bin/env python3
"""Verify the self-hosted latin WOFF2 faces and print the budget table.

Provenance
----------
Both files are the latin subsets that Google Fonts serves for the prototype's
own stylesheet request:

    curl -A 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36
             (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36' \
      'https://fonts.googleapis.com/css2?family=Bricolage+Grotesque:opsz,wght@12..96,600;12..96,700&family=Geist:wght@400;500;600&display=swap'

The browser agent matters: an unknown agent is served truetype, four times the
bytes. Bricolage Grotesque is one variable file covering wght 200-800 and
opsz 12-96; Geist is one variable file covering wght 100-900. Latin only, as
in the prototype.

Licences are SIL Open Font License 1.1, in ``public/fonts/licences/``:

    Bricolage Grotesque: Copyright 2022 The Bricolage Grotesque Project Authors
    Geist: Copyright (c) 2023 Vercel, in collaboration with basement.studio

Fallback metrics are measured from the two files with fontTools (hhea and
OS/2 tables): Bricolage x-height 528/1000, hhea ascent/descent 930/-270;
Geist x-height 530/1000, hhea ascent/descent 1005/-295. Against Arial's
519/1000 x-height and Arimo's 528.32/1000 that gives size-adjust 101.8 % and
100.3 %, and the overrides in ``src/styles/climb.css`` are the hhea values.

Run ``python3 site/scripts/build-fonts.py``; it writes nothing.
"""

from __future__ import annotations

import hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FONT_DIR = ROOT / "site" / "public" / "fonts"

BUDGET = 120 * 1024

# sha256 of the exact bytes served for the latin block on 2026-11-18.
FILES = {
    "bricolage-grotesque-latin.woff2": (
        "a79fdb52d4a5c76552452f69202add96e287401fff03d3e8c0e38b4dcb5a99cd",
        "Bricolage Grotesque",
        "wght 200-800, opsz 12-96",
    ),
    "geist-latin.woff2": (
        "19f9c92546aa300c312235e3125af1b81394d8db9a4bc4a425cd5b641d2d54e1",
        "Geist",
        "wght 100-900",
    ),
}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    failures = 0
    total = 0
    print(f"{'file':36} {'family':20} {'axes':22} {'bytes':>8}")
    for name, (expected, family, axes) in FILES.items():
        path = FONT_DIR / name
        if not path.is_file():
            print(f"{name:36} missing")
            failures += 1
            continue
        size = path.stat().st_size
        total += size
        actual = digest(path)
        mark = "ok" if actual == expected else "DIGEST MISMATCH"
        print(f"{name:36} {family:20} {axes:22} {size:8}")
        if actual != expected:
            print(f"  expected {expected}\n  actual   {actual}")
            failures += 1
        if mark != "ok":
            failures += 1
    print(f"{'total':36} {'':20} {'':22} {total:8}  budget {BUDGET}  "
          f"{'pass' if total <= BUDGET else 'FAIL'}")
    if total > BUDGET:
        failures += 1
    for licence in sorted((FONT_DIR / "licences").glob("*.txt")):
        print(f"licence {licence.name:28} {licence.stat().st_size:8} bytes")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
