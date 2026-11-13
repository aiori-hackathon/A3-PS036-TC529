#!/usr/bin/env python3
"""Site token and copy checks.

Two independent checks, one file, because both guard the same site build:

  contrast   the WCAG 2.x contrast ratio of every text, boundary and fill pair
             in the site token set, printed as a table. The palette below
             mirrors the site token table in ``site/src/lib/tokens.ts``; the
             table in ``docs/site/brand.md`` is this command's output, so the
             document and the checker cannot drift from each other.
  copy       scans ``docs/site/copy.md`` for the banned words, for numbers that
             are not backed by a fact id, and for fact ids that do not resolve
             in the committed results artifact.
  selftest   named checks over the pure helpers, so the checker is itself
             testable without a fixture file.

Run ``python3 scripts/site-check.py`` with no argument to run all three. Exits
non-zero when any check fails, so it can be wired into a review or a CI step.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
COPY_PATH = ROOT / "docs" / "site" / "copy.md"
RESULTS_PATH = ROOT / "docs" / "sample-output" / "climb-results.json"

# --------------------------------------------------------------------------
# Site tokens (palette v2). The site is dark-only. This table mirrors
# ``site/src/lib/tokens.ts``; a test on the site side keeps the two in step.
# --------------------------------------------------------------------------

PALETTE = {
    "ink": "#07080A",
    "surface": "#0F1216",
    "raised": "#161A20",
    "paper": "#F2EDE4",
    "text": "#EFEAE1",
    "muted": "#9AA3AD",
    "orange": "#FF5400",
    "red": "#FF1A1A",
    "teal": "#5CC8C2",
    "slate": "#3A4A66",
    "slate-light": "#6B7A93",
    "measured": "#3DDC97",
    "derived": "#6EA8FF",
    "analytical": "#B79CFF",
    "extrapolated": "#F5D547",
}

SURFACES = ("ink", "surface", "raised")

# Foreground -> the surfaces it is painted on. A pair that is not listed is
# not used by the built site and must not be introduced without a target.
TEXT_ON_SURFACES = {
    "text": ("ink", "surface", "raised"),
    "paper": ("ink",),
    "muted": ("ink", "surface", "raised"),
    "orange": ("ink", "surface", "raised"),
    "teal": ("ink", "surface"),
    "red": ("ink",),
    "measured": ("ink", "surface", "raised"),
    "derived": ("ink", "surface", "raised"),
    "analytical": ("ink", "surface", "raised"),
    "extrapolated": ("ink", "surface", "raised"),
}

# Fills carry the ink as their label; light text on any of them fails.
FILLS = ("orange", "teal", "measured", "derived", "analytical", "extrapolated")

# The read-only boundary token. It clears the 3:1 non-text floor; `slate` does
# not and is decorative only.
BOUNDARY = "slate-light"

TEXT_MIN = 4.5  # WCAG 1.4.3 normal text
LARGE_MIN = 3.0  # WCAG 1.4.3 large text and 1.4.11 non-text boundary


def _channel(value: int) -> float:
    value = value / 255.0
    return value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4


def _rgb(hex_colour: str) -> tuple[int, int, int]:
    hex_colour = hex_colour.lstrip("#")
    return tuple(int(hex_colour[i : i + 2], 16) for i in (0, 2, 4))  # type: ignore[return-value]


def luminance(hex_colour: str) -> float:
    red, green, blue = _rgb(hex_colour)
    return 0.2126 * _channel(red) + 0.7152 * _channel(green) + 0.0722 * _channel(blue)


def contrast(foreground: str, background: str) -> float:
    first, second = luminance(foreground), luminance(background)
    high, low = max(first, second), min(first, second)
    return (high + 0.05) / (low + 0.05)


def verdict(ratio: float, minimum: float) -> str:
    return "pass" if ratio >= minimum else "FAIL"


def print_contrast() -> int:
    """Print every used token pair and the ratio it measures. Returns failures."""
    failures = 0
    pairs = 0
    print("WCAG 2.x contrast, site tokens (rounded to two decimals)")
    print(f"thresholds: {TEXT_MIN}:1 normal text, {LARGE_MIN}:1 large text and UI boundaries")
    print()

    print("text and status ink on each surface")
    print("%-16s" % "foreground" + "".join("%9s" % name for name in SURFACES) + "   target")
    for name, used_surfaces in TEXT_ON_SURFACES.items():
        row = "%-16s" % name
        for surface in SURFACES:
            if surface in used_surfaces:
                ratio = contrast(PALETTE[name], PALETTE[surface])
                row += "%9.2f" % ratio
                pairs += 1
                if ratio < TEXT_MIN:
                    failures += 1
            else:
                row += "%9s" % "-"
        row += "     %.1f" % TEXT_MIN
        print(row)

    print()
    print("ink on an accent or status fill")
    for name in FILLS:
        ink = contrast(PALETTE["ink"], PALETTE[name])
        light = contrast(PALETTE["paper"], PALETTE[name])
        ink_ok = verdict(ink, TEXT_MIN)
        light_ok = verdict(light, TEXT_MIN)
        print("%-16s ink #07080A %9.2f  %s   paper #F2EDE4 %9.2f  %s"
              % (name, ink, ink_ok, light, light_ok))
        pairs += 1
        if ink_ok == "FAIL":
            failures += 1
        if light_ok == "FAIL":
            # Expected: the rule is that light text is never painted on a fill.
            print("                 rule: light text is never painted on a fill; use the ink.")

    print()
    print("interactive boundary (slate-light) against each surface")
    for surface in SURFACES:
        ratio = contrast(PALETTE[BOUNDARY], PALETTE[surface])
        mark = verdict(ratio, LARGE_MIN)
        print("%-16s on %-8s %9.2f  %s" % (BOUNDARY, surface, ratio, mark))
        pairs += 1
        if mark == "FAIL":
            failures += 1

    print()
    print("decorative slate, never used as text or a boundary")
    for surface in SURFACES:
        ratio = contrast(PALETTE["slate"], PALETTE[surface])
        print("%-16s on %-8s %9.2f  %s (decorative only)"
              % ("slate", surface, ratio, verdict(ratio, LARGE_MIN)))
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
]

# A value's chip is one of four evidence classes, or one of two provenance tags
# for a value that is not a measurement result: a fixed model input (`Constant`)
# or a count a tool emits (`Generated`).
EVIDENCE_CHIPS = [
    "Measured", "Derived", "Analytical", "Extrapolated",  # evidence classes
    "Constant", "Generated",  # provenance tags, text-only
]

# Fact id -> JSON pointer in the committed results artifact, or None when the
# value is not in that artifact (with the reason recorded here).
FACT_IDS: dict[str, tuple[str | None, str]] = {
    "F-wire-medium-a": ("/wire_size/1/plain_total_bytes", "MEASURED"),
    "F-wire-medium-b": ("/wire_size/1/without_mtl_total_bytes", "MEASURED"),
    "F-wire-medium-c": ("/wire_size/1/with_mtl_total_bytes", "MEASURED"),
    "F-wire-medium-c-vs-a": ("/comparisons/2/wire_reduction_c_vs_a_percent", "DERIVED"),
    "F-wire-medium-c-vs-b": ("/comparisons/2/wire_reduction_c_vs_b_percent", "DERIVED"),
    "F-sign-medium-c-vs-a": ("/comparisons/2/signing_speedup_c_vs_a", "DERIVED"),
    "F-verify-medium-c-vs-a": ("/comparisons/2/verifying_speedup_cached_vs_a", "DERIVED"),
    "F-sign-batch-size": ("/wire_size/1/message_count", "MEASURED"),
    "F-udp-c-fit": ("/derived/udp_fit/aggregate/2/fit_rows", "ANALYTICAL"),
    "F-udp-c-total": ("/derived/udp_fit/aggregate/2/total_rows", "ANALYTICAL"),
    "F-dns-www-a": ("/dns_pipeline/reduction_percent_www_a", "DERIVED"),
    "F-mem-small-ratio": ("/memory/measured/0/measured_analytical_total_ratio", "DERIVED"),
    "F-wire-sizes": ("/wire_size", "MEASURED"),
    "F-parameter-set": ("/dns_pipeline/algorithm", "MEASURED"),
    # Not in the results artifact; sources recorded so a missing fact is visible.
    "F-udp-payload": (None, "CONSTANT: UDP_MTU_1232"),
    "F-claims-count": (None, "GENERATED: `cargo xtask readme-badges`"),
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

    print()
    print("copy check: %d failure(s)" % failures)
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
           abs(contrast("#EFEAE1", "#07080A") - contrast("#07080A", "#EFEAE1")) < 1e-12)
    expect("the ink clears 4.5:1 on the accent", contrast("#07080A", "#FF5400") >= 4.5)
    expect("slate is decorative, below the 3:1 boundary floor",
           contrast("#3A4A66", "#07080A") < 3.0)
    expect("every used text pair clears its target",
           all(contrast(PALETTE[name], PALETTE[surface]) >= TEXT_MIN
               for name, surfaces in TEXT_ON_SURFACES.items()
               for surface in surfaces))
    expect("the boundary token clears 3:1 on every surface",
           all(contrast(PALETTE[BOUNDARY], PALETTE[surface]) >= LARGE_MIN
               for surface in SURFACES))

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

    print("digit scan")
    sample = "<!-- copy:begin id=x -->\nsends 2,743,352 B\n<!-- copy:end -->"
    expect("a bare number is caught", unbacked_number_lines(sample) == [(2, ["2,743,352"])])
    sample = "<!-- copy:begin id=x -->\nsends 2,743,352 B `[F-x]`\n<!-- copy:end -->"
    expect("a number with a fact id passes", unbacked_number_lines(sample) == [])
    expect("a non-claim token is ignored", unbacked_number_lines(
        "<!-- copy:begin id=x -->\nthe 512 B limit\n<!-- copy:end -->") == [])

    print()
    print("selftest: %d check(s), %d failure(s)" % (total, failures))
    return failures



def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "check",
        nargs="?",
        choices=["contrast", "copy", "selftest", "all"],
        default="all",
    )
    args = parser.parse_args()

    failures = 0
    if args.check in ("contrast", "all"):
        failures += print_contrast()
        print()
    if args.check in ("copy", "all"):
        failures += check_copy()
    if args.check in ("selftest", "all"):
        failures += selftest()
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
