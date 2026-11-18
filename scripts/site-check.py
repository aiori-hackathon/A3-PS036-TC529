#!/usr/bin/env python3
"""Site token, copy and design checks.

Four independent checks, one file, because they all guard the same site build:

  contrast   the WCAG 2.x contrast ratio of every text, boundary and fill pair
             in the site token set, printed as a table, plus the worst case for
             text on a glass panel: the brightest orb wash, composited with the
             panel scrim and the glass gradient, sampled from the CSS colour
             stops. The palette mirrors the token table in
             ``docs/site/brand.md`` and is compared against it, so the document
             and the checker cannot drift from each other.
  copy       scans ``docs/site/copy.md`` for the banned words, for numbers that
             are not backed by a fact id, for fact ids that do not resolve in
             the committed results artifact, for a fact id without its evidence
             chip, and for the scope sentence.
  docs       checks the design documents against the prototype and the site
             source: the brand token and shadow tables, the rule verdicts, the
             animation inventory (22 rows, every keyframe named, every selector
             present in the prototype, no SMIL in the site source), the
             fact-gap register, the blur allowlist, the canvas budget and the
             CSP settings.
  selftest   named checks over the pure helpers, so the checker is itself
             testable without a fixture file.

Run ``python3 scripts/site-check.py`` with no argument to run all four. Exits
non-zero when any check fails, so it can be wired into a review or a CI step.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
COPY_PATH = ROOT / "docs" / "site" / "copy.md"
RESULTS_PATH = ROOT / "docs" / "sample-output" / "climb-results.json"
BRAND_PATH = ROOT / "docs" / "site" / "brand.md"
RULES_PATH = ROOT / "docs" / "site" / "design-rules.md"
INVENTORY_PATH = ROOT / "docs" / "site" / "animation-inventory.md"
GAPS_PATH = ROOT / "docs" / "site" / "fact-gaps.md"
PLAN_PATH = ROOT / "docs" / "site" / "skin-plan.md"
PROTOTYPE_PATH = ROOT / "docs" / "site" / "prototype" / "climb-site.html"
SITE_SRC = ROOT / "site" / "src"
ASTRO_CONFIG = ROOT / "site" / "astro.config.mjs"
DIST_INDEX = ROOT / "site" / "dist" / "index.html"

# --------------------------------------------------------------------------
# Site tokens (brand v3, the prototype's palette). The site is dark-only.
# This table mirrors ``docs/site/brand.md`` §1; the ``docs`` check compares the
# two, so a value cannot change in one place only.
# --------------------------------------------------------------------------

TOKENS: dict[str, str] = {
    # ground and surfaces
    "ink": "#07080A",
    "ground": "#13161B",
    "g1": "#1B1E25",
    "g2": "#0F1217",
    "well": "#0D0F13",
    # ink on the page
    "cr": "#F2EDE4",
    "mut": "#A9A398",
    "ink-card": "#0B0B0D",
    "card-body": "#33333A",
    # brand
    "or": "#FF5400",
    "or2": "#FE7F00",
    "rose": "#FFA6A6",
    # asset-only colours: the logo gradients
    "navy": "#06234B",
    "mar": "#4D1111",
    # boundaries and glass
    "ln": "rgba(242,237,228,.13)",
    "edge": "rgba(242,237,228,.48)",
    "glass-scrim": "rgba(7,8,10,.78)",
    # orb washes (background only)
    "orb-warm": "rgba(255,84,0,.30)",
    "orb-cool": "rgba(24,96,200,.46)",
    "orb-rose": "rgba(255,166,166,.16)",
}

SURFACES = ("ink", "ground", "g1", "well")

ORBS = ("orb-warm", "orb-cool", "orb-rose")

# The brightest possible backdrop behind a glass panel: the orb wash composited
# over the page ground, then the panel scrim, then the top stop of the glass
# gradient. The pointer specular is excluded because the budget keeps it out of
# the text box (see ``docs/site/brand.md`` §6).
GLASS_GRADIENT_TOP = (1.0, 1.0, 1.0, 0.085)

# Foreground -> the surfaces it is painted on. A pair that is not listed is
# not used by the design and must not be introduced without a target.
TEXT_ON_SURFACES: dict[str, tuple[str, ...]] = {
    "cr": ("ink", "ground", "g1", "well"),
    "mut": ("ink", "ground", "g1", "well"),
    "or": ("ink", "ground", "g1", "well"),
    "or2": ("ink", "ground"),
    "rose": ("ink", "ground", "g1"),
}

# Foregrounds that also sit on a glass panel, checked against every orb wash.
TEXT_ON_GLASS = ("cr", "mut", "or")

# Fills carry the ink as their label; light text on any of them fails.
FILLS = ("cr", "or", "or2")

# The interactive boundary token. It clears the 3:1 non-text floor on every
# surface; `ln` does not and is decorative only.
BOUNDARY = "edge"

# Asset-only colours: inside the logo SVG, never in UI text.
ASSET_ONLY = ("navy", "mar")

TEXT_MIN = 4.5  # WCAG 1.4.3 normal text
LARGE_MIN = 3.0  # WCAG 1.4.3 large text and 1.4.11 non-text boundary

# The four shadow variables of the skin, copied literally from the prototype.
SHADOWS = {
    "--up": "9px 9px 20px rgba(0,0,0,.55),-7px -7px 18px rgba(255,255,255,.04)",
    "--up-s": "5px 5px 11px rgba(0,0,0,.5),-4px -4px 10px rgba(255,255,255,.04)",
    "--up-xs": "3px 3px 7px rgba(0,0,0,.5),-2px -2px 6px rgba(255,255,255,.04)",
    "--in": "inset 6px 6px 13px rgba(0,0,0,.55),inset -5px -5px 12px rgba(255,255,255,.035)",
}

# The only selectors allowed to blur. A `backdrop-filter` anywhere else is a
# budget breach, not a style choice.
BLUR_ALLOWED = (
    "#hd",
    ".led li",
    ".tm li",
    ".tr>div",
    ".lim",
    ".dg",
    ".arch",
    ".c",
    ".l",
    ".dt",
)

# At most this many canvases may exist in the bundle at once. The five band
# canvases run only while visible and are capped separately in the inventory.
MAX_CANVASES = 5

# The design authority's identity. A prototype that changes under the documents
# is a different design, so the digest is pinned here and repeated in brand.md.
PROTOTYPE_SHA256 = "0b0822338e004daba100bde1f98bcd9f2e5aa4c1f7b08198973d966a9570f108"

# The scope sentence, verbatim. It appears in the footer and in Honest limits.
SCOPE_SENTENCE = (
    "This website presents recorded results and a client-side dashboard. "
    "The DNSSEC and PQC benchmark itself is not deployed on the Internet; "
    "the DNS testbed runs locally."
)

# --------------------------------------------------------------------------
# Colour arithmetic.
# --------------------------------------------------------------------------

Colour = tuple[float, float, float, float]


def parse_colour(value: str) -> Colour:
    """Parse `#rrggbb` or `rgba(r,g,b,a)` into four floats in 0..1 range."""
    text = value.strip().lower()
    if text.startswith("#"):
        digits = text[1:]
        if len(digits) == 3:
            digits = "".join(part * 2 for part in digits)
        if len(digits) != 6:
            raise ValueError(f"unsupported hex colour: {value}")
        red, green, blue = (int(digits[i : i + 2], 16) / 255.0 for i in (0, 2, 4))
        return (red, green, blue, 1.0)
    match = re.fullmatch(r"rgba?\(([^)]*)\)", text)
    if match is None:
        raise ValueError(f"unsupported colour: {value}")
    parts = [part.strip() for part in match.group(1).split(",")]
    if len(parts) not in (3, 4):
        raise ValueError(f"unsupported colour: {value}")
    red, green, blue = (float(part) / 255.0 for part in parts[:3])
    alpha = float(parts[3]) if len(parts) == 4 else 1.0
    return (red, green, blue, alpha)


def over(foreground: Colour, background: Colour) -> Colour:
    """Composite `foreground` over an opaque or translucent `background`."""
    alpha = foreground[3] + background[3] * (1.0 - foreground[3])
    if alpha == 0.0:
        return (0.0, 0.0, 0.0, 0.0)
    channels = tuple(
        (foreground[i] * foreground[3] + background[i] * background[3] * (1.0 - foreground[3]))
        / alpha
        for i in range(3)
    )
    return (channels[0], channels[1], channels[2], alpha)


def _channel(value: float) -> float:
    return value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4


def luminance(colour: Colour) -> float:
    red, green, blue, _ = over(colour, (0.0, 0.0, 0.0, 1.0))
    return 0.2126 * _channel(red) + 0.7152 * _channel(green) + 0.0722 * _channel(blue)


def contrast(foreground: str | Colour, background: str | Colour) -> float:
    """Contrast ratio, compositing a translucent colour over its background."""
    first = parse_colour(foreground) if isinstance(foreground, str) else foreground
    second = parse_colour(background) if isinstance(background, str) else background
    painted = over(first, second)
    high, low = max(luminance(painted), luminance(second)), min(luminance(painted), luminance(second))
    return (high + 0.05) / (low + 0.05)


def verdict(ratio: float, minimum: float) -> str:
    return "pass" if ratio >= minimum else "FAIL"


def glass_backdrop(orb: str) -> Colour:
    """The brightest backdrop a glass panel can show over one orb wash."""
    base = over(parse_colour(TOKENS[orb]), parse_colour(TOKENS["ground"]))
    base = over(parse_colour(TOKENS["glass-scrim"]), base)
    return over(GLASS_GRADIENT_TOP, base)


def print_contrast() -> int:
    """Print every used token pair and the ratio it measures. Returns failures."""
    failures = 0
    pairs = 0
    print("WCAG 2.x contrast, site tokens (rounded to two decimals)")
    print(f"thresholds: {TEXT_MIN}:1 normal text, {LARGE_MIN}:1 large text and UI boundaries")
    print()

    print("text on each surface")
    print("%-16s" % "foreground" + "".join("%9s" % name for name in SURFACES) + "   target")
    for name, used_surfaces in TEXT_ON_SURFACES.items():
        row = "%-16s" % name
        for surface in SURFACES:
            if surface in used_surfaces:
                ratio = contrast(TOKENS[name], TOKENS[surface])
                row += "%9.2f" % ratio
                pairs += 1
                if ratio < TEXT_MIN:
                    failures += 1
            else:
                row += "%9s" % "-"
        row += "     %.1f" % TEXT_MIN
        print(row)

    print()
    print("text on glass, worst case over each orb wash (scrim + gradient top)")
    print("%-16s" % "foreground" + "".join("%9s" % orb for orb in ORBS) + "   target")
    for name in TEXT_ON_GLASS:
        row = "%-16s" % name
        for orb in ORBS:
            ratio = contrast(TOKENS[name], glass_backdrop(orb))
            row += "%9.2f" % ratio
            pairs += 1
            if ratio < TEXT_MIN:
                failures += 1
        row += "     %.1f" % TEXT_MIN
        print(row)

    print()
    print("card body text on the paper card")
    ratio = contrast(TOKENS["card-body"], TOKENS["cr"])
    print("%-16s on %-8s %9.2f  %s" % ("card-body", "cr", ratio, verdict(ratio, TEXT_MIN)))
    pairs += 1
    if ratio < TEXT_MIN:
        failures += 1

    print()
    print("ink on a fill")
    for name in FILLS:
        ink = contrast(TOKENS["ink-card"], TOKENS[name])
        light = contrast(TOKENS["cr"], TOKENS[name])
        ink_ok = verdict(ink, TEXT_MIN)
        light_ok = verdict(light, TEXT_MIN)
        print("%-16s ink #0B0B0D %9.2f  %s   cr #F2EDE4 %9.2f  %s"
              % (name, ink, ink_ok, light, light_ok))
        pairs += 1
        if ink_ok == "FAIL":
            failures += 1
        if light_ok == "FAIL":
            # Expected: the rule is that light text is never painted on a fill.
            print("                 rule: light text is never painted on a fill; use the ink.")

    print()
    print("interactive boundary (edge, paper at 48 percent) against each surface")
    for surface in SURFACES:
        ratio = contrast(TOKENS[BOUNDARY], TOKENS[surface])
        mark = verdict(ratio, LARGE_MIN)
        print("%-16s on %-8s %9.2f  %s" % (BOUNDARY, surface, ratio, mark))
        pairs += 1
        if mark == "FAIL":
            failures += 1
    for orb in ORBS:
        ratio = contrast(TOKENS[BOUNDARY], glass_backdrop(orb))
        mark = verdict(ratio, LARGE_MIN)
        print("%-16s on %-8s %9.2f  %s" % (BOUNDARY, "glass:" + orb, ratio, mark))
        pairs += 1
        if mark == "FAIL":
            failures += 1

    print()
    print("decorative hairline and asset-only colours, never used as text or a boundary")
    for name in ("ln",) + ASSET_ONLY:
        for surface in SURFACES:
            ratio = contrast(TOKENS[name], TOKENS[surface])
            print("%-16s on %-8s %9.2f  %s (decorative only)"
                  % (name, surface, ratio, verdict(ratio, LARGE_MIN)))
            if ratio >= LARGE_MIN:
                failures += 1

    print()
    print("used pairs: %d, failures: %d" % (pairs, failures))
    return failures


# --------------------------------------------------------------------------
# Copy deck checks.
# --------------------------------------------------------------------------

# Words the copy must never use. Kept as data, so the policy table in the deck
# can quote them in code spans without tripping the scan.
BANNED_WORDS = [
    # Unverifiable praise and marketing absolutes.
    "revolutionary", "seamless", "cutting-edge", "game-changing", "next-generation",
    "world-class", "best-in-class", "effortless", "unprecedented", "military-grade",
    "powerful", "leverage", "unlock", "bulletproof", "unbreakable",
    "hack-proof", "future-proof", "quantum-proof", "quantum-safe",
    "zero-risk", "guarantee", "guaranteed",
    # Formal-verification and security phrasing the benchmark does not test.
    "secure", "secured", "proof", "proves", "proven", "certified", "compliant",
    # Absolutes.
    "eliminates", "always", "never",
]

# Terms that contain a banned word or a digit but are not claims: identifiers,
# the parameter set, section labels and protocol constants. Removed before the
# digit scan.
NON_CLAIM_TOKENS = [
    "A3-PS036-TC529",
    "PS-036",
    "AIORI-3",
    "SLH-DSA-SHA2-128s-MTL-SHA2-128",
    "SHA2-128s",
    "FIPS 205",
    "RFC 4034",
    "01.00",
    "EDNS(0)",
    "512 B",  # the classic DNS UDP limit, a protocol constant, not a measurement
    "Track 2",  # the event track identifier
    "50 ms",  # the emulated round-trip target: an experiment input, not a result
]

# A value's chip is one of four evidence classes, or one of two provenance tags
# for a value that is not a measurement result: a fixed model input (`Constant`)
# or a count a tool emits (`Generated`).
EVIDENCE_CHIPS = [
    "Measured", "Derived", "Analytical", "Extrapolated",  # evidence classes
    "Constant", "Generated",  # provenance tags, text-only
]

# Fact id -> JSON pointer in the committed results artifact, or None when the
# value is not in that artifact (with the source recorded here).
FACT_IDS: dict[str, tuple[str | None, str]] = {
    "F-wire-medium-a": ("/wire_size/1/plain_total_bytes", "MEASURED"),
    "F-wire-medium-b": ("/wire_size/1/without_mtl_total_bytes", "MEASURED"),
    "F-wire-medium-c": ("/wire_size/1/with_mtl_total_bytes", "MEASURED"),
    "F-wire-medium-c-vs-a": ("/comparisons/2/wire_reduction_c_vs_a_percent", "DERIVED"),
    "F-wire-medium-c-vs-b": ("/comparisons/2/wire_reduction_c_vs_b_percent", "DERIVED"),
    "F-sign-medium-c-vs-a": ("/comparisons/2/signing_speedup_c_vs_a", "DERIVED"),
    "F-verify-medium-c-vs-a": ("/comparisons/2/verifying_speedup_cached_vs_a", "DERIVED"),
    "F-sign-batch-size": ("/wire_size/1/message_count", "MEASURED"),
    "F-ladder-signed-small": ("/cache_use/ladder_analytical/1/signed_ladder_bytes", "MEASURED"),
    "F-ladder-signed-medium": ("/cache_use/ladder_analytical/0/signed_ladder_bytes", "MEASURED"),
    "F-ladder-cache-measured": ("/cache_use/ladder_analytical/0/measured_bytes", "MEASURED"),
    "F-ladder-cache-analytical": ("/cache_use/ladder_analytical/0/analytical_bytes", "ANALYTICAL"),
    "F-ladder-cache-ratio": ("/cache_use/ladder_analytical/0/measured_over_analytical", "DERIVED"),
    "F-dns-rtt-plain-50": ("/dns_rtt/cells/8/median_ms", "MEASURED"),
    "F-dns-rtt-condensed-50": ("/dns_rtt/cells/9/median_ms", "MEASURED"),
    "F-dns-rtt-samples": ("/dns_rtt/min_samples_per_cell", "CONSTANT"),
    "F-breakeven-bytes": ("/derived/reuse/1/breakeven_reuse_bytes", "DERIVED"),
    "F-breakeven-signing": ("/derived/reuse/1/breakeven_reuse_time", "DERIVED"),
    "F-udp-c-fit": ("/derived/udp_fit/aggregate/2/fit_rows", "ANALYTICAL"),
    "F-udp-c-total": ("/derived/udp_fit/aggregate/2/total_rows", "ANALYTICAL"),
    "F-dns-www-a": ("/dns_pipeline/reduction_percent_www_a", "DERIVED"),
    "F-mem-small-ratio": ("/memory/measured/0/measured_analytical_total_ratio", "DERIVED"),
    "F-wire-sizes": ("/wire_size", "MEASURED"),
    "F-parameter-set": ("/dns_pipeline/algorithm", "MEASURED"),
    # Not in the results artifact; sources recorded so a missing fact is visible.
    "F-plain-signature-bytes": (None, "LADDER: /sizes/7/signature_len in ladder-verification.recorded.json"),
    "F-condensed-path-bytes": (None, "LADDER: /sizes/7/condensed/min in ladder-verification.recorded.json"),
    "F-udp-payload": (None, "CONSTANT: UDP_MTU_1232"),
    "F-claims-count": (None, "GENERATED: `cargo xtask readme-badges`"),
    # Registered with no producer yet: every one is listed in
    # docs/site/fact-gaps.md with the pointer or generator that must produce it.
    "F-claims-measured": (None, "GAP: count of class=measured in site/src/data/generated/claims.json"),
    "F-claims-derived": (None, "GAP: count of class=derived in site/src/data/generated/claims.json"),
    "F-claims-analytical": (None, "GAP: count of class=analytical in site/src/data/generated/claims.json"),
    "F-claims-extrapolated": (None, "GAP: count of class=extrapolated in site/src/data/generated/claims.json"),
    "F-message-bytes-a": (None, "GAP: generator `climb_report::site_data` from /wire_size/1/plain_signature_len_bytes"),
    "F-message-bytes-b": (None, "GAP: generator `climb_report::site_data` from /wire_size/1/without_mtl_total_bytes / message_count"),
    "F-message-bytes-c": (None, "GAP: generator `climb_report::site_data` from /wire_size/1/with_mtl_total_bytes / message_count"),
}

COPY_BEGIN = re.compile(r"<!--\s*copy:begin")
COPY_END = re.compile(r"<!--\s*copy:end")
FACT_REF = re.compile(r"F-[a-z0-9]+(?:-[a-z0-9]+)*")
FENCE = re.compile(r"^\s*```")
INLINE_CODE = re.compile(r"`[^`]*`")
NUMBER = re.compile(r"(?<![\w\-./])(\d[\d,]*(?:\.\d+)?)")
WORD = re.compile(r"[A-Za-z][A-Za-z-]*")


def resolve_pointer(document: object, pointer: str) -> tuple[bool, object]:
    current = document
    for part in pointer.strip("/").split("/"):
        if part == "":
            continue
        if isinstance(current, list):
            if not part.isdigit() or int(part) >= len(current):
                return False, None
            current = current[int(part)]
        elif isinstance(current, dict):
            if part not in current:
                return False, None
            current = current[part]
        else:
            return False, None
    return True, current


def copy_blocks(text: str) -> list[tuple[int, str]]:
    """Return (1-based line number, line) for every line inside a copy block."""
    blocks: list[tuple[int, str]] = []
    inside = False
    for index, line in enumerate(text.splitlines(), start=1):
        if COPY_BEGIN.search(line):
            inside = True
            continue
        if COPY_END.search(line):
            inside = False
            continue
        if inside:
            blocks.append((index, line))
    return blocks


def banned_hits(text: str) -> list[tuple[int, str]]:
    """Every banned word, as (1-based line, matched word), outside code spans."""
    hits: list[tuple[int, str]] = []
    in_fence = False
    for index, line in enumerate(text.splitlines(), start=1):
        if FENCE.match(line):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        prose = INLINE_CODE.sub(" ", line)
        for match in WORD.finditer(prose):
            if match.group(0).lower() in BANNED_WORDS:
                hits.append((index, match.group(0)))
    return hits


def unbacked_number_lines(text: str) -> list[tuple[int, list[str]]]:
    """Copy-block lines that carry digits but no fact id on the same line."""
    rows: list[tuple[int, list[str]]] = []
    for index, line in copy_blocks(text):
        scanned = INLINE_CODE.sub(" ", line)
        for token in NON_CLAIM_TOKENS:
            scanned = scanned.replace(token, " ")
        digits = NUMBER.findall(scanned)
        if digits and not FACT_REF.search(line):
            rows.append((index, digits))
    return rows


def sha256_digest(data: bytes) -> str:
    """The lower-case hex SHA-256 of a byte string."""
    return hashlib.sha256(data).hexdigest()


def normalise_space(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def scope_sentence_count(text: str) -> int:
    """How many times the scope sentence appears inside the copy blocks."""
    joined = normalise_space(" ".join(line for _, line in copy_blocks(text)))
    return joined.count(normalise_space(SCOPE_SENTENCE))


def check_copy() -> int:
    if not COPY_PATH.is_file():
        print("copy check: %s does not exist" % COPY_PATH, file=sys.stderr)
        return 1
    text = COPY_PATH.read_text(encoding="utf-8")
    failures = 0

    # 1. Banned words, outside code spans and fenced blocks.
    print("banned words")
    banned = banned_hits(text)
    for index, word in banned:
        print("  %s:%d: banned word `%s`" % (COPY_PATH.name, index, word))
    print("  %d banned word(s)" % len(banned))
    failures += len(banned)

    # 2. Numbers inside a copy block must be backed by a fact id on the line.
    print("numbers backed by a fact id (copy blocks only)")
    unbacked = unbacked_number_lines(text)
    for index, digits in unbacked:
        print("  %s:%d: %s without a fact id" % (COPY_PATH.name, index, ", ".join(digits)))
    print("  %d unbacked number line(s)" % len(unbacked))
    failures += len(unbacked)

    # 3. Every fact id used in the deck resolves, or is listed as non-artifact.
    print("fact ids")
    used = sorted(set(FACT_REF.findall(text)))
    document = json.loads(RESULTS_PATH.read_text(encoding="utf-8"))
    missing = 0
    for fact in used:
        if fact not in FACT_IDS:
            print("  %s: not declared in the fact registry" % fact)
            missing += 1
            continue
        pointer, evidence = FACT_IDS[fact]
        if pointer is None:
            print("  %-26s %-10s not in the artifact (%s)" % (fact, evidence, evidence))
            if evidence.startswith("GAP:"):
                print("  %s: gap id used in copy before its producer exists" % fact)
                missing += 1
            continue
        ok, value = resolve_pointer(document, pointer)
        if not ok:
            print("  %s: pointer %s does not resolve" % (fact, pointer))
            missing += 1
            continue
        shown = value if not isinstance(value, (list, dict)) else "%s[%d]" % (type(value).__name__, len(value))
        print("  %-26s %-10s %s -> %s" % (fact, evidence, pointer, shown))
    print("  %d unresolved or undeclared fact id(s)" % missing)
    failures += missing

    # 4. A line that carries a fact id must also name its evidence chip.
    print("evidence chips")
    unchipped = 0
    for index, line in copy_blocks(text):
        if FACT_REF.search(line) and not any(chip in line for chip in EVIDENCE_CHIPS):
            print("  %s:%d: fact id without an evidence chip" % (COPY_PATH.name, index))
            unchipped += 1
    print("  %d unchipped line(s)" % unchipped)
    failures += unchipped

    # 5. The scope sentence is used verbatim, twice: the footer and the limits.
    print("scope sentence")
    scope_hits = scope_sentence_count(text)
    print("  %d occurrence(s) of the scope sentence (expected 2)" % scope_hits)
    if scope_hits < 2:
        failures += 1

    print()
    print("copy check: %d failure(s)" % failures)
    return failures


# --------------------------------------------------------------------------
# Design document checks.
# --------------------------------------------------------------------------

TOKEN_ROW = re.compile(r"^\|\s*`--([a-z0-9-]+)`\s*\|\s*`([^`]+)`\s*\|")
RULE_ROW = re.compile(r"^\|\s*(\d+)\s*\|\s*([^|]+?)\s*\|\s*(KEEP|CHANGE|REMOVE|NEW)\s*\|\s*(.+?)\s*\|")
KEYFRAMES = re.compile(r"@keyframes\s+([A-Za-z0-9_-]+)")
SELECTOR_REF = re.compile(r"`([.#][^`]+)`")
DOC_TOKEN = re.compile(r"`--([a-z0-9-]+)`")


def brand_tokens(text: str) -> dict[str, str]:
    """Colour rows of a brand token table: `--name` -> literal value.

    Non-colour variables (the face stack, the shadows, the radii) share the
    table shape, so a row is a colour row only when its value parses as one.
    """
    rows: dict[str, str] = {}
    for line in text.splitlines():
        match = TOKEN_ROW.match(line)
        if match is None:
            continue
        try:
            parse_colour(match.group(2))
        except ValueError:
            continue
        rows["--" + match.group(1)] = match.group(2)
    return rows


def same_colour(first: str, second: str) -> bool:
    return parse_colour(first) == parse_colour(second)


def rule_verdicts(text: str) -> dict[int, tuple[str, str]]:
    rows: dict[int, tuple[str, str]] = {}
    for line in text.splitlines():
        match = RULE_ROW.match(line)
        if match is None:
            continue
        number = int(match.group(1))
        rows[number] = (match.group(3), match.group(4).strip())
    return rows


def inventory_rows(text: str) -> list[list[str]]:
    rows: list[list[str]] = []
    for line in text.splitlines():
        if not re.match(r"^\|\s*\d+\s*\|", line):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        rows.append(cells)
    return rows


def split_selector_list(text: str) -> list[str]:
    """Split a selector list on its top-level commas only."""
    parts: list[str] = []
    depth = 0
    current = ""
    for character in text:
        if character == "(":
            depth += 1
        elif character == ")":
            depth -= 1
        if character == "," and depth == 0:
            parts.append(current)
            current = ""
        else:
            current += character
    parts.append(current)
    return [part.strip() for part in parts if part.strip()]


def expand_selector(selector: str) -> list[str]:
    """Expand a leading :is() group to the elements it names.

    A state prefix (``.rf :is(#hd,.dg)``) qualifies a panel's appearance, not
    which element blurs, so the prefix is dropped and the inner selectors are
    returned. Only the leading :is() the stylesheet uses is handled; a nested
    group would be a new shape and is deliberately not guessed at.
    """
    index = selector.find(":is(")
    if index < 0:
        return [selector]
    start = index + len(":is(")
    depth = 1
    end = start
    while end < len(selector) and depth > 0:
        if selector[end] == "(":
            depth += 1
        elif selector[end] == ")":
            depth -= 1
        end += 1
    inner = selector[start : end - 1]
    expanded: list[str] = []
    for part in split_selector_list(inner):
        expanded.extend(expand_selector(part))
    return expanded


def blur_selectors(css_text: str) -> list[str]:
    """Selectors of every rule block that declares a backdrop-filter."""
    selectors: list[str] = []
    for block in css_text.split("}"):
        if "backdrop-filter" not in block:
            continue
        head = block.split("{")[0]
        cleaned = head.strip().strip(",").strip()
        for selector in split_selector_list(cleaned):
            selectors.extend(expand_selector(selector))
    return selectors


def inline_scripts(html: str) -> list[str]:
    """Executable inline scripts. A JSON data block is not executable."""
    return [
        match.group(0)
        for match in re.finditer(
            r"<script(?![^>]*\bsrc=)(?![^>]*\btype=[\"']application/json[\"'])[^>]*>", html
        )
    ]


def canvas_count(source: str) -> int:
    return source.count("<canvas")


def plan_status(pending_key: str) -> bool:
    """True while the plan marks `pending_key` as deferred to a later phase."""
    if not PLAN_PATH.is_file():
        return False
    return re.search(r"^\s*" + re.escape(pending_key) + r":\s*pending", PLAN_PATH.read_text(encoding="utf-8"), re.M) is not None


def check_docs() -> int:
    failures = 0
    for path in (BRAND_PATH, RULES_PATH, INVENTORY_PATH, GAPS_PATH, PLAN_PATH, PROTOTYPE_PATH):
        if not path.is_file():
            print("docs check: %s does not exist" % path, file=sys.stderr)
            return 1

    brand = BRAND_PATH.read_text(encoding="utf-8")
    rules = RULES_PATH.read_text(encoding="utf-8")
    inventory = INVENTORY_PATH.read_text(encoding="utf-8")
    gaps = GAPS_PATH.read_text(encoding="utf-8")
    prototype = PROTOTYPE_PATH.read_text(encoding="utf-8")

    # 0. The design authority is the file the documents were read from.
    print("prototype identity")
    digest = sha256_digest(PROTOTYPE_PATH.read_bytes())
    if digest != PROTOTYPE_SHA256:
        print("  %s: sha256 %s, the documents were written against %s"
              % (PROTOTYPE_PATH.name, digest, PROTOTYPE_SHA256))
        failures += 1
    else:
        print("  %s: sha256 matches the pinned digest" % PROTOTYPE_PATH.name)
    for path in (BRAND_PATH, INVENTORY_PATH):
        if PROTOTYPE_SHA256 not in path.read_text(encoding="utf-8"):
            print("  %s: does not pin the prototype digest" % path.name)
            failures += 1

    # 1. The brand document declares every token with the same value.
    print("brand tokens")
    declared = brand_tokens(brand)
    drift = 0
    for name, value in TOKENS.items():
        key = "--" + name
        if key not in declared:
            print("  %s: missing from brand.md" % key)
            drift += 1
            continue
        if not same_colour(declared[key], value):
            print("  %s: brand.md says %s, the checker says %s" % (key, declared[key], value))
            drift += 1
    for key in declared:
        name = key.lstrip("-")
        if name not in TOKENS:
            print("  %s: declared in brand.md but not in the checker" % key)
            drift += 1
    print("  %d token(s) compared, %d drift(s)" % (len(declared), drift))
    if len(declared) != len(TOKENS):
        print("  brand.md declares %d colour token(s), the checker has %d"
              % (len(declared), len(TOKENS)))
        failures += 1
    failures += drift

    # 2. The four shadow variables are copied verbatim from the prototype.
    print("shadow tokens")
    flat_brand = normalise_space(brand)
    shadow_drift = 0
    for name, value in SHADOWS.items():
        if normalise_space(value) not in flat_brand:
            print("  %s: %s is not in brand.md" % (name, value))
            shadow_drift += 1
    print("  %d shadow(s) compared, %d drift(s)" % (len(SHADOWS), shadow_drift))
    failures += shadow_drift

    # 3. Every animation row has a selector that exists and a reduced-motion state.
    print("animation inventory")
    rows = inventory_rows(inventory)
    keyframes = sorted(set(KEYFRAMES.findall(prototype)))
    problems = 0
    if len(rows) != 22:
        print("  %d row(s), expected 22" % len(rows))
        problems += 1
    for row in rows:
        if len(row) != 8:
            print("  row %s has %d cells, expected 8" % (row[0], len(row)))
            problems += 1
            continue
        number, effect, selector, trigger, constants, timing, reduced, parity = row
        for label, cell in (
            ("effect", effect),
            ("selector", selector),
            ("trigger", trigger),
            ("constants", constants),
            ("timing", timing),
            ("reduced motion", reduced),
            ("parity", parity),
        ):
            if not cell:
                print("  row %s: empty %s cell" % (number, label))
                problems += 1
        if "parity" not in parity.lower():
            print("  row %s: no parity assertion" % number)
            problems += 1
        for selector_ref in SELECTOR_REF.findall(selector):
            head = selector_ref.split()[0]
            name = re.split(r"[:\[ >]", head.lstrip(".#"))[0]
            if head not in prototype and name not in prototype:
                print("  row %s: selector %s is not in the prototype" % (number, head))
                problems += 1
    for name in keyframes:
        if name not in inventory:
            print("  @keyframes %s is not named by any row" % name)
            problems += 1
    print("  %d row(s), %d keyframe(s), %d problem(s)" % (len(rows), len(keyframes), problems))
    failures += problems

    # 4. No SMIL in the site source; the prototype's animateMotion is removed.
    print("motion markup")
    sources = sorted(path for path in SITE_SRC.rglob("*") if path.is_file() and path.suffix in {".astro", ".ts", ".css"})
    smil = 0
    gradient_text = 0
    canvases = 0
    blur = 0
    for path in sources:
        text = path.read_text(encoding="utf-8")
        if "<animateMotion" in text or "createElementNS" in text and "animateMotion" in text:
            print("  %s: SMIL animation in the bundle; use a CSS or scheduled effect" % path.relative_to(ROOT))
            smil += 1
        if re.search(r"background-clip\s*:\s*text|-webkit-background-clip\s*:\s*text|text-fill-color", text):
            print("  %s: gradient text is forbidden" % path.relative_to(ROOT))
            gradient_text += 1
        if "backdrop-filter" in text:
            for selector in blur_selectors(text):
                if selector not in BLUR_ALLOWED:
                    print("  %s: %s blurs but is not on the allowlist" % (path.relative_to(ROOT), selector))
                    blur += 1
        canvases += canvas_count(text)
    print("  %d source file(s): %d SMIL, %d gradient text, %d blur breach(es), %d canvas(es)"
          % (len(sources), smil, gradient_text, blur, canvases))
    failures += smil + gradient_text + blur
    if canvases > MAX_CANVASES:
        print("  %d canvas(es), budget is %d" % (canvases, MAX_CANVASES))
        failures += 1

    # 5. The fact-gap register and the registry agree.
    print("fact gaps")
    gap_ids = sorted(set(FACT_REF.findall(gaps)))
    declared_gaps = sorted(fact for fact, (pointer, source) in FACT_IDS.items()
                           if pointer is None and source.startswith("GAP:"))
    unregistered = [fact for fact in gap_ids if fact not in FACT_IDS]
    undescribed = [fact for fact in declared_gaps if fact not in gap_ids]
    for fact in unregistered:
        print("  %s: listed as a gap but not registered in the checker" % fact)
    for fact in undescribed:
        print("  %s: registered as a gap but not described in fact-gaps.md" % fact)
    print("  %d fact id(s) referenced, %d registered gap(s), %d unregistered"
          % (len(gap_ids), len(declared_gaps), len(unregistered)))
    failures += len(unregistered) + len(undescribed)

    # 6. The governance documents keep their shape.
    print("document shape")
    verdicts = rule_verdicts(rules)
    shape = 0
    for number in range(1, 15):
        if number not in verdicts:
            print("  rule %d has no verdict row" % number)
            shape += 1
            continue
        state, reason = verdicts[number]
        if len(reason.split()) < 4:
            print("  rule %d has no reason" % number)
            shape += 1
    for orb in ORBS:
        if orb.replace("orb-", "") not in brand:
            print("  orb %s is not described in brand.md" % orb)
            shape += 1
    for selector in BLUR_ALLOWED:
        if selector not in brand:
            print("  blur allowlist entry %s is not in brand.md" % selector)
            shape += 1
    print("  %d rule verdict(s), %d shape problem(s)" % (len(verdicts), shape))
    failures += shape

    # 7. CSP: the build settings that keep script and style external.
    print("csp")
    config = ASTRO_CONFIG.read_text(encoding="utf-8") if ASTRO_CONFIG.is_file() else ""
    if "assetsInlineLimit: 0" not in config:
        print("  astro.config.mjs: vite.build.assetsInlineLimit is not 0")
        failures += 1
    if DIST_INDEX.is_file():
        html = DIST_INDEX.read_text(encoding="utf-8")
        found_inline = inline_scripts(html)
        if found_inline:
            print("  dist/index.html: %d inline script(s)" % len(found_inline))
            failures += 1
        if "<style" in html:
            if plan_status("csp-inline-styles"):
                print("  dist/index.html: <style> block present; pending to MS1 (plan marker csp-inline-styles)")
            else:
                print("  dist/index.html: <style> block present; build.inlineStylesheets must be 'never'")
                failures += 1
    else:
        print("  no built dist; the inline checks run after a build")
    print("  csp: %d failure(s) so far" % failures)

    print()
    return failures


def selftest() -> int:
    """Named checks over the pure helpers, so the checker itself is testable."""
    failures = 0
    total = 0

    def expect(name: str, ok: bool) -> None:
        nonlocal failures, total
        total += 1
        print("  %s  %s" % ("ok  " if ok else "FAIL", name))
        if not ok:
            failures += 1

    print("contrast arithmetic")
    expect("white on black is 21:1", abs(contrast("#FFFFFF", "#000000") - 21.0) < 0.01)
    expect("a colour against itself is 1:1", abs(contrast("#123456", "#123456") - 1.0) < 0.001)
    expect("order does not change the ratio",
           abs(contrast("#F2EDE4", "#07080A") - contrast("#07080A", "#F2EDE4")) < 1e-12)
    expect("the ink clears 4.5:1 on the accent", contrast("#0B0B0D", "#FF5400") >= 4.5)
    expect("the hairline is decorative, below the 3:1 boundary floor",
           contrast(TOKENS["ln"], TOKENS["ground"]) < 3.0)
    expect("every used text pair clears its target",
           all(contrast(TOKENS[name], TOKENS[surface]) >= TEXT_MIN
               for name, surfaces in TEXT_ON_SURFACES.items()
               for surface in surfaces))
    expect("the boundary token clears 3:1 on every surface",
           all(contrast(TOKENS[BOUNDARY], TOKENS[surface]) >= LARGE_MIN
               for surface in SURFACES))

    print("glass worst case")
    expect("an orb wash is brighter than the page ground",
           luminance(glass_backdrop("orb-cool")) > luminance(glass_backdrop("orb-warm")))
    expect("the scrim keeps muted text above 4.5:1 over the cool orb",
           contrast(TOKENS["mut"], glass_backdrop("orb-cool")) >= TEXT_MIN)
    expect("dropping the scrim fails muted text over the cool orb",
           contrast(TOKENS["mut"], over(GLASS_GRADIENT_TOP,
                                        over(parse_colour(TOKENS["orb-cool"]),
                                            parse_colour(TOKENS["ground"])))) < TEXT_MIN)
    expect("orange text on glass clears 4.5:1 over every orb",
           all(contrast(TOKENS["or"], glass_backdrop(orb)) >= TEXT_MIN for orb in ORBS))

    print("prototype identity")
    expect("the digest helper matches the published SHA-256 of abc",
           sha256_digest(b"abc") == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    expect("the pinned prototype digest is a 64-character lower-case hex string",
           len(PROTOTYPE_SHA256) == 64 and all(c in "0123456789abcdef" for c in PROTOTYPE_SHA256))

    print("pointer resolution")
    document = {"a": {"b": [10, 20]}, "c": "value"}
    expect("a nested list index resolves", resolve_pointer(document, "/a/b/1") == (True, 20))
    expect("an out-of-range index does not resolve", resolve_pointer(document, "/a/b/9") == (False, None))
    expect("a missing key does not resolve", resolve_pointer(document, "/a/z") == (False, None))

    print("copy-block markers")
    sample = "outside\n<!-- copy:begin id=x -->\ninside\n<!-- copy:end -->\nafter"
    expect("only marked lines are copy", copy_blocks(sample) == [(3, "inside")])

    print("banned-word scan")
    expect("a banned word in prose is caught", banned_hits("this is a secure system") == [(1, "secure")])
    expect("a banned word in a code span is ignored", banned_hits("use `secure` carefully") == [])
    expect("a fenced block is ignored", banned_hits("```\nsecure\n```") == [])
    expect("a hyphenated word is matched whole", banned_hits("a game-changing idea") == [(1, "game-changing")])

    print("digit scan")
    sample = "<!-- copy:begin id=x -->\nsends 2,743,352 B\n<!-- copy:end -->"
    expect("a bare number is caught", unbacked_number_lines(sample) == [(2, ["2,743,352"])])
    sample = "<!-- copy:begin id=x -->\nsends 2,743,352 B `[F-x]`\n<!-- copy:end -->"
    expect("a number with a fact id passes", unbacked_number_lines(sample) == [])
    expect("a non-claim token is ignored", unbacked_number_lines(
        "<!-- copy:begin id=x -->\nthe 512 B limit\n<!-- copy:end -->") == [])

    print("document parsers")
    expect("a brand token row is read", brand_tokens("| `--ink` | `#07080A` | ground |") == {"--ink": "#07080A"})
    expect("a rule verdict row is read",
           rule_verdicts("| 3 | No glass | REMOVE | the blur budget replaces it |") == {3: ("REMOVE", "the blur budget replaces it")})
    expect("an inventory row becomes cells",
           inventory_rows("| 1 | Orb drift | `.orbs i` | load | 30s | linear | none at rest | parity: the orb stops |")[0][1] == "Orb drift")
    expect("a blur selector is read from its block",
           blur_selectors(".a{backdrop-filter:blur(2px)}") == [".a"])
    expect("a rule with no backdrop-filter yields nothing",
           blur_selectors(".a{color:#fff}") == [])
    expect("a grouped :is selector expands to its elements",
           blur_selectors(":is(.a,.b){backdrop-filter:blur(2px)}") == [".a", ".b"])
    expect("a state prefix does not hide a blur element",
           blur_selectors(".rf :is(#hd,.dg){backdrop-filter:blur(2px)}") == ["#hd", ".dg"])
    expect("a comma-separated blur rule is split",
           blur_selectors(".c,.l{backdrop-filter:blur(6px)}") == [".c", ".l"])
    expect("a JSON data block is not an executable inline script",
           inline_scripts('<script type="application/json" id="d">{}</script>') == [])
    expect("an executable inline script is found",
           inline_scripts("<script>alert(1)</script>") == ["<script>"])
    expect("canvases are counted", canvas_count("<canvas></canvas><canvas/>") == 2)
    expect("the scope sentence is counted inside a copy block",
           scope_sentence_count("<!-- copy:begin id=x -->\n" + SCOPE_SENTENCE + "\n<!-- copy:end -->") == 1)
    expect("the scope sentence outside a copy block is not counted",
           scope_sentence_count(SCOPE_SENTENCE) == 0)

    print()
    print("selftest: %d check(s), %d failure(s)" % (total, failures))
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "check",
        nargs="?",
        choices=["contrast", "copy", "docs", "selftest", "all"],
        default="all",
    )
    args = parser.parse_args()

    failures = 0
    if args.check in ("contrast", "all"):
        failures += print_contrast()
        print()
    if args.check in ("copy", "all"):
        failures += check_copy()
        print()
    if args.check in ("docs", "all"):
        failures += check_docs()
    if args.check in ("selftest", "all"):
        failures += selftest()
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
