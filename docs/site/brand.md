# Site brand

The design system for the CLIMB website: the dark-only palette, the type
system, and the asset set. This is a specification, not a build — the site
code comes later. Every colour decision here is measured, and the measurement
is printed by `python3 scripts/site-check.py contrast`, which holds the
machine-readable token set and is the single source the site build reads. The
tables below are that command's output.

The palette is derived from the team's logo (`docs/media/brand.toml`): the
orange and the teal are the logo's own inks, the rest is built to sit under
them. It replaces the earlier violet-to-cyan gradient idea outright; there is
no gradient text anywhere in this system.

## 1. The palette (dark-only)

The site has one mode: dark. There is no light theme, no theme switch and no
`prefers-color-scheme` branch. A light surface is not in scope.

### Surfaces

| Token | Value | Role |
| --- | --- | --- |
| `bg-0` | `#07090c` | The page ground. The darkest surface; everything else lifts from it. |
| `bg-1` | `#0b0d10` | A raised section, the sticky header, a full-bleed band. |
| `bg-2` | `#11141a` | A panel or card that holds content. |
| `bg-3` | `#171b22` | A well or inset: table tracks, code blocks, chart plots. |
| `line` | `rgba(255,255,255,.08)` | Decorative hairline only. Never a control boundary (see §3). |
| `line-strong` | `#646A74` | An interactive boundary: input edges, tab underlines, chart axes. Added because `line` fails the 3:1 non-text floor. |

### Ink

| Token | Value | Role |
| --- | --- | --- |
| `text` | `#E6EDF3` | Primary text and icons. |
| `muted` | `#8B98A5` | Secondary text, captions, axis labels, metadata. |
| `ink` | `#07090c` | The ink painted on an accent or status fill. It is `bg-0` reused as a foreground; the site never paints light text on a fill. |

### Brand

| Token | Value | Role |
| --- | --- | --- |
| `accent-orange` | `#FF5400` | The logo orange. Fills, rules, chart data, the one deliberate decorative element. |
| `accent-text` | `#FF8A4C` | A lightened orange for accent **text** on dark: links, eyebrows, the number in a KPI. Kept separate so small text never rides the raw logo orange. |
| `secondary-teal` | `#5CC8C2` | The logo teal. Secondary data series, the "second" of any pair, the one non-orange highlight. |

Two accent colours are visible on any one screen at most: orange and teal. The
four evidence colours in §4 are status colours, not accents, and do not count
against that limit.

### Evidence-class colours (status only)

| Token | Value | Class | Meaning |
| --- | --- | --- | --- |
| `measured` | `#6CCB8E` | Measured | Read from a measurement artifact. |
| `derived` | `#5CA9FF` | Derived | Computed from other recorded values (ratios, byte arithmetic). |
| `analytical` | `#C58AF9` | Analytical | From a closed-form model. |
| `extrapolated` | `#E0A93B` | Extrapolated | `N ×` a measured per-message cost, not a whole-batch measurement; rendered with `≈`. |

These four are the **only** status colours. There is no separate success,
warning or error hue; an empty state, a loading state and an error state are
carried by `text`/`muted`, the accent, an icon and a border, never by a fifth
colour. Colour is never the only channel: the class is also carried by a fill
pattern (solid, hatched, dotted) and a written label, so the four stay
distinguishable without colour vision.

Two values on the site are not evidence-class results: a fixed model input (the
`1,232 B` EDNS(0) payload) and a count a tool emits (the claims badge). They
carry the provenance tags `Constant` and `Generated`, rendered as plain `muted`
text with no colour and no pattern. A provenance tag is a label, not a status,
so the four-colour rule above still holds.

## 2. Contrast (computed)

Computed with the WCAG 2.x sRGB relative-luminance formula from the literal
token values. Thresholds: 4.5:1 for normal text, 3:1 for large text and UI
component boundaries.

```
WCAG 2.x contrast, site tokens (rounded to two decimals)
thresholds: 4.5:1 normal text, 3.0:1 large text and UI boundaries

text and status ink on each surface
foreground           bg-0     bg-1     bg-2     bg-3
text                16.87    16.47    15.61    14.61
muted                6.77     6.61     6.26     5.86
accent-orange        6.19     6.04     5.73     5.36
accent-text          8.54     8.33     7.90     7.39
secondary-teal       9.96     9.72     9.22     8.63
measured            10.03     9.79     9.28     8.69
derived              8.14     7.94     7.53     7.05
analytical           7.95     7.76     7.36     6.89
extrapolated         9.41     9.18     8.70     8.15
line-strong          3.66     3.57     3.39     3.17

decorative hairline rgba(255,255,255,.08) composited over each surface
line             #1B1D1F on bg-0        1.18  FAIL (decorative only)
line             #1F2023 on bg-1        1.19  FAIL (decorative only)
line             #24272C on bg-2        1.23  FAIL (decorative only)
line             #2A2D34 on bg-3        1.25  FAIL (decorative only)

ink on an accent or status fill
accent-orange    ink #07090c      6.19  pass   light text #E6EDF3      2.73  FAIL
                 rule: light text is never painted on a fill; use the ink.
secondary-teal   ink #07090c      9.96  pass   light text #E6EDF3      1.69  FAIL
                 rule: light text is never painted on a fill; use the ink.
extrapolated     ink #07090c      9.41  pass   light text #E6EDF3      1.79  FAIL
                 rule: light text is never painted on a fill; use the ink.

interactive boundary (line-strong) against each surface
line-strong      on bg-0        3.66  pass
line-strong      on bg-1        3.57  pass
line-strong      on bg-2        3.39  pass
line-strong      on bg-3        3.17  pass

failures that need a rule (not a token change): 0
```

Every text and status colour clears 4.5:1 on all four surfaces. No token was
tuned to reach that: the values as specified already pass.

## 3. What failed, and the rule that fixes it

Only two pairs fail, and both are fixed by a rule rather than by moving a
colour the brief fixes:

1. **Light text on an accent or status fill** (`#E6EDF3` on `#FF5400` is
   2.73:1; on the teal, 1.69:1; on the extrapolated amber, 1.79:1). Rule:
   text on a fill is always the ink `#07090c`, which clears 6.19:1 on the
   orange, 9.96:1 on the teal and 9.41:1 on the amber. A filled button, a
   badge, a selected chip and a chart bar label all follow this.
2. **The decorative hairline `line`** composites to 1.18–1.25:1, well below
   the 3:1 non-text floor. It is therefore **decorative only** — section
   rules, table row separators, chart gridlines that a reader does not need
   to see to operate a control. Any boundary a reader must perceive to use a
   control (an input edge, a focus ring, a tab underline, a checkbox, a chart
   axis) uses `line-strong` `#646A74`, which measures 3.17–3.66:1. This is
   the one token added beyond the brief, and it exists precisely because the
   brief's `line` value fails the threshold.

## 4. Typography

Three faces, which is the "at most two families plus a neutral body face"
shape: a display serif, a neutral body sans, and a mono for data.

| Role | Face | Weights | Why |
| --- | --- | --- | --- |
| Headlines and large numerals | **Instrument Serif** | 400 regular, 400 italic | Already the face of the commit graph; an editorial serif gives the few large statements (the hero, a KPI numeral, a section title) a voice a dashboard face cannot. |
| Body copy, UI, navigation | **Geist** | 400 regular | A neutral grotesque that stays quiet under a serif and reads at 16 px. It carries the bulk of the copy, so it is chosen for legibility, not character. |
| Data, code, tabular numbers | **Geist Mono** | 400 regular | The numbers on this site are artifact values. A mono face signals "this is a value, not prose", keeps columns aligned, and matches the repository's command-and-output vocabulary. |

**Rationale, in one paragraph.** The site is a record of measurements, so its
type has one job: make the few large claims unmistakable and the surrounding
evidence calm and readable. Instrument Serif does the first — it is already
the project's display face in the commit graph, so the site inherits a
recognisable voice rather than inventing one. Geist does the second: a neutral
grotesque is the right body face because body copy should not compete with the
numerals it explains. Geist Mono does the third: every number here has a fact
id behind it, and a monospaced digit run reads as data rather than as
advertising. Three faces, two families plus a neutral body face, no more.

### Subsets and budget

Self-hosted, latin-only `woff2`, `font-display: swap`, no third-party font
request at load time (the prototype's Google Fonts stylesheet is removed, not
proxied).

| File | Subset | Budget |
| --- | --- | --- |
| `instrument-serif-regular-latin.woff2` | Latin (basic + Latin-1 punctuation) | 24 KB |
| `instrument-serif-italic-latin.woff2` | Latin | 24 KB |
| `geist-regular-latin.woff2` | Latin | 30 KB |
| `geist-mono-regular-latin.woff2` | Latin | 34 KB |
| **Total** | | **112 KB planned, 120 KB cap** |

Eight kilobytes of headroom are held for a licence-mandated glyph or a subset
correction. No second weight of Geist or Geist Mono is shipped; emphasis uses
weight 400 plus a size, colour or rule, not a second file.

### Licences

All three faces are under the SIL Open Font License 1.1 and may be
redistributed with their licence text:

- Instrument Serif — `assets/fonts/OFL.txt` (already in the repository).
- Geist and Geist Mono — `assets/fonts/OFL-Geist.txt` (added with this
  specification).

Instrument Serif ships as a `.ttf` in `assets/fonts/`; Geist and Geist Mono are
fetched from their upstream release and subset at build time, so their `.ttf`
sources are not committed here. The `woff2` subsets and their build command are
added when the site is built, not in this specification. The repository's
existing `assets/fonts/JetBrainsMono-Regular.ttf` stays: the commit graph
embeds it as outlines, and this specification does not touch the commit graph.

### Fallbacks

`font-family` stacks name the real face, then a metric-similar system face,
then the generic family, so a missing file degrades to a readable page rather
than to invisible text:

- display: `"Instrument Serif", "Iowan Old Style", "Times New Roman", serif`
- body: `"Geist", system-ui, -apple-system, "Segoe UI", Roboto, sans-serif`
- mono: `"Geist Mono", ui-monospace, "SF Mono", "Cascadia Mono", monospace`

## 5. Assets

Everything in `docs/media/logo/` is a build output of
`scripts/logo/build_logo_assets.py`; the site consumes the files, it does not
redraw them. The set the site uses:

| Asset | Source file | Use on the site |
| --- | --- | --- |
| Dark wordmark (vector) | `climb-logo-dark.svg` | Header, hero, footer. The only logo on a dark surface. |
| Light wordmark (vector) | `climb-logo-light.svg` | Not used by the dark-only site; kept for the README and any light artifact. |
| Raster wordmarks | `climb-logo-dark@1x/2x/3x.png` | A raster fallback only if a surface cannot take SVG. |
| `favicon.svg` | `favicon.svg` | Primary favicon; follows the viewer's colour scheme for the mark's strokes. |
| `favicon.ico` | `favicon.ico` | Legacy browsers: the 16, 32 and 48 px renditions. |
| `favicon-192.png`, `favicon-512.png` | same | Web-app manifest icons. |
| `apple-touch-icon.png` | same | iOS home-screen icon. |
| Open Graph card | `og-image.png` | Social preview for link unfurls; see the spec below. |
| Social preview | `social-preview.png` | The repository's own preview card; the site links to it, it does not render it. |

### Open Graph image — specification

The existing `og-image.png` is the mark centred on the ink with no fact. The
site build produces a replacement at the same 1200 × 630:

- Background `bg-0` `#07090c`.
- The dark wordmark, top-left, with the standard clear space.
- **One** headline fact, set in Instrument Serif at display size, in `text`
  `#E6EDF3`, with its evidence chip in `muted` beneath it. The fact is a fact
  id slot, not a literal: the suggested one is the Medium wire reduction,
  `F-wire-medium-c-vs-a`, because it is the single most legible result. The
  build reads it from the committed artifact and never types the digits.
- The wordmark and the fact are the only two elements. No gradient, no glow,
  no second fact, no logo lock-up beyond the wordmark.
- Generated by the same asset script as the rest of the logo set, so the OG
  card cannot drift from the palette or from the artifact.

## 6. Rules for the site build

1. A colour used as text, an icon or a boundary comes from this document and
   nowhere else. No raw channel triples in component code; the token set is
   imported once.
2. Text on a fill is always the ink `#07090c`. Light text never sits on the
   orange, the teal or any status colour.
3. `line` is decorative; `line-strong` is for anything a reader must perceive
   to operate. A focus ring uses `accent-text` (8.54:1 on `bg-0`).
4. The four evidence colours are the only status colours and always ship with
   a pattern and a written label beside them.
5. Two accents per screen, maximum: orange and teal.
6. Every token pair used in the built site is asserted against the numbers in
   §2 by a test that reads the same token source this file is generated from.
