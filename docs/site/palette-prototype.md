# Prototype palette to site tokens

Source: `docs/site/prototype/climb-site.html` (429 lines). Colour values below are copied
literally from the file; contrast ratios are computed with the WCAG 2.x sRGB formula (see
`docs/site/prototype-audit.md` §5). No browser was used to sample gradients.

## Target tokens

The site design names six colours. Two of them have no prototype source and must be defined
before the first site component is styled.

| Token | Target value | Prototype source | Status |
| --- | --- | --- | --- |
| ink | `#07080A` | `--ink` (line 9); also `html{background:var(--ink)}` (line 11) | exact match |
| paper | `#F2EDE4` | `--cr` (line 9); card background (line 52) | exact match |
| orange | `#FF5400` | `--or` (line 9); primary button (line 19) | exact match |
| surface | **not yet defined** | no single source: page ground `#13161B` (line 123), `--g1 #1B1E25`, `--g2 #0F1217`, `--well #0D0F13` (lines 124–125) | prototype uses four near-neighbours; choose one surface and derive elevation from it |
| teal | **not yet defined** | none | the prototype has no teal; any value must be contrast-checked against ink and paper before use |
| red path | `#FF0000` | the two red strokes in the embedded logo (line 9, `#FF0000`) | exact match; logo only |

The prototype has no `teal`. Do not read the blue orb `rgba(24,96,200,.46)` (line 132) as teal:
it is an unrelated cool wash and should be dropped rather than promoted to a token.

## Prototype variables

| Prototype variable | Value | Where used | Maps to | Rule |
| --- | --- | --- | --- | --- |
| `--ink` | `#07080A` | header background (28), page ground (11), node fills (62) | `ink` | keep exact |
| `--cr` | `#F2EDE4` | body text (12), card fill (52), primary borders (18) | `paper` | keep exact |
| `--or` | `#FF5400` | result figure (46), primary button (19), progress bar (30), ladder path/selected state (61, 69), bar fill (80) | `orange` | keep exact |
| `--or2` | `#FE7F00` | extrapolated badge (23), focus outline (13), bright hover (19), ladder dot (62), header link hover (31) | **no separate token** | alias to `orange`; if a hover tint is needed, derive it with opacity, not a second brand orange |
| `--rose` | `#FFA6A6` | derived badge (22), ladder outer ring (64), second orb (133), hero tint (348), band trails (415–417) | **logo accent only** | remove from UI text, badges and canvas; only the logo gradient may use it |
| `--navy` | `#06234B` | not used in CSS; only `foot-navy` inside the embedded logo (line 9) | **logo accent only** | never text or icon colour on ink (1.29:1) |
| `--mar` | `#4D1111` | ladder root gradient end (63); `m-base` inside the logo (line 9) | **not a site token** | illustration/logo only; if the ladder root ships, its label must stay over the orange centre, not the maroon rim |
| `--mut` | `#A9A398` | secondary text (17, 39, 47, 48, 72, 92) | derived `paper` at 66 % (or a `paper-muted` token) | keep the value if a muted token exists; contrast 7.99:1 on ink |
| `--ln` | `rgba(242,237,228,.13)` | hairlines and control borders (28, 44, 45, 59, 67, 71, 73, 76, 80, 82, 84, 85, 87, 89) | `paper` at 13 % → derived `line` | **raise for interactive boundaries**; 1.32–1.41:1 fails the 3:1 non-text requirement |
| `--fd` | `"Bricolage Grotesque", …` | headings (14, 15, 38, 90) | display font token | self-host; see the audit §1 |
| `--g1` | `#1B1E25` | neumorphic control gradient start (170), raised panel | `surface` elevation | derivative, not a brand colour |
| `--g2` | `#0F1217` | neumorphic control gradient end (170) | `surface` elevation | derivative |
| `--well` | `#0D0F13` | ladder buttons (67), wells, bar track | `surface` well | derivative |
| `--logo` | data-URI SVG | header/hero wordmark (9) | logo asset | replace the data URI with the tracked logo files if the site can use an `<img>`/inline SVG |

## Gradients and effects

### Brand gradients

| Line | Declaration | Colours | Maps to | Disposition |
| --- | --- | --- | --- | --- |
| 63 | `.rt` ladder root, radial | `--or2` → `--or` at 42 % → `--mar` at 100 % | orange → maroon | keep as an illustration; dark text is 6.11:1 over the centre and 1.31:1 at the rim |
| 131 | first orb, radial | `rgba(255,84,0,.30)` → transparent | orange wash | optional; reduce or drop |
| 132 | second orb, radial | `rgba(24,96,200,.46)` → transparent | **off-palette blue** | drop |
| 133 | third orb, radial | `rgba(255,166,166,.16)` → transparent | rose (logo accent) | drop or replace with orange at low alpha |
| 160 | first result card | white radial highlight + `rgba(255,110,30,.2)` → white alphas | orange-ish, not exactly `orange` | replace the literal `255,110,30` with the orange token at low alpha |
| 59 | ladder panel | `rgba(77,17,17,.38)` → transparent, over `rgba(242,237,228,.02)` | maroon | illustration only; do not promote to a surface token |

### Glass, neumorphism and light

These are effects, not palette. They should become surface/elevation primitives so they cannot
drift into new colours.

| Line(s) | Declaration | Colours | Disposition |
| --- | --- | --- | --- |
| 140–146 | glass cards | white radial + `rgba(255,255,255,.085/.025/.055)`, 1 px white gradient edge, blur 16 px | surface elevation tokens; keep the white alphas as effect tokens |
| 147 | `.rf` filter variant | SVG turbulence filter + blur | progressive enhancement only |
| 151–152 | header glass | white radial + linear white alphas | surface elevation; header must remain opaque enough for 17:1 text |
| 170–171 | neumorphic buttons | `linear-gradient(145deg, --g1, --g2)`, black/white shadows | control elevation tokens |
| 63 | ladder root glow | `0 0 90px rgba(255,84,0,.3)` | orange glow token |
| 61–62 | ladder paths/nodes | path `rgba(242,237,228,.17)`, hover `rgba(254,127,0,.75)`, on `--or`; dot `--or2` with orange drop-shadow | orange + paper alphas |
| 69 | pressed control | `#1D1008` fill, orange ring and glow | orange at low alpha over ink |
| 101–104 | page index | paper alphas, orange cursor, muted readout | paper/orange alphas |
| 159–167 | card/panel radii and shadows | no new hues | layout tokens |

### Diagram and canvas colours

| Line(s) | Colours | Disposition |
| --- | --- | --- |
| 260–265 | `#A9A398`, `#F2EDE4`, `#FF5400`, `#07080A`, `#FFFFFF` | map to muted, paper, orange, ink, paper (white is not a token; replace with paper) |
| 270–272 | `rgba(242,237,228,.22)`, `rgba(255,84,0,.6)`, `#0D0F13`, `#F2EDE4`, `#A9A398`, `#FF5400` | paper/orange/surface/paper/muted/orange |
| 348 | `COL=[[255,84,0],[254,127,0],[255,166,166]]`; `TINT=['242,237,228','255,166,166','254,127,0']` | orange, orange2, rose; paper, rose, orange2 — remove rose from UI canvases |
| 356–366 | paper alphas, orange/rose orbit particles | paper + orange only; rose is a logo accent |
| 394 | paper and `rgba(254,127,0,…)` particles | paper + orange |
| 413–417 | paper alphas, `rgba(255,84,0,…)`, `rgba(255,166,166,…)`, `rgba(254,127,0,…)`, `#FF5400` | paper + orange; remove rose/orange2 duplication |
| 428 | glass pointer light | white alphas (effect token) |

## Embedded logo palette

The logo is a base64 SVG in `--logo` (line 9). Its gradients are a separate, closed palette and
must not leak into UI tokens:

| SVG gradient id | Stops | Note |
| --- | --- | --- |
| `c-edge` | `#C96262` → `#773F3F` transparent | a dusty red edge, not `rose` or `orange` |
| `m-orange` | `#FD5500` → `#FD5700` (57 %) → `#FE5900` transparent | three near-identical oranges; keep inside the asset |
| `m-rose` | `#D9D9D9` → `#F10000` transparent | **misnamed**: grey to pure red, no `#FFA6A6` |
| `m-base` | `#4D1111` transparent → `#4D1111` | maroon |
| `foot-navy` | `#06234B` → transparent | the only use of navy |
| `foot-rose` | `#E59B9B` → `#FFA6A6` transparent | the only use of the rose token |
| `foot-orange` | `#FE7F00` → transparent | bright orange |
| solid strokes | `#F2EDE4`, `#FF5400`, `#FF0000`, `#000000`, `#050505` | two red path strokes use `#FF0000` |

Consequences:

- `navy` and `rose` are logo-gradient accents exactly as the target token list says. The
  prototype violates that rule for rose in six UI places (badge, ring, orb, hero tint, two canvas
  trails); the site must not.
- The logo's `m-rose` is not the rose token; if the logo is redrawn, rename it to avoid a future
  maintainer assuming `#FFA6A6`.
- The logo's `#FFFFFF`-adjacent greys (`#D9D9D9`, `#C96262`, `#773F3F`) are asset-only and need
  no token.

## Proposed token set (for review)

| Token | Value | Provenance | Contrast check |
| --- | --- | --- | --- |
| ink | `#07080A` | exact prototype | paper 17.18:1 |
| paper | `#F2EDE4` | exact prototype | ink 17.18:1 |
| orange | `#FF5400` | exact prototype | on ink 6.22:1; ink on it 6.11:1 |
| surface | `#13161B` (proposal, from `html` line 123) | prototype page ground | paper on it 15.55:1; muted 7.23:1; orange 5.63:1 |
| surface-raised | `#1B1E25` (proposal, `--g1`) | prototype control gradient | muted 6.66:1 |
| line | `#5C5C5C` (proposal; paper at ≈36 %) | replaces 13 % hairline | 3.00:1 on ink — the minimum that passes; visually much stronger than the prototype |
| red-path | `#FF0000` | logo only | 5.01:1 on ink, but it is an asset stroke, not UI |
| navy | `#06234B` | logo gradient only | 1.29:1 on ink; never text |
| rose | `#FFA6A6` | logo gradient only | 10.74:1 on ink; do not use in UI |
| teal | **TBD** | no prototype source | must pass 4.5:1 on ink and on paper before adoption |

The `line` proposal is deliberately called out: the prototype's 13 % paper hairline fails the
3:1 non-text threshold, and simply renaming it to a token would carry the failure forward.
Passing 3:1 needs roughly `#5C5C5C` on ink, a far more visible rule than the prototype's. The
site therefore has a real choice: either raise the boundary contrast (and accept a more visible
grid) or keep the hairline decorative and add a separate, passing affordance (fill, icon, or
underline) to every interactive control.

## Rules for the site build

1. A colour used as text or as an icon must come from ink, paper, orange, teal, or a defined
   muted/line derivative, and must pass 4.5:1 on its actual background.
2. Navy, rose, maroon, the logo greys and the red path are asset colours. They may appear inside
   the logo SVG and nowhere else.
3. No second orange. `--or2`, `#FD5500`, `#FE5900`, `#FF6E1E` and `rgba(255,110,30,…)` all
   collapse to `orange` with an opacity, or are deleted.
4. No off-palette blue. The `rgba(24,96,200,.46)` orb is removed, not tokenised.
5. Gradients are defined from tokens, never from raw channel triples. The canvas code at lines
   348, 394 and 413–417 currently hardcodes `255,84,0`-style triples; the site version must read
   the token values once and pass them in.
6. Every token pair used in the UI gets a contrast test in the site's test suite. The table in
   §5 of the audit is the starting fixture.
