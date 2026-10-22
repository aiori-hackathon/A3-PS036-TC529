#!/usr/bin/env python3
"""Build the CLIMB logo asset set from the original artwork.

The input is a single 1408x768 PNG: a white panel with light-blue side
margins, the CLIMB crest, the wordmark, a two-line tagline, and a faint
generator sparkle in the bottom-right corner that must not survive into any
published asset.

Pipeline
--------
1. Analyse the source: size, background colours, content bounding box and the
   sampled brand colours.
2. Build a transparent master: crop to the content box plus 4% padding (which
   also drops the corner sparkle) and convert the white panel to transparency
   with a colour-to-alpha algorithm that recovers the ink colour instead of
   thresholding, so anti-aliased edges keep no white fringe.
3. Derive the published variants: a light and a dark full lockup, a light and
   a dark mark (crest plus wordmark, no tagline), square icon renditions and a
   social-preview card.

Every published asset is written as an sRGB PNG with an alpha channel; the
dark variants are the master with the ink colours remapped, so the two themes
share one geometry.

Usage:
    python3 scripts/logo/make_assets.py            # rebuild every asset
    python3 scripts/logo/make_assets.py --analyse  # print the analysis only
"""

from __future__ import annotations

import argparse
import math
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image

# --------------------------------------------------------------------------
# Paths
# --------------------------------------------------------------------------

REPO_ROOT = Path(__file__).resolve().parents[2]
MEDIA_DIR = REPO_ROOT / "docs" / "media"
SOURCE_PNG = MEDIA_DIR / "source" / "climb-logo-original.png"
BRAND_TOML = MEDIA_DIR / "brand.toml"
MEDIA_README = MEDIA_DIR / "README.md"

#: Markers around the file table in `docs/media/README.md`, refreshed by the
#: script so the documented sizes cannot drift away from the files on disk.
MEDIA_TABLE_BEGIN = "<!-- media:begin -->"
MEDIA_TABLE_END = "<!-- media:end -->"

#: What each generated file is for, in the order the table lists them.
ASSET_ROLES = [
    (
        "climb-logo-light.png",
        "Full lockup on a light background: crest, wordmark and tagline.",
    ),
    (
        "climb-logo-dark.png",
        "Full lockup on a dark background; the slate ink is lightened.",
    ),
    (
        "climb-mark-light.png",
        "Crest and wordmark without the tagline, for a light background.",
    ),
    (
        "climb-mark-dark.png",
        "Crest and wordmark without the tagline, for a dark background.",
    ),
    ("climb-icon.png", "The crest alone, on a square transparent canvas."),
    ("climb-icon-32.png", "The crest alone at favicon size."),
    ("climb-icon-180.png", "The crest alone at Apple touch-icon size."),
    ("climb-icon-192.png", "The crest alone at web-app icon size."),
    ("climb-icon.svg", "The crest alone, traced to vector."),
    (
        "social-preview.png",
        "The social preview card: the dark mark on a near-black field.",
    ),
    ("brand.toml", "The sampled palette and the dark-background colour decisions."),
]

# --------------------------------------------------------------------------
# Constants
# --------------------------------------------------------------------------

#: Padding added around the content bounding box, as a fraction of each side.
PAD_FRACTION = 0.04

#: A pixel whose brightest channel is at least this far below white is
#: treated as panel background and made fully transparent. The panel is white
#: with a couple of units of noise, so the tolerance has to absorb that
#: without swallowing the faintest real anti-aliasing.
BACKGROUND_TOLERANCE = 6.0

#: Coverage below this fraction is dropped outright. The artwork's own noise
#: leaves isolated specks of a fraction of a percent of coverage in the paper;
#: they are invisible, but they would otherwise be mistaken for artwork when
#: the crest is cut out of the master.
ALPHA_FLOOR = 0.02

#: Coverage a pixel must reach before it counts as artwork when the crest and
#: the wordmark are told apart.
INK_THRESHOLD = 32

#: The height in pixels a band of lettering must reach to count as a display
#: line. The wordmark's capitals are far taller than the tagline's; the
#: tagline is the run of short bands that follows the last band this tall.
TEXT_LINE_HEIGHT = 40

#: The dark theme's page background, used for the contrast arithmetic and for
#: the social-preview card.
DARK_BACKGROUND = (0x0D, 0x11, 0x17)

#: The social-preview card background.
PREVIEW_BACKGROUND = (0x0B, 0x0D, 0x10)

#: Minimum contrast ratio (WCAG 2.1) a brand colour must reach against the
#: dark background before it is left untouched in the dark variants.
MIN_DARK_CONTRAST = 3.0

#: The slate colour replaced by a light one in the dark variants.
DARK_SLATE = (0xE6, 0xED, 0xF3)


def hex_of(rgb) -> str:
    """Format an RGB triple as an uppercase hex string."""
    return "#{:02X}{:02X}{:02X}".format(*(int(round(c)) for c in rgb))


# --------------------------------------------------------------------------
# Colour helpers
# --------------------------------------------------------------------------


def relative_luminance(rgb) -> float:
    """WCAG 2.1 relative luminance of an sRGB colour."""
    channels = []
    for raw in rgb:
        c = raw / 255.0
        channels.append(c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4)
    r, g, b = channels
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast_ratio(fg, bg) -> float:
    """WCAG 2.1 contrast ratio between two sRGB colours."""
    a = relative_luminance(fg)
    b = relative_luminance(bg)
    lighter, darker = max(a, b), min(a, b)
    return (lighter + 0.05) / (darker + 0.05)


# --------------------------------------------------------------------------
# Step 1 — analysis
# --------------------------------------------------------------------------


def content_bounding_box(rgb: np.ndarray, threshold: int = 232):
    """Bounding box of the artwork inside the white panel.

    The panel margins carry a few units of colour noise, so the box is taken
    from pixels that are clearly darker than the paper rather than from every
    non-white pixel.
    """
    ink = rgb.min(axis=2) < threshold
    ys, xs = np.nonzero(ink)
    return int(xs.min()), int(ys.min()), int(xs.max()), int(ys.max())


def sample_brand_colours(rgb: np.ndarray):
    """Sample the ink colours of the artwork.

    Each family is defined by a generous chromaticity window; the reported
    colour is the most common value inside the dense core of that family, so
    anti-aliased edge pixels cannot drag the sample towards the paper.
    """
    r = rgb[..., 0].astype(np.float64)
    g = rgb[..., 1].astype(np.float64)
    b = rgb[..., 2].astype(np.float64)
    mx = rgb.max(axis=2).astype(np.float64)
    mn = rgb.min(axis=2).astype(np.float64)
    sat = mx - mn

    windows = {
        "slate": (mx < 150) & (sat <= 45) & (b >= r),
        "orange": (r > 150) & ((r - b) > 70) & (g < 140) & (g >= 60),
        "teal": ((g - r) > 25) & ((b - r) > 12) & (mx < 210),
    }

    sampled = {}
    for name, window in windows.items():
        px = rgb[window]
        if len(px) == 0:
            continue
        median = np.median(px, axis=0)
        # The core of the family: pixels within 16 units of the median. The
        # most common colour there is the flat fill the generator used.
        core = px[np.all(np.abs(px - median) <= 16, axis=1)]
        if len(core) == 0:
            core = px
        values, counts = np.unique(core, axis=0, return_counts=True)
        sampled[name] = {
            "rgb": tuple(int(v) for v in values[int(np.argmax(counts))]),
            "median": tuple(int(v) for v in median),
            "pixels": int(window.sum()),
            "core_pixels": int(len(core)),
        }
    return sampled


def analyse(source: Image.Image) -> dict:
    """Print and return the analysis of the source artwork."""
    rgb = np.array(source.convert("RGB")).astype(np.uint8)
    height, width = rgb.shape[:2]

    print(f"source            : {SOURCE_PNG.relative_to(REPO_ROOT)}")
    print(f"size              : {width} x {height} px  (mode {source.mode})")

    # Backgrounds: the paper panel and the tinted side margins.
    row = 30
    margins = np.concatenate(
        [rgb[row, :200].reshape(-1, 3), rgb[row, 1200:].reshape(-1, 3)]
    )
    margin_median = np.median(margins, axis=0)
    panel = rgb[row, 400:1000].reshape(-1, 3)
    panel_median = np.median(panel, axis=0)
    print(f"panel background  : {hex_of(panel_median)}  (white paper)")
    print(f"margin background : {hex_of(margin_median)}  (light-blue sides)")

    x0, y0, x1, y1 = content_bounding_box(rgb)
    print(
        f"content bbox      : x {x0}..{x1}, y {y0}..{y1}  "
        f"({x1 - x0 + 1} x {y1 - y0 + 1} px)"
    )

    # The generator sparkle lives outside the content box, so the crop drops it.
    sparkle = np.argwhere(
        (rgb.min(axis=2) < 244)
        & (np.arange(height)[:, None] > 560)
        & (np.arange(width)[None, :] > 1100)
    )
    if len(sparkle):
        sy = (sparkle[:, 0].min(), sparkle[:, 0].max())
        sx = (sparkle[:, 1].min(), sparkle[:, 1].max())
        print(
            f"corner sparkle    : x {sx[0]}..{sx[1]}, y {sy[0]}..{sy[1]} "
            f"({len(sparkle)} px) — outside the content box, removed by the crop"
        )

    brands = sample_brand_colours(rgb)
    print("brand colours     :")
    for name, info in brands.items():
        print(
            f"  {name:<7} sampled {hex_of(info['rgb'])}   "
            f"(median {hex_of(info['median'])}, {info['pixels']} px, "
            f"core {info['core_pixels']} px)"
        )

    # Contrast of the sampled colours against both GitHub themes.
    print("contrast (WCAG 2.1):")
    for name, info in brands.items():
        light = contrast_ratio(info["rgb"], (255, 255, 255))
        dark = contrast_ratio(info["rgb"], DARK_BACKGROUND)
        print(
            f"  {name:<7} {info['rgb']} -> #FFFFFF {light:5.2f}:1   "
            f"-> #0D1117 {dark:5.2f}:1"
        )

    return {
        "width": width,
        "height": height,
        "panel": tuple(int(v) for v in panel_median),
        "margin": tuple(int(v) for v in margin_median),
        "bbox": (x0, y0, x1, y1),
        "brands": brands,
    }


# --------------------------------------------------------------------------
# Step 2 — the transparent master
# --------------------------------------------------------------------------


def build_palette(brands: dict):
    """The ink palette: the sampled brand colours, as flat RGB triples."""
    palette = []
    for name in ("slate", "orange", "teal"):
        palette.append((name, tuple(brands[name]["rgb"])))
    return palette


def to_transparent(rgb: np.ndarray, palette):
    """Convert a white-paper RGB image into a transparent RGBA image.

    The paper is the only fully transparent colour. Every other pixel is
    treated as a blend of one of the palette inks with the paper, and the
    blend is solved for instead of thresholded:

        d = 255 - c = alpha * (255 - ink)

    The direction of ``d`` is the ink's chromaticity whatever the coverage
    is, so the ink is identified by direction, and the coverage follows from
    the projection of ``d`` onto that ink's deficit vector. The published
    colour is the recovered ink, not the white-veiled mixture the source
    pixels happen to hold, which is what keeps anti-aliased edges free of a
    white fringe.
    """
    c = rgb.astype(np.float64)
    d = 255.0 - c
    magnitude = np.linalg.norm(d, axis=2)

    best_alpha = np.zeros(rgb.shape[:2], dtype=np.float64)
    best_residual = np.full(rgb.shape[:2], np.inf)
    best_family = np.full(rgb.shape[:2], -1, dtype=np.int32)

    for index, (_name, ink) in enumerate(palette):
        e = 255.0 - np.array(ink, dtype=np.float64)
        denom = float(e @ e)
        alpha = np.clip((d @ e) / denom, 0.0, 1.0)
        residual = np.linalg.norm(d - alpha[..., None] * e, axis=2)
        better = residual < best_residual
        best_alpha = np.where(better, alpha, best_alpha)
        best_residual = np.where(better, residual, best_residual)
        best_family = np.where(better, index, best_family)

    # The paper, with its couple of units of noise, is fully transparent.
    paper = d.max(axis=2) < BACKGROUND_TOLERANCE
    alpha = np.where(paper, 0.0, best_alpha)
    alpha = np.where(alpha < ALPHA_FLOOR, 0.0, alpha)
    family = np.where(alpha <= 0.0, -1, best_family)

    out = np.zeros((*rgb.shape[:2], 4), dtype=np.uint8)
    for index, (_name, ink) in enumerate(palette):
        mask = family == index
        for channel in range(3):
            out[..., channel][mask] = ink[channel]
    out[..., 3] = np.round(alpha * 255.0).astype(np.uint8)
    return out, family


def master_from_source(source: Image.Image, brands: dict, bbox):
    """Crop the source to the padded content box and make the paper transparent."""
    x0, y0, x1, y1 = bbox
    pad_x = math.ceil(PAD_FRACTION * (x1 - x0 + 1))
    pad_y = math.ceil(PAD_FRACTION * (y1 - y0 + 1))
    left = max(0, x0 - pad_x)
    top = max(0, y0 - pad_y)
    right = min(source.width, x1 + pad_x + 1)
    bottom = min(source.height, y1 + pad_y + 1)

    cropped = source.convert("RGB").crop((left, top, right, bottom))
    palette = build_palette(brands)
    rgba, family = to_transparent(np.array(cropped), palette)
    return Image.fromarray(rgba, mode="RGBA"), family, (left, top, right, bottom)


# --------------------------------------------------------------------------
# Step 3 — variants
# --------------------------------------------------------------------------


def save_png(image: Image.Image, path: Path) -> int:
    """Write an sRGB PNG, optimised, and return its size in bytes."""
    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(path, format="PNG", optimize=True, compress_level=9)
    return path.stat().st_size


def remap_family_colours(rgba: np.ndarray, family: np.ndarray, palette, mapping):
    """Recolour a transparent image by family, keeping the recovered alpha."""
    out = rgba.copy()
    for index, (name, ink) in enumerate(palette):
        mask = family == index
        if not mask.any():
            continue
        target = mapping.get(name, ink)
        for channel in range(3):
            out[..., channel][mask] = target[channel]
    return Image.fromarray(out, mode="RGBA")


def dark_mapping(palette, brands):
    """The dark-theme colour map.

    The slate is always replaced by the light slate. Teal and orange are only
    lightened when their measured contrast against the dark page background
    falls below the minimum, so a colour that already reads is left alone.
    """
    mapping = {}
    report = []
    for name, ink in palette:
        ratio = contrast_ratio(ink, DARK_BACKGROUND)
        if name == "slate":
            target = DARK_SLATE
            reason = "recoloured to the light slate"
        elif ratio < MIN_DARK_CONTRAST:
            target = lighten(ink, MIN_DARK_CONTRAST)
            reason = f"lightened to reach {MIN_DARK_CONTRAST:.0f}:1"
        else:
            target = ink
            reason = f"kept: {ratio:.2f}:1 already reaches {MIN_DARK_CONTRAST:.0f}:1"
        mapping[name] = target
        report.append((name, ink, target, ratio, reason))
    return mapping, report


def lighten(rgb, target_ratio: float, background=DARK_BACKGROUND):
    """Scale a colour towards white until it reaches a contrast ratio."""
    lo, hi = 1.0, 100.0
    for _ in range(40):
        mid = (lo + hi) / 2.0
        candidate = tuple(int(round(c + (255 - c) * (mid - 1.0) / 99.0)) for c in rgb)
        if contrast_ratio(candidate, background) < target_ratio:
            lo = mid
        else:
            hi = mid
    factor = (hi - 1.0) / 99.0
    return tuple(int(round(c + (255 - c) * factor)) for c in rgb)


def crop_transparent(image: Image.Image, box) -> Image.Image:
    return image.crop(box)


def ink_bands(alpha: np.ndarray):
    """The horizontal bands of artwork in a transparent image, as (start, end) rows.

    A row counts as artwork when it carries real coverage, so the isolated
    specks the artwork's own noise leaves behind cannot be mistaken for a
    band and shift a crop.
    """
    ink = alpha >= INK_THRESHOLD
    bands = []
    start = None
    for y in range(len(ink)):
        if ink[y].any() and start is None:
            start = y
        elif not ink[y].any() and start is not None:
            bands.append((start, y - 1))
            start = None
    if start is not None:
        bands.append((start, len(ink) - 1))
    return bands


def tagline_start(bands):
    """The first row of the tagline.

    The artwork is a crest, a wordmark and a two-line tagline. The wordmark's
    capitals are much taller than the tagline's lines, so the tagline is the
    run of short bands that follows the last band tall enough to be display
    lettering.
    """
    tall = [i for i, (start, end) in enumerate(bands) if end - start + 1 >= TEXT_LINE_HEIGHT]
    if not tall or tall[-1] + 1 >= len(bands):
        return bands[-1][0]
    return bands[tall[-1] + 1][0]


def build_variants(master: Image.Image, family: np.ndarray, palette, brands):
    """Produce every published variant from the transparent master."""
    built = {}

    # (a) light full lockup -------------------------------------------------
    built["climb-logo-light.png"] = master

    # (b) dark full lockup --------------------------------------------------
    mapping, report = dark_mapping(palette, brands)
    dark_master = remap_family_colours(
        np.array(master), family, palette, mapping
    )
    built["climb-logo-dark.png"] = dark_master

    # (c) marks: the crest and the wordmark, without the tagline ------------
    # The mark is the master cut just above the tagline, which keeps the
    # master's own top padding and gives the wordmark the same breathing room
    # the tagline had.
    alpha = np.array(master)[..., 3]
    bands = ink_bands(alpha)
    tagline_top = tagline_start(bands)
    mark_box = (0, 0, master.width, tagline_top)
    built["climb-mark-light.png"] = crop_transparent(master, mark_box)
    built["climb-mark-dark.png"] = crop_transparent(dark_master, mark_box)

    # (d) square icon --------------------------------------------------------
    icon_source = master.crop(icon_box(master, bands))
    square = square_icon(icon_source)
    built["climb-icon.png"] = square.resize((512, 512), Image.LANCZOS)
    for size in (32, 180, 192):
        built[f"climb-icon-{size}.png"] = square.resize((size, size), Image.LANCZOS)

    # (e) social preview -----------------------------------------------------
    built["social-preview.png"] = social_preview(built["climb-mark-dark.png"])

    return built, report, mark_box, square


def icon_box(master: Image.Image, bands):
    """The bounding box of the crest alone: the ink of the first band."""
    top, bottom = bands[0]
    alpha = np.array(master)[..., 3]
    columns = np.nonzero((alpha[top : bottom + 1] >= INK_THRESHOLD).any(axis=0))[0]
    return (int(columns.min()), top, int(columns.max()) + 1, bottom + 1)


def square_icon(icon: Image.Image) -> Image.Image:
    """Pad an icon into a square canvas, keeping the master's padding ratio."""
    w, h = icon.size
    side = int(math.ceil(max(w, h) * (1.0 + 2.0 * PAD_FRACTION)))
    canvas = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    canvas.paste(icon, ((side - w) // 2, (side - h) // 2))
    return canvas


def write_media_table(built, icon_square) -> int:
    """Refresh the file table in the media README between its markers.

    The sizes are read back off disk, so the table cannot drift away from the
    files it describes.
    """
    if not MEDIA_README.exists():
        return 0
    text = MEDIA_README.read_text(encoding="utf-8")
    begin = text.find(MEDIA_TABLE_BEGIN)
    end = text.find(MEDIA_TABLE_END)
    if begin < 0 or end < 0 or end < begin:
        return 0

    rows = []
    for name, role in ASSET_ROLES:
        path = MEDIA_DIR / name
        if not path.exists():
            continue
        size = path.stat().st_size
        if name in built:
            image = built[name]
            dimensions = f"{image.width} × {image.height}"
        elif name.endswith(".svg"):
            dimensions = f"{icon_square.width} × {icon_square.height}"
        else:
            dimensions = "—"
        rows.append(f"| `{name}` | {role} | {size:,} B | {dimensions} |")

    table = "\n".join(
        ["| File | What it is | Size | Dimensions |", "|---|---|---|---|"] + rows
    )
    updated = (
        text[:begin]
        + MEDIA_TABLE_BEGIN
        + "\n"
        + table
        + "\n"
        + MEDIA_TABLE_END
        + text[end + len(MEDIA_TABLE_END) :]
    )
    MEDIA_README.write_text(updated, encoding="utf-8")
    return len(rows)


def social_preview(mark: Image.Image) -> Image.Image:
    """A 1280x640 card carrying the dark mark on the near-black background."""

    width, height = 1280, 640
    canvas = Image.new("RGBA", (width, height), PREVIEW_BACKGROUND + (255,))
    target_height = 360
    scale = target_height / mark.height
    target_width = int(round(mark.width * scale))
    resized = mark.resize((target_width, target_height), Image.LANCZOS)
    canvas.alpha_composite(
        resized, ((width - target_width) // 2, (height - target_height) // 2)
    )
    return canvas.convert("RGBA")


# --------------------------------------------------------------------------
# Step 4 — the vector rendition of the crest (optional)
# --------------------------------------------------------------------------


def trace_crest_svg(master: Image.Image, palette, box, work_dir: Path) -> str:
    """Trace the crest to SVG, one pass per ink family.

    The crest is flat: three ink colours on transparent paper. Each colour is
    traced on its own from a binary mask and the resulting paths are filled
    with the ink they came from, so the vector rendition carries the same
    geometry and the same colours as the raster master.

    Returns the SVG text, or an empty string when no tracer is installed.
    """
    tracer = shutil.which("vtracer")
    if tracer is None:
        return ""

    crest = master.crop(box)
    pixels = np.array(crest)
    colours, coverage = pixels[..., :3], pixels[..., 3]
    red, green, blue = (colours[..., i].astype(int) for i in range(3))
    solid = coverage > 127
    masks = {
        "slate": solid
        & (colours.max(axis=2) < 170)
        & (colours.max(axis=2) - colours.min(axis=2) <= 60),
        "orange": solid & (red > 130) & ((red - blue) > 55),
        "teal": solid & ((green - red) > 15) & ((blue - red) > 5),
    }

    width, height = crest.size
    groups = []
    for name, mask in masks.items():
        binary = np.where(mask, 0, 255).astype(np.uint8)
        mask_path = work_dir / f"{name}-mask.png"
        svg_path = work_dir / f"{name}.svg"
        Image.fromarray(binary, mode="L").convert("RGB").save(mask_path)
        result = subprocess.run(
            [
                tracer,
                "-i", str(mask_path),
                "-o", str(svg_path),
                "--colormode", "bw",
                "-m", "spline",
                "-f", "4",
                "-c", "60",
                "-l", "4.0",
                "-s", "45",
            ],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0 or not svg_path.exists():
            return ""
        text = svg_path.read_text(encoding="utf-8")
        inner = re.search(r"<svg[^>]*>(.*)</svg>", text, re.S)
        if inner is None:
            return ""
        body = re.sub(
            r'fill="#[0-9A-Fa-f]{6}"',
            f'fill="{hex_of(dict(palette)[name])}"',
            inner.group(1),
        )
        body = re.sub(r'stroke="[^"]*"', 'stroke="none"', body)
        groups.append(f'  <g id="{name}">\n{body.strip()}\n  </g>')

    return (
        '<svg xmlns="http://www.w3.org/2000/svg" '
        f'viewBox="0 0 {width} {height}" width="{width}" height="{height}">\n'
        + "\n".join(groups)
        + "\n</svg>\n"
    )


# --------------------------------------------------------------------------
# Brand palette file
# --------------------------------------------------------------------------


def write_brand_toml(analysis: dict, palette, dark_report) -> None:
    """Write the sampled brand colours to docs/media/brand.toml."""
    lines = [
        "# CLIMB brand colours, sampled from the original artwork.",
        "#",
        "# The values below are measured from docs/media/source/climb-logo-original.png",
        "# by scripts/logo/make_assets.py: for each ink family the most common colour",
        "# inside the dense core of that family is taken, so anti-aliased edge pixels",
        "# cannot drag the sample towards the white paper.",
        "",
        "[brand]",
        f'paper = "{hex_of(analysis["panel"])}"',
        f'panel_margin = "{hex_of(analysis["margin"])}"',
        "",
    ]
    for name, ink in palette:
        info = analysis["brands"][name]
        lines += [
            f"[brand.{name}]",
            f'hex = "{hex_of(ink)}"',
            f'rgb = [{ink[0]}, {ink[1]}, {ink[2]}]',
            f'family_median = "{hex_of(info["median"])}"',
            f'family_pixels = {info["pixels"]}',
            f'light_theme_contrast = {contrast_ratio(ink, (255, 255, 255)):.2f}',
            f'dark_theme_contrast = {contrast_ratio(ink, DARK_BACKGROUND):.2f}',
            "",
        ]
    lines += [
        "[dark_theme]",
        f'background = "{hex_of(DARK_BACKGROUND)}"',
        f'preview_background = "{hex_of(PREVIEW_BACKGROUND)}"',
        f'ink = "{hex_of(DARK_SLATE)}"',
        f"minimum_contrast = {MIN_DARK_CONTRAST:.1f}",
        "",
    ]
    for name, ink, target, ratio, reason in dark_report:
        lines += [
            f"[dark_theme.{name}]",
            f'source = "{hex_of(ink)}"',
            f'replacement = "{hex_of(target)}"',
            f'contrast_against_background = {ratio:.2f}',
            f'reason = "{reason}"',
            "",
        ]
    BRAND_TOML.parent.mkdir(parents=True, exist_ok=True)
    BRAND_TOML.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")


# --------------------------------------------------------------------------
# Entry point
# --------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--analyse",
        action="store_true",
        help="print the analysis of the source and stop",
    )
    args = parser.parse_args()

    if not SOURCE_PNG.exists():
        print(
            f"source artwork not found: {SOURCE_PNG}\n"
            "place the original 1408x768 PNG there and run this script again",
            file=sys.stderr,
        )
        return 1

    source = Image.open(SOURCE_PNG)
    analysis = analyse(source)
    if args.analyse:
        return 0

    palette = build_palette(analysis["brands"])
    master, family, crop = master_from_source(
        source, analysis["brands"], analysis["bbox"]
    )
    built, dark_report, mark_box, icon_square = build_variants(
        master, family, palette, analysis["brands"]
    )

    print()
    print(f"master             : crop {crop} -> {master.width} x {master.height} px")
    print(f"mark               : {master.width} x {mark_box[3]} px (tagline removed)")
    print("dark-variant colour decisions:")
    for name, ink, target, ratio, reason in dark_report:
        print(f"  {name:<7} {hex_of(ink)} -> {hex_of(target)}  ({reason})")

    print()
    print("written assets:")
    for name, image in built.items():
        size = save_png(image, MEDIA_DIR / name)
        print(f"  {name:<24} {image.width:>4} x {image.height:<4} px   {size:>8} B")

    # The vector rendition of the crest is optional: it needs a tracer on the
    # path, and the raster icon above is the published fallback.
    with tempfile.TemporaryDirectory() as work:
        svg = trace_crest_svg(icon_square, palette, (0, 0) + icon_square.size, Path(work))
    if svg:
        svg_path = MEDIA_DIR / "climb-icon.svg"
        svg_path.write_text(svg, encoding="utf-8")
        print(
            f"  {'climb-icon.svg':<24} {'':>13}   "
            f"{svg_path.stat().st_size:>8} B  (traced with vtracer)"
        )
    else:
        print(
            "  climb-icon.svg          skipped: no `vtracer` binary on PATH; "
            "the raster icon is the published rendition"
        )

    write_brand_toml(analysis, palette, dark_report)
    print(f"  {'brand.toml':<24} {'':>13}   {BRAND_TOML.stat().st_size:>8} B")

    rows = write_media_table(built, icon_square)
    print(f"  {'README.md table':<24} {rows:>3} row(s) refreshed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
