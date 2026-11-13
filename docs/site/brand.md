# Site brand

The design system for the CLIMB website: one dark palette, three type roles,
and the asset set. `site/src/lib/tokens.ts` is the machine-readable token
table; `site/src/styles/tokens.css` carries the same values as CSS custom
properties, and a Vitest check compares the two so they cannot drift.
`python3 scripts/site-check.py contrast` prints the table in §2 from a mirror
of those values, so this document and the checker cannot drift from each
other; the Vitest check anchors both to the stylesheet the site ships.

The palette is derived from the team's logo (`docs/media/brand.toml`): the
orange and the teal are the logo's own inks, the rest is built to sit under
them. It replaces the earlier violet-to-cyan gradient idea outright; there is
no gradient text anywhere in this system.

This is palette v2. It replaces the earlier four-surface scale (`bg-0` through
`bg-3`) and its two support tokens with three surfaces and a smaller set of
names. The earlier lightened accent (`accent-text`) is gone because the logo
orange clears 4.5:1 as text on all three surfaces; the earlier composited
hairline is gone because `slate` is explicitly decorative and `slate-light` is
the boundary token.

## 1. The palette (dark-only)

The site has one mode: dark. There is no light theme, no theme switch and no
`prefers-color-scheme` branch. A light surface is not in scope.

### Surfaces

| Token | Value | Role |
| --- | --- | --- |
| `ink` | `#07080A` | The page ground. The darkest surface; everything else lifts from it. |
| `surface` | `#0F1216` | The header, the footer, and a full-bleed band. |
| `raised` | `#161A20` | A panel or card that holds content. |
| `slate` | `#3A4A66` | Decorative rule only: section rules and table row separators. It measures 2.24:1 on the ink, below the 3:1 non-text floor, so it never becomes a control boundary. |
| `slate-light` | `#6B7A93` | An interactive boundary: input edges, tab underlines, chart axes, the outline of the secondary button. It clears 3:1 on all three surfaces. |

### Ink

| Token | Value | Role |
| --- | --- | --- |
| `text` | `#EFEAE1` | Primary text and icons. |
| `muted` | `#9AA3AD` | Secondary text, captions, axis labels, metadata. |
| `paper` | `#F2EDE4` | Display headlines and the wordmark's warm cream. It is a text colour, never a surface; orange text on it measures 2.76:1. |
| `ink` | `#07080A` | The ink painted on an accent or status fill. It is the page ground reused as a foreground; the site never paints light text on a fill. |

### Brand

| Token | Value | Role |
| --- | --- | --- |
| `orange` | `#FF5400` | The logo orange. Fills, rules, chart data, eyebrows, inline links, and the one deliberate decorative element. |
| `red` | `#FF1A1A` | An alert accent for text on the page ground only; 5.17:1 on `ink`. |
| `teal` | `#5CC8C2` | The logo teal. Secondary data series, the "second" of any pair, the focus ring, and the one non-orange highlight. |

Two accent colours are visible on any one screen at most: orange and teal. The
four evidence colours in §1 are status colours, not accents, and do not count
against that limit.

### Evidence-class colours (status only)

| Token | Value | Class | Meaning |
| --- | --- | --- | --- |
| `measured` | `#3DDC97` | Measured | Read from a measurement artifact. |
| `derived` | `#6EA8FF` | Derived | Computed from other recorded values (ratios, byte arithmetic). |
| `analytical` | `#B79CFF` | Analytical | From a closed-form model. |
| `extrapolated` | `#F5D547` | Extrapolated | `N ×` a measured per-message cost, not a whole-batch measurement; rendered with `≈`. |

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
component boundaries. This is the output of `python3 scripts/site-check.py
contrast`:

```
WCAG 2.x contrast, site tokens (rounded to two decimals)
thresholds: 4.5:1 normal text, 3.0:1 large text and UI boundaries

text and status ink on each surface
foreground            ink  surface   raised   target
text                16.72    15.67    14.57     4.5
paper               17.18        -        -     4.5
muted                7.84     7.35     6.83     4.5
orange               6.22     5.83     5.42     4.5
teal                10.01     9.38        -     4.5
red                  5.17        -        -     4.5
measured            11.34    10.62     9.88     4.5
derived              8.31     7.79     7.24     4.5
analytical           8.79     8.24     7.66     4.5
extrapolated        13.83    12.96    12.05     4.5

ink on an accent or status fill
orange           ink #07080A      6.22  pass   paper #F2EDE4      2.76  FAIL
                 rule: light text is never painted on a fill; use the ink.
teal             ink #07080A     10.01  pass   paper #F2EDE4      1.72  FAIL
                 rule: light text is never painted on a fill; use the ink.
measured         ink #07080A     11.34  pass   paper #F2EDE4      1.52  FAIL
                 rule: light text is never painted on a fill; use the ink.
derived          ink #07080A      8.31  pass   paper #F2EDE4      2.07  FAIL
                 rule: light text is never painted on a fill; use the ink.
analytical       ink #07080A      8.79  pass   paper #F2EDE4      1.95  FAIL
                 rule: light text is never painted on a fill; use the ink.
extrapolated     ink #07080A     13.83  pass   paper #F2EDE4      1.24  FAIL
                 rule: light text is never painted on a fill; use the ink.

interactive boundary (slate-light) against each surface
slate-light      on ink           4.61  pass
slate-light      on surface       4.32  pass
slate-light      on raised        4.02  pass

decorative slate, never used as text or a boundary
slate            on ink           2.24  FAIL (decorative only)
slate            on surface       2.10  FAIL (decorative only)
slate            on raised        1.95  FAIL (decorative only)

used pairs: 34, failures: 0
```

Every text and status colour clears 4.5:1 on every surface it is painted on.
No token was tuned to reach that: the values as specified already pass.

## 3. What failed, and the rules that fix it

No used pair fails, so no brief value was changed. Four rules keep it that
way:

1. **Light text on an accent or status fill fails** (`#F2EDE4` on `#FF5400` is
   2.76:1; on the teal, 1.72:1; on the extrapolated yellow, 1.24:1). Rule:
   text on a fill is always the ink `#07080A`, which clears 6.22:1 on the
   orange, 10.01:1 on the teal and 13.83:1 on the extrapolated yellow. A
   filled button, a badge, a selected chip and a chart bar label all follow
   this.
2. **The decorative `slate`** measures 1.95–2.24:1, well below the 3:1
   non-text floor. It is therefore **decorative only** — section rules, table
   row separators, chart gridlines that a reader does not need to see to
   operate a control. Any boundary a reader must perceive to use a control (an
   input edge, a focus ring, a tab underline, a checkbox, a chart axis) uses
   `slate-light` `#6B7A93`, which measures 4.02–4.61:1.
3. **`red` stays on the page ground.** It measures 5.17:1 on `ink`, 4.84:1 on
   `surface` and 4.502:1 on `raised`; the last is close enough to the floor
   that the token is reserved for alert text on `ink` rather than being
   painted on every surface.
4. **`paper` is a text colour, not a surface.** Orange on `paper` is 2.76:1,
   so the cream never becomes a background under an accent. It appears as
   display text on `ink` and as the wordmark's fill.

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
proxied). The Geist variable sources are instanced at `wght=400` before
subsetting, so a static 400 face ships and no unused weight is downloaded.

| File | Subset | Bytes | Budget |
| --- | --- | ---: | ---: |
| `instrument-serif-regular-latin.woff2` | Latin (basic + Latin-1 punctuation) | 20,204 | 24 KB |
| `instrument-serif-italic-latin.woff2` | Latin | 21,312 | 24 KB |
| `geist-regular-latin.woff2` | Latin | 10,784 | 30 KB |
| `geist-mono-regular-latin.woff2` | Latin | 8,392 | 34 KB |
| **Total** | | **60,692 (59.3 KiB)** | **120 KiB cap** |

The build leaves 62,188 bytes of headroom under the 122,880-byte cap. No
second weight of Geist or Geist Mono is shipped; emphasis uses weight 400 plus
a size, colour or rule, not a second file.

Only the two hero faces are preloaded — Instrument Serif regular and Geist
regular. Instrument Serif italic (the footer tagline) and Geist Mono (data)
load on use, so they do not compete with the first paint.

### Fallbacks

Each stack names the real face, then a size-adjusted fallback built from a
metric-compatible system face, then a generic family. The `size-adjust` and
vertical overrides keep the line box stable when the webfont swaps in:

- display: `"Instrument Serif", "Instrument Serif Fallback", "Iowan Old Style", "Times New Roman", serif`
- body: `"Geist", "Geist Fallback", system-ui, -apple-system, "Segoe UI", Roboto, sans-serif`
- mono: `"Geist Mono", "Geist Mono Fallback", ui-monospace, "SF Mono", "Cascadia Mono", monospace`

The fallback faces use `size-adjust: 111.12%` for Instrument Serif (against
Tinos/Times New Roman) and `100.32%` for Geist and Geist Mono (against
Arimo/Arial and Cousine/Courier New), with ascent/descent overrides taken from
each webfont's own hhea metrics: 99% / 31% for Instrument Serif and
100.5% / 29.5% for Geist. Every face declares `font-display: swap`.

### Licences

All three faces are under the SIL Open Font License 1.1 and are redistributed
with their licence text:

- Instrument Serif — `site/public/fonts/licences/instrument-serif-OFL.txt`
  (source: `assets/fonts/OFL.txt`).
- Geist and Geist Mono — `site/public/fonts/licences/geist-OFL.txt` (source:
  `assets/fonts/OFL-Geist.txt`).

Instrument Serif ships as a `.ttf` in `assets/fonts/`; Geist and Geist Mono
are fetched from their upstream release and subset at build time, so their
`.ttf` sources are not committed. `site/scripts/build-fonts.py` reproduces the
four `woff2` files and records the upstream release and checksums. The
repository's existing `assets/fonts/JetBrainsMono-Regular.ttf` stays: the
commit graph embeds it as outlines, and this specification does not touch the
commit graph.

## 5. Assets

Everything in `docs/media/logo/` is a build output of
`scripts/logo/build_logo_assets.py`; the site consumes copies under
`site/public/`, it does not redraw them. The set the site uses:

| Asset | Source file | Use on the site |
| --- | --- | --- |
| Dark wordmark (vector) | `climb-logo-dark.svg` | `site/public/logo/`; header and footer. The only logo on a dark surface. |
| Light wordmark (vector) | `climb-logo-light.svg` | Not used by the dark-only site; kept for the README and any light artifact. |
| Raster wordmarks | `climb-logo-dark@1x/2x/3x.png` | A raster fallback only if a surface cannot take SVG. |
| `favicon.svg` | `favicon.svg` | `site/public/`; primary favicon. |
| `favicon.ico` | `favicon.ico` | `site/public/`; legacy browsers: the 16, 32 and 48 px renditions. |
| `favicon-192.png`, `favicon-512.png` | same | `site/public/`; web-app manifest icons. |
| `apple-touch-icon.png` | same | `site/public/`; iOS home-screen icon. |
| Open Graph card | `og-image.png` | `site/public/`; social preview at 1200 × 630. |
| Social preview | `social-preview.png` | The repository's own preview card; the site links to it, it does not render it. |

The layout emits `og:image`, `og:image:width`, `og:image:height`,
`og:image:alt`, the Twitter card pair, the canonical URL and a dark
`theme-color` of `#07080A`. The image is the L1 card unchanged: the wordmark
on the ink ground with no fact. If a later phase regenerates it, the same
palette rules apply — ink `#07080A` ground, `paper` `#F2EDE4` display text,
one fact read from the committed artifact and never typed as a literal.

## 6. Rules for the site build

1. A colour used as text, an icon or a boundary comes from this document and
   nowhere else. No raw channel triples in component code; the token set is
   imported once.
2. Text on a fill is always the ink `#07080A`. Light text never sits on the
   orange, the teal or any status colour.
3. `slate` is decorative; `slate-light` is for anything a reader must perceive
   to operate. A focus ring uses `teal` (10.01:1 on `ink`, 9.38:1 on
   `surface`).
4. The four evidence colours are the only status colours and always ship with
   a written label beside them.
5. Two accents per screen, maximum: orange and teal.
6. Every token pair used in the built site is asserted against the numbers in
   §2 by a Vitest check that reads `site/src/lib/tokens.ts`; the Python table
   in §2 is the same list, reviewed alongside it.
