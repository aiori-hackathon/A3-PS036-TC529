# Prototype audit

Target: `docs/site/prototype/climb-site.html` (429 lines, 69,950 bytes, SHA-256
`0b0822338e004daba100bde1f98bcd9f2e5aa4c1f7b08198973d966a9570f108`).
All line numbers refer to that file. The file is an input, not a published page; nothing here
approves it for release.

**What this audit is.** A static read of the HTML, CSS and inline JavaScript, with computed colour
contrast and a hand trace of the keyboard path. **What it is not.** No browser was launched, no
screen reader was run, no device or GPU was tested, and no network request was observed in
development tools. Statements about what a browser *would* fetch or announce are marked as
inferences; everything else is read from the file.

## 1. External requests

| Line | Request | Kind | Verdict |
| --- | --- | --- | --- |
| 7 | `https://fonts.googleapis.com/css2?family=Bricolage+Grotesque…&family=Geist…&display=swap` | render-blocking stylesheet | must be removed; self-host |
| 6 | `data:image/svg+xml;base64,…` favicon | inline data URI | no request |
| 9 | `--logo:url("data:image/svg+xml;base64,…")` | inline data URI | no request |
| 226, 233, 243, 280, 284–286, 290, 293–295, 334 | `github.com`, `youtube.com`, `nfsu.ac.in`, `portal.aiori.in` | navigations behind links | no request until activated |
| 312 | `http://www.w3.org/2000/svg` | XML namespace string | not a network request |

The only third-party request the page makes on load is the Google Fonts stylesheet at line 7.
That stylesheet in turn requests font binaries from `fonts.gstatic.com` (inference from the
stylesheet URL; the binary URLs are not in the source). The stylesheet is also the only
render-blocking cross-origin resource, so removing it removes both the privacy leak and a
round trip on first paint.

Replace it with self-hosted files:

- The two families used are `Bricolage Grotesque` (`--fd`, display) and `Geist` (body, UI,
  diagrams). Both must be licensed for redistribution before they are copied into the repository;
  do not vendor them on the assumption that they are OFL.
- The repository already ships `assets/fonts/InstrumentSerif-Regular.ttf`,
  `assets/fonts/InstrumentSerif-Italic.ttf`, `assets/fonts/JetBrainsMono-Regular.ttf` and
  `assets/fonts/OFL.txt`. Those are not Bricolage or Geist, so they cannot substitute without a
  design decision.
- Self-hosting means a `@font-face` block with `font-display: swap`, `woff2` files, and the
  files committed with their licence text. The font files are a new asset class; flag the
  addition before adding them (see Open questions in the audit record).

## 2. Alt text and other text alternatives

There are **zero `<img>` elements** in the file (`grep -c '<img'` → `0`), so no `alt` attribute
is missing. The equivalent question is whether the graphics have text alternatives:

| Element | Line | Current treatment | Verdict |
| --- | --- | --- | --- |
| Header brand link (CSS background logo) | 224 | `<a class="brand logo" … aria-label="CLIMB home">`, empty content | named, but the visible wordmark is text inside a background image; the link name is only "CLIMB home" |
| Hero logo | 230 | `<i class="hl logo" role="img" aria-label="CLIMB, Cryptographic Ladder Impact Measurement Benchmark">` | named; `role="img"` on an `<i>` is valid ARIA but an inline `<svg role="img">` would be sturdier |
| 9 canvases | 223, 229, 230 (×2), 245, 251, 267, 274, 282 | `aria-hidden="true"` on the canvas or its wrapper | correctly hidden: they are decorative |
| How-it-works diagram | 259 | `<svg role="img" aria-label="Diagram of what each of three messages carries in the selected configuration">` | named, but the label does not convey the A/B/C difference; the adjacent text does |
| Architecture diagram | 269 | `<svg role="img" aria-label="Native libraries and DNS pipeline feed the Rust workspace…">` | named; diagram text is real SVG `<text>`, so it is exposed in browse mode |
| Ladder connector SVG | 249 | `<svg id="sv" aria-hidden="true">` | correctly hidden; the buttons on either side carry the information |
| Favicon, logo gradients | 6, 9 | data URIs | no alternative needed |

Gaps worth fixing in the site build:

- The header link's accessible name does not include the full project name, while the hero logo
  does. Make them consistent.
- The How-it-works SVG label is static and repeats in all three tabs; when the tab changes, the
  caption `#gt` changes (line 265) but the SVG's accessible name does not. Either bind the label
  to the selected configuration or hide the SVG and rely on the caption.
- The page index (`#rl`, line 227) is a visual scroll rail with generated links; the links are
  real anchors (line 377) but the rail is `pointer-events:none` (line 99) and only the links
  re-enable pointer events (line 101). Keyboard users can still tab to the links, but the rail
  has no accessible grouping label beyond the `aside` label "Page index".
- There is no skip link. The fixed header plus an eight-link nav makes the keyboard path to
  `main` longer than it needs to be.

## 3. Reduced motion

There are exactly **two** `@media (prefers-reduced-motion: reduce)` blocks:

- Line 118: disables smooth scrolling, `.fill` transitions, the rotating rings, diagram
  animations, and the reveal/scale transitions.
- Line 194: disables the three `.orbs i` drifting orbs.

The inline script also checks `matchMedia('(prefers-reduced-motion: reduce)')` once (`rm`, line
300) and guards most of the canvas work:

| Motion | Guard | Effect under reduced motion |
| --- | --- | --- |
| Hero ripple/orbit loop | 369–372 | one static frame, no pointer listener, no `requestAnimationFrame` loop |
| Count-up figures | 341–345 | figures keep their markup values |
| Ladder travelling dot | 329 | not created |
| Architecture packets (DOM) | 399 | not created |
| Section-band canvases | 419 | `sz()` draws one frame, then returns before the loop |
| Scroll particle field | 396 | `fl()` not started |
| Background burst | 391 | `burst()` returns early |
| Ladder path draw-in | 331 | `revLad()` still releases paths, no motion |

Remaining gaps:

- The How-it-works diagram contains SMIL `<animateMotion>` elements at line 264 with
  `repeatCount="indefinite"`. Neither the CSS block nor `rm` stops them, so the three white
  packets keep moving for a reduced-motion user. This is the clearest miss.
- The `.c`, `.l`, `.cd`, `.trk`, `.rt`, `.dt`, `#rl a` and `.arch path` transitions
  (lines 52, 55, 63, 67, 71, 80, 101, 109) are not disabled; these are short hover/reveal
  transitions rather than ambient motion, but a strict reading of reduced motion would remove
  the 5 px translate on hover (line 68).
- The `.orbs` are still painted (static) after line 194; acceptable, but they are large
  low-contrast washes that add nothing when static and could be hidden entirely.
- There is no `prefers-reduced-transparency` or `prefers-contrast` handling for the heavy
  backdrop-filter/glass layers (lines 141, 147, 151, 217, 219). Not required by WCAG, but the
  page leans on blur for legibility.

## 4. The nine canvases

Counting `<canvas` occurrences gives 8 lines but 9 elements: line 230 contains both `#ob` and
`#of`.

| # | Element | Line | What it draws | Owner code | Merge? |
| --- | --- | --- | --- | --- | --- |
| 1 | `#fx` | 223 | fixed full-viewport particle field; particles drift upward with scroll velocity and a burst on each `h2` | 388–396 | could merge with `#gc` into one ambient background canvas |
| 2 | `#gc` | 229 | hero grid with a ripple centred on the logo; pointer proximity distorts nearby cells | 346–360 | could merge with `#fx`; keep its hero mask |
| 3 | `#ob` | 230 | back half of the nanoparticle orbits, painted behind the logo | 346–366 | cannot merge with `#of` without losing the logo sandwich, unless the logo becomes a DOM layer between two draw passes on one canvas |
| 4 | `#of` | 230 | front half of the same orbits, painted in front of the logo | 346–366 | see above |
| 5 | `.bd canvas[data-m="0"]` | 245 | Merkle-tree band: hashes climb from leaves to root, nodes light up | 401–421 (`band(cv, 0)`) | one of three identical tree instances |
| 6 | `.bd canvas[data-m="1"]` | 251 | ladder band: packets hop three horizontal rungs | 401–421 (`band(cv, 1)`) | one of two identical ladder instances |
| 7 | `.bd canvas[data-m="0"]` | 267 | same tree band | same | duplicate |
| 8 | `.bd canvas[data-m="1"]` | 274 | same ladder band | same | duplicate |
| 9 | `.bd canvas[data-m="0"]` | 282 | same tree band | same | duplicate |

Merge conclusions:

- The five `.bd` canvases already share one function and differ only by `data-m`. They should be
  one component instantiated five times, or reduced to two instances (one tree, one ladder) if
  the design can live with fewer dividers. Nothing about them requires five separate canvases.
- `#ob` and `#of` are separate only to put the logo between the back and front halves. A single
  canvas with two draw passes and the logo as a positioned DOM element above it would preserve
  the effect with one context; the current split also doubles the resize and DPR bookkeeping
  (line 351).
- `#fx` and `#gc` are both ambient canvas effects with independent loops. Merging them reduces
  two `requestAnimationFrame` loops to one, but `#gc` is confined to the hero and masked
  (line 36), while `#fx` is fixed. Merge only if the hero mask can be applied per layer.
- All nine are correctly `aria-hidden`; none needs a text alternative.

## 5. Palette contrast (computed)

Method: sRGB relative luminance and the WCAG 2.x contrast formula, computed from the literal
colour values in the file. Ratios are rounded to two decimals. Thresholds: 4.5:1 for normal text,
3:1 for large text and UI component boundaries, 3:1 for focus indicators against adjacent colours.

| Pair | Foreground | Background | Ratio | Verdict |
| --- | --- | --- | --- | --- |
| Paper text on ink | `#F2EDE4` | `#07080A` | 17.18:1 | pass |
| Muted text on ink | `#A9A398` | `#07080A` | 7.99:1 | pass |
| Muted text on well | `#A9A398` | `#0D0F13` | 7.65:1 | pass |
| Muted text on raised panel | `#A9A398` | `#1B1E25` | 6.66:1 | pass |
| Orange label on ink | `#FF5400` | `#07080A` | 6.22:1 | pass |
| Orange label on well | `#FF5400` | `#0D0F13` | 5.96:1 | pass |
| Bright orange on ink | `#FE7F00` | `#07080A` | 7.87:1 | pass |
| Rose (derived badge, orb) on ink | `#FFA6A6` | `#07080A` | 10.74:1 | pass |
| Ink text on orange button | `#0B0B0D` | `#FF5400` | 6.11:1 | pass |
| Ink text on bright orange hover | `#0B0B0D` | `#FE7F00` | 7.73:1 | pass |
| Ink text on paper card | `#0B0B0D` | `#F2EDE4` | 16.86:1 | pass |
| Card body `#33333A` on paper | `#33333A` | `#F2EDE4` | 10.75:1 | pass |
| Card body on white hover | `#33333A` | `#FFFFFF` | 12.53:1 | pass |
| Navy on ink | `#06234B` | `#07080A` | 1.29:1 | fail as text; logo only |
| Maroon on ink | `#4D1111` | `#07080A` | 1.34:1 | fail as text; gradient stop only |
| Ink on maroon | `#0B0B0D` | `#4D1111` | 1.31:1 | fail; only the outer edge of the ladder root |
| Rose on maroon | `#FFA6A6` | `#4D1111` | 8.02:1 | pass |
| Paper on maroon | `#F2EDE4` | `#4D1111` | 12.83:1 | pass |
| Hairline border `--ln` on ink | `rgba(242,237,228,.13)` → `#262626` | `#07080A` | 1.32:1 | **fail 3:1 for interactive boundaries** |
| Hairline border `--ln` on well | same → `#2B2C2E` | `#0D0F13` | 1.37:1 | **fail 3:1** |
| Hairline border `--ln` on page ground | same → `#303235` | `#13161B` | 1.41:1 | **fail 3:1** |

The palette's text pairs are strong. The failures are structural, not accidental:

- `--navy` and `--mar` are used only as gradient stops (`.rt`, line 63) and inside the embedded
  logo (`foot-navy`, `m-base`). They must never become text or icon colours on ink.
- The ladder root `.rt` sets dark text over a radial gradient that ends in `--mar`. The label is
  centred over the orange middle (6.11:1 pass), but any growth that pushes text toward the
  maroon rim would fail at 1.31:1. Cap the text area or lighten the outer stop.
- The 13 % hairline (`--ln`) is the only boundary on the ladder capability buttons, branch
  buttons, tab underline, bar track and table rows. At 1.32–1.41:1 it does not meet the 3:1
  non-text contrast requirement. The focus outline (`--or2`, 7.87:1) is fine for keyboard users;
  the resting boundary is not.
- The second orb uses `rgba(24,96,200,.46)`, a blue that is not in the palette at all. It is
  visible as a cool wash behind the hero and does not map to any of the named tokens.

## 6. Keyboard access of the ladder explorer

What works:

- Capability rows are real `<button class="c">` elements created at line 314, each with
  `aria-pressed` set in `render()` (line 317). Branch rows are real `<button class="l">` elements
  (line 318) with their own `aria-pressed`. Native buttons give Tab, Enter and Space for free.
- The two groups are labelled `role="group" aria-label="Capabilities"` and
  `role="group" aria-label="Branches"` (line 249).
- The detail pane is `aria-live="polite"` (line 249), so a leaf change is announced.
- The copy control in the detail pane is a real `<button>` (line 319), reachable by Tab.
- Focus is visible: `:focus-visible` draws a 2 px `--or2` outline with 3 px offset (line 13).

What fails or is missing:

- **No arrow-key navigation.** The capability and branch groups are buttons in a group, not a
  composite widget, so the user must Tab through every capability (7 stops) and every branch
  (2–3 stops) to reach the detail pane. The How-it-works tabs do implement Arrow Left/Right with
  a roving `tabindex` (lines 338–339), so the pattern exists in the file but was not applied to
  the ladder.
- **No expanded state.** A capability button toggles its branches by replacing `#lv`, but it
  carries only `aria-pressed`, not `aria-expanded`/`aria-controls`. A screen-reader user is not
  told that activating it opened a branch list or which element changed.
- **The branch list is replaced without a focus target.** If a keyboard user activates a leaf
  and then changes capability, `render()` empties `#lv` (line 317) and rebuilds it; there is no
  focus move or announcement of the new list length.
- **The connector graph is hidden.** `#sv` is `aria-hidden="true"` (line 249), which is correct
  for the drawing, but the visible parent→branch relationship exists only in layout; the two
  groups are siblings with no `aria-controls` linking them.
- **The detail pane is announced, not focused.** That is acceptable for a live region, but long
  leaf text plus a command is a lot to hear with no way to jump to it.
- **Pointer-only affordances.** Hover sets a highlight class (`hv()`, line 316) and moves the
  card 5 px (line 68); the selected state is also exposed via `aria-pressed`, so this is
  redundant but not blocking.
- **No skip link** to `#ladder`, and the fixed header's menu button and eight links come first in
  tab order.

Recommended pattern: either keep the toggle-button model and add `aria-expanded` +
`aria-controls`, or make each column a `listbox`/`tablist`-style composite with one tab stop and
Arrow Up/Down, Home and End, mirroring the tab code at line 339. Do not add both patterns.

## 7. Security-claim phrases

The brief quotes a hero line, **"The next layer of trust."** That string does **not** occur in
this file. The hero is `Post-quantum DNSSEC, measured.` (line 231). The closest
security-adjacent phrases that do occur are:

| Line | Phrase | Why it is security-adjacent | Action |
| --- | --- | --- | --- |
| 290 | `Sign once. Prove many.` | "Prove" can be read as a proof of security rather than "demonstrate at scale". | replace |
| 240, 306 | `trusted cached ladder` | "Trusted" is a verifier assumption, not a property of MTL; unexplained, it reads as a trust guarantee. | reword to "pre-validated ladder (verifier trusts the cache)" |
| 279, 296 | `No formal security guarantee, proof or certification is claimed` | a disclaimer, not a claim; it must stay | keep verbatim |
| 275 | `checked against a claims registry` | "Claims" here means numeric claims in the report, not security claims; a reader can misread it | keep, but define once |
| 305–306 | `evidence class`, `Derived`, `Measured` | evidence vocabulary, not security | keep |

No line says "secure", "quantum-resistant", "unbreakable", "certified" or "guaranteed" as a
property of the scheme. There is no claim of a formal proof anywhere in the visible copy.

Two replacements for `Sign once. Prove many.` that claim nothing about security:

1. **`One signed ladder. Many short paths.`** — describes the mechanism and the wire effect.
2. **`Sign a batch. Measure the difference.`** — describes the experiment and its output.

Both avoid "prove", "trust", "secure" and "guarantee". If a shorter footer line is needed,
`One ladder, many signatures.` also works and makes no security claim.

## 8. Content duplicated from the README

The prototype was written from the README, and several passages are near-verbatim. Duplication
is a maintenance risk: a number or limit fixed in one place will drift in the other.

| Prototype line(s) | Prototype content | README source | Duplication | Recommendation |
| --- | --- | --- | --- | --- |
| 232 | Hero lede: "Measuring how Merkle Tree Ladder amortisation changes…" | `README.md:12` | near-verbatim | keep on the site; README keeps its own one-line description, do not try to keep them identical |
| 236–243 | Results intro, four headline figures, evidence legend | `README.md:94–120` | same facts, different layout | site reads `target/climb-results.json`; README table stays generated. Do not copy values by hand into either |
| 246, 305–311 | Seven capabilities and their commands | `README.md:71` (report link), `README.md:377–398` (repository map) | overlapping capability descriptions | keep the seven-card taxonomy only on the site; link the repository map instead of restating it |
| 252–258 | How it works A/B/C and wire bars | `docs/BASELINES.md` (linked at `README.md:294`), `README.md:102–105` | conceptual duplication | keep the explainer on the site; it is the one place a reader sees the mechanism |
| 268 | Architecture paragraph | `README.md:377–398` | summary of the repository map | keep one sentence; link `docs/ARCHITECTURE.md` for the full map |
| 275–278 | Transparency cards: datasets, pinned submodules, claims registry | `README.md:356–369`, `README.md:393`, `README.md:428` | same claims in prose | keep on the site as the trust section; the README remains the developer-facing source |
| 279 | Known experimental limits | `README.md:504–534` | heavily overlapping | **do not duplicate in full**; keep a 2–3 sentence summary and link the README's "Honest limits" anchor |
| 283–286 | Team names, roles, links | `README.md:60–62` | identical data | one source (prefer a generated team block) or accept the duplication and add a drift check |
| 292 | Footer description | `README.md:10–12` | paraphrase | keep, it is short |
| 296 | `771 of 771 claims verified` | `README.md:22` (`852/852`) | **stale duplicate** | remove the number from the site; render the generated count or link the badge |
| 290 | `Sign once. Prove many.` | `README.md:565` (`Measured, not claimed.`) | different taglines | site uses the replacement in §7; README tagline is fine as is |

The highest-risk duplicates are the claims count (already stale) and the limits paragraph. The
site should summarise limits and link, not restate them, because the README version is the one
`cargo xtask docs-check` protects.

## 9. Other observations for the site build

- `html{background:#13161b}` (line 123) differs from `--ink` (line 9); the page ground is a
  slate, not the ink token. Decide whether the site's `surface` token is this slate.
- The page relies on `backdrop-filter` in five declarations (ten properties including the
  `-webkit-` alias; lines 141, 147, 151, 217, 219). Where it is unsupported the glass layers
  fall back to translucent fills; contrast then depends on what is behind them. The audit could
  not test a non-supporting browser.
- The `#mn` mobile menu button sets `aria-expanded` (line 301), but the nav's open state is a
  CSS class; the audit did not verify the mobile layout or whether focus is trapped/restored.
- `line 334` inserts an eighth card (`All seven capabilities…`) into the same grid as the seven
  capability cards; it is a link, not a button, which is correct, but it reads as an eighth
  capability in a section that says "seven". Consider moving it out of the grid.
- The footer says `MIT licence` (line 296). The repository root has an `MIT License` file
  (`LICENSE`, Copyright 2026 CLIMB Team, A3-PS036-TC529), so the line matches the licence file.
  Whether the licence is the right one for the project is a decision for the maintainers, not an
  audit finding.
- The prototype is 69,950 bytes (68.3 KiB), not the "61 KB" in the audit brief. Treat the brief's
  size as approximate; the file on disk is the source of truth.

## What was not verified

- No browser, screen reader, mobile device, high-contrast mode or GPU rendering.
- No network trace: the `fonts.gstatic.com` follow-up request is inferred from the stylesheet
  URL, not observed.
- No automated accessibility audit (axe, Lighthouse) was run.
- Contrast was computed from literal CSS colours only; gradients and translucent layers were
  evaluated at their nominal stops, not sampled from a rendered screenshot.
- The font redistribution licences were not checked.
