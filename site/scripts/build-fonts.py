#!/usr/bin/env python3
"""Build the self-hosted latin WOFF2 subsets for the site.

Sources
-------
Instrument Serif regular and italic are tracked in ``assets/fonts``.
Geist and Geist Mono are SIL OFL 1.1 and are taken from the upstream
``vercel/geist-font`` release ``v1.8.2`` (published 2026-10-21):

    https://github.com/vercel/geist-font/releases/download/v1.8.2/geist-font-v1.8.2.zip

Expected files inside the extracted release:

    geist-font/Geist/variable/Geist[wght].ttf
    geist-font/GeistMono/variable/GeistMono[wght].ttf

The release is not committed; point ``--geist-dir`` at the extracted
``geist-font`` directory. The script verifies the two file digests before it
writes anything, so a different release cannot silently change the shipped
fonts.

Only the regular weight is used, so each variable source is instanced at
``wght=400`` before subsetting. The subset keeps basic latin, latin-1 and
general punctuation, which is the character set the copy deck uses.

Requires ``fonttools`` 4.68.0 and ``brotli`` 1.2.1 (both MIT), installed in a
throw-away environment:

    python3 -m venv /tmp/climb-fonts
    /tmp/climb-fonts/bin/pip install fonttools==4.68.0 brotli==1.2.1
    /tmp/climb-fonts/bin/python site/scripts/build-fonts.py \
        --geist-dir /tmp/geist-font/geist-font
"""

from __future__ import annotations

import argparse
import hashlib
import sys
import tempfile
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

ROOT = Path(__file__).resolve().parents[2]
ASSET_DIR = ROOT / "assets" / "fonts"
OUT_DIR = ROOT / "site" / "public" / "fonts"

# Basic latin, latin-1, latin extended punctuation, currency and a few marks.
UNICODES = (
    "U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,"
    "U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,"
    "U+2212,U+2215,U+FEFF,U+FFFD"
)

GEIST_FILES = {
    "geist-regular-latin.woff2": (
        "Geist[wght].ttf",
        "98c7f53f48cb74b8b89e012e3902f0c11d12ae361c67db8da35d5969cfaa610d",
    ),
    "geist-mono-regular-latin.woff2": (
        "GeistMono[wght].ttf",
        "4d2bad39b04d4e994500de6d2a70d2a74beb61de24cd127394294258f55e8c7f",
    ),
}

BUDGET = 120 * 1024


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def instance_at_regular(source: Path, target: Path) -> None:
    font = TTFont(source)
    instantiateVariableFont(font, {"wght": 400}, inplace=True)
    font.save(target)


def subset_font(source: Path, target: Path) -> None:
    options = subset.Options()
    options.flavor = "woff2"
    options.layout_features = ["kern", "liga", "calt", "tnum", "lnum", "onum", "pnum"]
    options.name_IDs = ["*"]
    options.name_legacy = True
    options.name_languages = ["*"]
    options.notdef_outline = True
    options.recommended_glyphs = True
    font = subset.load_font(str(source), options)
    subsetter = subset.Subsetter(options=options)
    subsetter.populate(unicodes=subset.parse_unicodes(UNICODES))
    subsetter.subset(font)
    subset.save_font(font, str(target), options)


def build_instrument(name: str, source: Path) -> Path:
    target = OUT_DIR / name
    subset_font(source, target)
    return target


def build_geist(name: str, source: Path) -> Path:
    target = OUT_DIR / name
    with tempfile.TemporaryDirectory(prefix="climb-font-") as tmp:
        static = Path(tmp) / "regular.ttf"
        instance_at_regular(source, static)
        subset_font(static, target)
    return target


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--geist-dir",
        type=Path,
        required=True,
        help="the extracted geist-font release directory (contains Geist/ and GeistMono/)",
    )
    args = parser.parse_args()

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    outputs: list[Path] = []

    outputs.append(
        build_instrument(
            "instrument-serif-regular-latin.woff2",
            ASSET_DIR / "InstrumentSerif-Regular.ttf",
        )
    )
    outputs.append(
        build_instrument(
            "instrument-serif-italic-latin.woff2",
            ASSET_DIR / "InstrumentSerif-Italic.ttf",
        )
    )

    for name, (filename, expected) in GEIST_FILES.items():
        source = args.geist_dir / (
            "GeistMono" if filename.startswith("GeistMono") else "Geist"
        ) / "variable" / filename
        if not source.is_file():
            print(f"missing source: {source}", file=sys.stderr)
            return 1
        actual = digest(source)
        if actual != expected:
            print(
                f"unexpected digest for {source.name}:\n  expected {expected}\n  found    {actual}",
                file=sys.stderr,
            )
            return 1
        outputs.append(build_geist(name, source))

    print(f"{'file':44} {'bytes':>8}")
    total = 0
    for path in outputs:
        size = path.stat().st_size
        total += size
        print(f"{path.name:44} {size:8d}")
    print(f"{'total':44} {total:8d}")
    print(f"budget: {BUDGET} bytes, headroom: {BUDGET - total} bytes")
    return 0 if total <= BUDGET else 1


if __name__ == "__main__":
    sys.exit(main())
