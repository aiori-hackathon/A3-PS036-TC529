#!/usr/bin/env python3
"""Build the CLIMB logo asset set from the clean vector sources.

The source of truth is the pair of vector files in docs/media/logo:
climb-logo-light.svg and climb-logo-dark.svg. Their geometry is the team's
design; the files carry no filters, no blend modes and no external
references, so they render identically in every conforming renderer. This
script renders the raster derivatives, composites the social cards, checks
the vector files against the publication rules, prints the WCAG contrast
table of every letterform against the dark backgrounds, and refreshes the
logo file table in docs/media/README.md.

Rasteriser: resvg (SVG to PNG). Compositor: cairo (PNG onto a card). No
other image library is needed.

Usage:
    python3 scripts/logo/build_logo_assets.py           # rebuild every asset
    python3 scripts/logo/build_logo_assets.py --check   # verify the committed set
    python3 scripts/logo/build_logo_assets.py --contrast # print the contrast table
"""

from __future__ import annotations

import argparse
import math
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

import cairo

REPO_ROOT = Path(__file__).resolve().parents[2]
LOGO_DIR = REPO_ROOT / "docs" / "media" / "logo"
MEDIA_README = REPO_ROOT / "docs" / "media" / "README.md"

#: Markers around the logo file table in docs/media/README.md, refreshed by
#: this script so the documented sizes cannot drift from the files on disk.
LOGO_TABLE_BEGIN = "<!-- logo-media:begin -->"
LOGO_TABLE_END = "<!-- logo-media:end -->"

#: The two dark surfaces the dark variant is published against: the brand
#: ink and the canvas a dark-themed code host paints around it.
BRAND_INK = "#07080A"
HOST_DARK = "#0D1117"

#: The letterform inks of the dark variant, and the contrast floor every
#: letter body must clear on both dark surfaces. Fades layered on top of
#: another letter are listed for the record; they are not standalone marks.
LETTER_INKS = [
    ("C, I, B bodies; L rungs; tree ink strokes; root dot", "#F2EDE4", 4.5),
    ("L top rung", "#FF5400", 4.5),
    ("M peak (strongest stop of the body gradient)", "#FD5500", 4.5),
    ("tree red strokes", "#FF0000", 4.5),
]
RECORDED_FADES = [
    ("C edge stroke, rose (peak)", "#C96262"),
    ("L foot rose (peak)", "#E59B9B"),
    ("L foot orange (peak)", "#FE7F00"),
    ("M rose wash (peak)", "#D9D9D9"),
    ("M base red-brown (peak; layered on the orange M)", "#4D1111"),
    ("L foot navy (peak; decorative fade)", "#06234B"),
]

#: What each generated file is for, in the order the table lists them.
#: Dimensions are the raster size; the vectors carry the intrinsic 1504x912
#: canvas. The width of the rendered logo on the social cards is chosen so
#: the lockup clears a wide margin on both card sizes.
ASSET_ROLES = [
    ("climb-logo-light.svg",
     "The wordmark for light surfaces; geometry and inks of the team design, "
     "cleaned of filters, blend modes and external references."),
    ("climb-logo-dark.svg",
     "The wordmark for dark surfaces: the black letterforms become the "
     "off-white ink; the orange, the red tree strokes and the L foot stay."),
    ("climb-logo-light@1x.png", "The light wordmark at native resolution."),  # path-hygiene:allow
    ("climb-logo-light@2x.png", "The light wordmark at twice native resolution."),  # path-hygiene:allow
    ("climb-logo-light@3x.png", "The light wordmark at thrice native resolution."),  # path-hygiene:allow
    ("climb-logo-dark@1x.png", "The dark wordmark at native resolution."),  # path-hygiene:allow
    ("climb-logo-dark@2x.png", "The dark wordmark at twice native resolution."),  # path-hygiene:allow
    ("climb-logo-dark@3x.png", "The dark wordmark at thrice native resolution."),  # path-hygiene:allow
    ("social-preview.png",
     "The social preview card: the dark wordmark centred on the brand ink."),
    ("og-image.png",
     "The Open Graph card: the dark wordmark centred on the brand ink."),
    ("favicon.svg",
     "The favicon: the M with the tree on a square canvas; the ink strokes "
     "and the root dot follow the viewer's colour scheme."),
    ("favicon.ico",
     "The favicon for browsers without SVG support: the 16, 32 and 48 px "
     "renditions packed in one ICO."),
    ("favicon-192.png", "The favicon at web-app icon size, transparent."),
    ("favicon-512.png", "The favicon at large web-app icon size, transparent."),
    ("apple-touch-icon.png",
     "The Apple touch icon: the dark-ink mark on the brand ink, opaque."),
]

#: Elements a published logo vector must not carry, with the reason each is
#: banned: filters are dropped or mis-rendered by whole renderers (librsvg
#: aborts on this artwork's feImage), blend modes resolve differently across
#: renderers, and external references turn a vector into a network fetch.
BANNED_ELEMENTS = {
    "filter", "feImage", "script", "foreignObject", "image", "use",
}
BANNED_TEXT = ("mix-blend-mode",)
SVG_NS = "{http://www.w3.org/2000/svg}"


def fail(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    sys.exit(1)


# --------------------------------------------------------------------------
# Vector checks
# --------------------------------------------------------------------------

def check_vector(path: Path) -> list[str]:
    """Return the publication-rule violations of one logo vector.

    The rules: strict XML (no recovery parsing), at most 12,288 bytes, no
    banned element, no blend mode, and no reference that leaves the file.
    """
    problems: list[str] = []
    raw = path.read_bytes()
    if len(raw) > 12_288:
        problems.append(f"{len(raw)} bytes exceeds the 12,288-byte budget")
    try:
        root = ET.fromstring(raw)
    except ET.ParseError as error:
        return [f"not strict XML: {error}"]
    for element in root.iter():
        tag = element.tag.replace(SVG_NS, "")
        if tag in BANNED_ELEMENTS:
            problems.append(f"carries a <{tag}> element")
        for key, value in element.attrib.items():
            if any(banned in value for banned in BANNED_TEXT):
                problems.append(f"{key}={value!r} carries a blend mode")
            if key.endswith("href") and not value.startswith("#"):
                problems.append(f"{key}={value!r} leaves the file")
    return problems


# --------------------------------------------------------------------------
# Rasterisation and compositing
# --------------------------------------------------------------------------

def resvg(svg: Path, out: Path, width: int | None = None,
          height: int | None = None) -> None:
    command = ["resvg"]
    if width is not None:
        command += ["-w", str(width)]
    if height is not None:
        command += ["-h", str(height)]
    command += [str(svg), str(out)]
    try:
        subprocess.run(command, check=True, capture_output=True, text=True)
    except FileNotFoundError:
        fail("resvg is not installed; it is the rasteriser of this script")
    except subprocess.CalledProcessError as error:
        fail(f"resvg failed on {svg}: {error.stderr.strip()}")


def composite_card(logo: Path, out: Path, width: int, height: int,
                  logo_width: int) -> None:
    """Centre one rasterised logo on a card of the brand ink."""
    surface = cairo.ImageSurface(cairo.FORMAT_ARGB32, width, height)
    context = cairo.Context(surface)
    context.set_source_rgb(7 / 255, 8 / 255, 10 / 255)
    context.paint()
    logo_surface = cairo.ImageSurface.create_from_png(str(logo))
    logo_height = round(logo_height_of(logo_surface, logo_width))
    context.set_source_surface(
        logo_surface,
        (width - logo_width) / 2,
        (height - logo_height) / 2,
    )
    context.paint()
    surface.write_to_png(str(out))


def logo_height_of(surface: cairo.ImageSurface, width: int) -> float:
    return surface.get_height() * width / surface.get_width()


def render_favicon_set(scratch: Path, dest: Path) -> None:
    """Render the favicon derivatives of the chosen mark.

    The ICO and the transparent PNGs carry the light ink (the vector's
    presentation attributes); renderers without colour-scheme support draw
    exactly these. The Apple touch icon is opaque by platform requirement,
    so it composites the dark-ink rendition of the same vector on the brand
    ink.
    """
    favicon = LOGO_DIR / "favicon.svg"
    for size in (16, 32, 48):
        resvg(favicon, scratch / f"favicon-{size}.png", width=size, height=size)
    write_ico({size: scratch / f"favicon-{size}.png" for size in (16, 32, 48)},
              dest / "favicon.ico")
    for size in (192, 512):
        resvg(favicon, dest / f"favicon-{size}.png",
              width=size, height=size)
    # the dark-ink rendition: the same vector with the scheme-dependent ink
    # baked in, rendered once onto the opaque brand-ink card
    text = favicon.read_text()
    for ink in ('stroke="#000000"', 'stroke="#050505"', 'fill="#000000"'):
        if ink not in text:
            fail(f"favicon.svg no longer carries the ink attribute {ink!r}")
        text = text.replace(ink, ink.replace('#000000', '#F2EDE4')
                            .replace('#050505', '#F2EDE4'))
    dark_vector = scratch / "favicon-dark-ink.svg"
    dark_vector.write_text(text)
    resvg(dark_vector, scratch / "favicon-dark-ink.png", width=180, height=180)
    composite_card(scratch / "favicon-dark-ink.png",
                   dest / "apple-touch-icon.png", 180, 180, logo_width=180)


def write_ico(images: dict[int, Path], out: Path) -> None:
    """Pack PNG renditions into one ICO file.

    Every entry is a PNG blob, which every browser released after 2008
    reads; the classic BMP encoding is not needed.
    """
    import struct

    if not images:
        fail("an ICO needs at least one image")
    entries = b""
    blobs = b""
    offset = 6 + 16 * len(images)
    for size, path in sorted(images.items()):
        data = path.read_bytes()
        if size < 1 or size > 256:
            fail(f"an ICO entry must be 1..256 px, not {size}")
        dimension = 0 if size == 256 else size
        entries += struct.pack("<BBBBHHII", dimension, dimension, 0, 0,
                               1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    out.write_bytes(struct.pack("<HHH", 0, 1, len(images)) + entries + blobs)


# --------------------------------------------------------------------------
# Contrast
# --------------------------------------------------------------------------

def channels(hexcolor: str) -> tuple[int, int, int]:
    digits = hexcolor.lstrip("#")
    if len(digits) != 6:
        fail(f"{hexcolor!r} is not a six-digit hex colour")
    return tuple(int(digits[i:i + 2], 16) for i in (0, 2, 4))  # type: ignore[return-value]


def luminance(hexcolor: str) -> float:
    linear = []
    for value in channels(hexcolor):
        value /= 255
        linear.append(value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4)
    red, green, blue = linear
    return 0.2126 * red + 0.7152 * green + 0.0722 * blue


def contrast(foreground: str, background: str) -> float:
    first, second = luminance(foreground), luminance(background)
    lighter, darker = max(first, second), min(first, second)
    return (lighter + 0.05) / (darker + 0.05)


def contrast_table() -> tuple[list[tuple[str, str, float, float]], list[str]]:
    """The WCAG contrast ratios of every letter ink on both dark surfaces."""
    rows = []
    failures = []
    for label, colour, floor in LETTER_INKS:
        row = (label, colour,
               contrast(colour, BRAND_INK), contrast(colour, HOST_DARK))
        rows.append(row)
        for surface, ratio in ((BRAND_INK, row[2]), (HOST_DARK, row[3])):
            if ratio + 1e-9 < floor:
                failures.append(
                    f"{label} {colour} is {ratio:.2f}:1 on {surface}, "
                    f"floor {floor}:1")
    recorded = []
    for label, colour in RECORDED_FADES:
        recorded.append(
            f"{label:<52} {colour}  "
            f"{contrast(colour, BRAND_INK):6.2f}:1  "
            f"{contrast(colour, HOST_DARK):6.2f}:1")
    return rows, failures


def print_contrast_table() -> list[str]:
    rows, failures = contrast_table()
    print("WCAG contrast of the dark-variant letterforms")
    print(f"{'letterform':<52} {'colour'}   vs {BRAND_INK}  vs {HOST_DARK}")
    for label, colour, brand, host in rows:
        print(f"{label:<52} {colour}   {brand:6.2f}:1  {host:6.2f}:1")
    print("\nLayered fades, recorded rather than floored (they sit on another "
          "letter or trail from one):")
    for line in recorded_lines():
        print(f"  {line}")
    return failures


def recorded_lines() -> list[str]:
    lines = []
    for label, colour in RECORDED_FADES:
        lines.append(
            f"{label:<52} {colour}  "
            f"{contrast(colour, BRAND_INK):6.2f}:1  "
            f"{contrast(colour, HOST_DARK):6.2f}:1")
    return lines


# --------------------------------------------------------------------------
# The media table
# --------------------------------------------------------------------------

def dimensions_of(path: Path) -> str:
    if path.suffix == ".svg":
        root = ET.fromstring(path.read_bytes())
        return f"{root.get('width')} × {root.get('height')}"
    if path.suffix == ".ico":
        return "16 × 16, 32 × 32, 48 × 48"
    surface = cairo.ImageSurface.create_from_png(str(path))
    return f"{surface.get_width()} × {surface.get_height()}"


def size_of(path: Path) -> str:
    return f"{path.stat().st_size:,} B"


def render_media_table() -> str:
    lines = [
        "| File | What it is | Size | Dimensions |",
        "|---|---|---|---|",
    ]
    for name, role in ASSET_ROLES:
        path = LOGO_DIR / name
        lines.append(f"| `{name}` | {role} | {size_of(path)} | "
                     f"{dimensions_of(path)} |")
    return "\n".join(lines)


def refresh_media_table() -> None:
    text = MEDIA_README.read_text()
    if LOGO_TABLE_BEGIN not in text or LOGO_TABLE_END not in text:
        fail(f"{MEDIA_README} is missing the `{LOGO_TABLE_BEGIN}` / "
              f"`{LOGO_TABLE_END}` table markers")
    before, rest = text.split(LOGO_TABLE_BEGIN, 1)
    _, after = rest.split(LOGO_TABLE_END, 1)
    MEDIA_README.write_text(
        before + LOGO_TABLE_BEGIN + "\n" + render_media_table() + "\n"
        + LOGO_TABLE_END + after)


# --------------------------------------------------------------------------
# Build and check
# --------------------------------------------------------------------------

def build(logo_light: Path, logo_dark: Path) -> None:
    for name in ("climb-logo-light.svg", "climb-logo-dark.svg", "favicon.svg"):
        problems = check_vector(LOGO_DIR / name)
        if problems:
            fail(f"{name}: " + "; ".join(problems))
    resolutions = (1, 2, 3)
    for variant, source in (("light", logo_light), ("dark", logo_dark)):
        for scale in resolutions:
            out = LOGO_DIR / f"climb-logo-{variant}@{scale}x.png"
            resvg(source, out, width=1504 * scale, height=912 * scale)
            print(f"rendered {out.relative_to(REPO_ROOT)}")
    dark_native = LOGO_DIR / "climb-logo-dark@1x.png"  # path-hygiene:allow
    composite_card(dark_native, LOGO_DIR / "social-preview.png",
                   1280, 640, logo_width=800)
    composite_card(dark_native, LOGO_DIR / "og-image.png",
                   1200, 630, logo_width=720)
    print("composited social-preview.png and og-image.png")
    with tempfile.TemporaryDirectory() as temporary:
        render_favicon_set(Path(temporary), LOGO_DIR)
    print("rendered the favicon set")
    refresh_media_table()
    print(f"refreshed the logo table in {MEDIA_README.relative_to(REPO_ROOT)}")


def check(logo_light: Path, logo_dark: Path) -> None:
    failures = []
    for name in ("climb-logo-light.svg", "climb-logo-dark.svg", "favicon.svg"):
        problems = check_vector(LOGO_DIR / name)
        if problems:
            failures.append(f"{name}: " + "; ".join(problems))
    with tempfile.TemporaryDirectory() as temporary:
        scratch = Path(temporary)
        rendered = {}
        for variant, source in (("light", logo_light), ("dark", logo_dark)):
            for scale in (1, 2, 3):
                out = scratch / f"climb-logo-{variant}@{scale}x.png"
                resvg(source, out, width=1504 * scale, height=912 * scale)
                rendered[f"climb-logo-{variant}@{scale}x.png"] = out
        dark_native = scratch / "climb-logo-dark@1x.png"  # path-hygiene:allow
        composite_card(dark_native, scratch / "social-preview.png",
                       1280, 640, logo_width=800)
        composite_card(dark_native, scratch / "og-image.png",
                       1200, 630, logo_width=720)
        for name in ("social-preview.png", "og-image.png"):
            rendered[name] = scratch / name
        render_favicon_set(scratch, scratch)
        for name in ("favicon.ico", "favicon-192.png", "favicon-512.png",
                     "apple-touch-icon.png"):
            rendered[name] = scratch / name
        for name, path in rendered.items():
            committed = LOGO_DIR / name
            if not committed.exists():
                failures.append(f"{committed.relative_to(REPO_ROOT)} is missing")
            elif committed.read_bytes() != path.read_bytes():
                failures.append(
                    f"{committed.relative_to(REPO_ROOT)} differs from a fresh "
                    f"render of the committed vectors")
    failures.extend(print_contrast_table())
    expected_table = render_media_table()
    if f"{LOGO_TABLE_BEGIN}\n{expected_table}\n{LOGO_TABLE_END}" \
            not in MEDIA_README.read_text():
        failures.append(
            f"the logo table in {MEDIA_README.relative_to(REPO_ROOT)} has "
            f"drifted from the files on disk")
    if failures:
        for failure in failures:
            print(f"error: {failure}", file=sys.stderr)
        sys.exit(1)
    print("the committed logo set matches a fresh render of the vectors")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true",
                        help="verify the committed assets instead of writing")
    parser.add_argument("--contrast", action="store_true",
                        help="print the WCAG contrast table only")
    arguments = parser.parse_args()

    logo_light = LOGO_DIR / "climb-logo-light.svg"
    logo_dark = LOGO_DIR / "climb-logo-dark.svg"
    for path in (logo_light, logo_dark):
        if not path.exists():
            fail(f"{path.relative_to(REPO_ROOT)} is missing")

    if arguments.contrast:
        failures = print_contrast_table()
        if failures:
            for failure in failures:
                print(f"error: {failure}", file=sys.stderr)
            return 1
        return 0
    if arguments.check:
        check(logo_light, logo_dark)
    else:
        build(logo_light, logo_dark)
    return 0


if __name__ == "__main__":
    sys.exit(main())
