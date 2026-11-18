# Site brand

The design system for the CLIMB website, taken from the team's design authority:
`docs/site/prototype/climb-site.html`, 429 lines, 69,950 bytes, SHA-256
`0b0822338e004daba100bde1f98bcd9f2e5aa4c1f7b08198973d966a9570f108`.

Every colour, gradient, shadow, radius, duration and easing in this document was
read from that file, not retyped from memory. Line numbers below refer to it.
`scripts/site-check.py` holds the same token table and compares it against §1 on
every run, so a value cannot change in one place only; `python3
scripts/site-check.py contrast` prints §3.

**Migration status.** This document describes the skin the site is being ported
to. The built site still ships the previous palette until the components move
across; the file-by-file order is in `docs/site/skin-plan.md`. Where the two
disagree during the port, this document and the prototype win.

This is brand v3. It replaces palette v2 (three surfaces, slate boundary, four
status hues) outright. The contrast **method** is unchanged: WCAG 2.x sRGB
relative luminance, 4.5:1 for normal text, 3:1 for large text and for a
boundary a reader must perceive to operate a control, computed from literal
colour values.

## 1. The palette (dark-only)

One mode, no theme switch, no `prefers-color-scheme` branch. The document
ground is the prototype's slate `#13161B` (line 123), not `--ink`; the ink is
the deepest surface and the colour painted on a fill.

| Token | Value | Role | Source |
| --- | --- | --- | --- |
| `--ink` | `#07080A` | The deepest surface: ladder nodes, command wells, the ink painted on a fill. | line 9 |
| `--ground` | `#13161B` | The document background (`html{background:#13161b}`) and the `theme-color`. | line 123 |
| `--g1` | `#1B1E25` | Raised control gradient, light end. | line 124 |
| `--g2` | `#0F1217` | Raised control gradient, dark end. | line 124 |
| `--well` | `#0D0F13` | Inset wells: command bars, tab rails, bar tracks, ladder buttons at rest. | line 124 |
| `--cr` | `#F2EDE4` | Primary text; the paper card fill. | line 9 |
| `--mut` | `#A9A398` | Secondary text, captions, diagram labels, metadata. | line 9 |
| `--ink-card` | `#0B0B0D` | Text painted on the paper or orange fill. | lines 19, 51 |
| `--card-body` | `#33333A` | Body text on the paper capability card. | line 54 |
| `--or` | `#FF5400` | The brand orange: fills, rules, the selected state, chart data, links on hover. | line 9 |
| `--or2` | `#FE7F00` | The bright step of the same hue: focus ring, hover fill, the Extrapolated mark. | line 9 |
| `--rose` | `#FFA6A6` | The Derived evidence mark and the outer ladder ring. A status tint, not a second accent. | line 9 |
| `--navy` | `#06234B` | **Logo gradient only.** 1.17–1.29:1 on the ground; never text. | line 9 |
| `--mar` | `#4D1111` | **Logo and ladder-root gradient only.** 1.12–1.34:1; never text. | line 9 |
| `--ln` | `rgba(242,237,228,.13)` | Decorative hairline: section rules, table separators, panel edges. 1.32–1.44:1, so it never becomes a control boundary. | line 9 |
| `--edge` | `rgba(242,237,228,.48)` | An interactive boundary: input edges, resting control outlines that carry no other affordance. 4.31–4.45:1 on every surface, 4.13–4.17:1 on glass. | accessibility addition |
| `--glass-scrim` | `rgba(7,8,10,.78)` | The ink layer under every glass panel, before the white gradient. | accessibility addition |
| `--orb-warm` | `rgba(255,84,0,.30)` | Background orb, warm, top left. | line 131 |
| `--orb-cool` | `rgba(24,96,200,.46)` | Background orb, cool, right. It is the brightest wash and therefore the worst case for text on glass. | line 132 |
| `--orb-rose` | `rgba(255,166,166,.16)` | Background orb, rose, bottom. | line 133 |

Two additions are marked "accessibility addition" and are deliberate
deviations from the prototype's literal CSS; every other value is the
prototype's. They exist because the prototype's glass fails the contrast
method: muted text over the cool orb falls below 4.5:1 and orange falls below 3:1, and
the pointer specular pushes both lower; the checker's selftest removes the scrim and
asserts the failure, so this is a reproduced result, not an estimate. §4 states the budget that fixes it.

The previous palette's teal, red, slate, slate-light and the four status hues
(`measured`, `derived`, `analytical`, `extrapolated`) are **removed**. Evidence
is carried by a shape and a word (§8), not by four hues.

### Custom properties set at runtime

These are written by the page, not declared in a stylesheet; they are part of
the skin's contract because component CSS reads them.

| Property | Written by | Value | Used for |
| --- | --- | --- | --- |
| `--fd` | `:root` (line 14) | `"Bricolage Grotesque", …` | the display stack on headings and the ladder root |
| `--logo` | `:root` (line 9) | the inline SVG data URI | the wordmark; replaced by a tracked asset in the port |
| `--i` | the ladder renderer (line 314) | the capability row index | the `70ms` entrance stagger |
| `--d` | the reveal writer (line 386) | the child index | the `80ms` reveal delay |
| `--mx`, `--my` | the glass pointer light (line 428) | pixels from the panel's top-left | the specular position |
| `--sp` | the panel's own hover rule (line 143) | `.05` at rest, `.16` on hover | the specular alpha; the port clips it out of the text box (§3) |


## 2. Type

| Role | Face | Weights | Source |
| --- | --- | --- | --- |
| Display: headings, large numerals, the ladder root, menu links | **Bricolage Grotesque** | 600, 700, optical size 12–96 | line 7 stylesheet, applied at line 14 |
| Body, UI, diagrams | **Geist** | 400, 500, 600 | line 7, applied at line 12 |

| Token | Value | Use |
| --- | --- | --- |
| `--fd` | `"Bricolage Grotesque", …` | `h1`, `h2`, `h3`, `.nm`, `.rt`, mobile menu links |
| body stack | `Geist,"Helvetica Neue",Arial,system-ui,sans-serif` | everything else |
| mono stack | `ui-monospace,Menlo,monospace` at 12–12.5 px | command bars, sheet values, the page-index readout |

Headings carry `letter-spacing:-.03em` and `line-height:1.06`; `h1` tightens to
`-.04em`/`1`; the footer headline to `-.045em`/`.95`; `h2` is
`clamp(1.9rem,3.4vw,2.8rem)`; `h1` is `clamp(2.6rem,5.4vw,4.6rem)`; the intro
headline is `clamp(1.15rem,1.8vw,1.45rem)` at `1.25`. Body is
`400 16px/1.6`. Both faces are self-hosted (no Google Fonts request at load
time) and the licence check and subset plan are in `docs/site/skin-plan.md` §5.

## 3. Contrast (computed)

Computed with the WCAG 2.x formula from literal token values. Output of
`python3 scripts/site-check.py contrast`:

```
WCAG 2.x contrast, site tokens (rounded to two decimals)
thresholds: 4.5:1 normal text, 3.0:1 large text and UI boundaries

text on each surface
foreground            ink   ground       g1     well   target
cr                  17.18    15.55    14.30    16.45     4.5
mut                  7.99     7.23     6.66     7.65     4.5
or                   6.22     5.63     5.18     5.96     4.5
or2                  7.87     7.12        -        -     4.5
rose                10.74     9.72     8.94        -     4.5

text on glass, worst case over each orb wash (scrim + gradient top)
foreground       orb-warm orb-cool orb-rose   target
cr                  13.06    12.95    13.15     4.5
mut                  6.08     6.03     6.12     4.5
or                   4.73     4.69     4.76     4.5

card body text on the paper card
card-body        on cr           10.75  pass

ink on a fill
cr               ink #0B0B0D     16.86  pass   cr #F2EDE4      1.00  FAIL
                 rule: light text is never painted on a fill; use the ink.
or               ink #0B0B0D      6.11  pass   cr #F2EDE4      2.76  FAIL
                 rule: light text is never painted on a fill; use the ink.
or2              ink #0B0B0D      7.73  pass   cr #F2EDE4      2.18  FAIL
                 rule: light text is never painted on a fill; use the ink.

interactive boundary (edge, paper at 48 percent) against each surface
edge             on ink           4.42  pass
edge             on ground        4.42  pass
edge             on g1            4.31  pass
edge             on well          4.45  pass
edge             on glass:orb-warm      4.15  pass
edge             on glass:orb-cool      4.13  pass
edge             on glass:orb-rose      4.17  pass

decorative hairline and asset-only colours, never used as text or a boundary
ln               on ink           1.32  FAIL (decorative only)
ln               on ground        1.41  FAIL (decorative only)
ln               on g1            1.44  FAIL (decorative only)
ln               on well          1.37  FAIL (decorative only)
navy             on ink           1.29  FAIL (decorative only)
navy             on ground        1.17  FAIL (decorative only)
navy             on g1            1.07  FAIL (decorative only)
navy             on well          1.23  FAIL (decorative only)
mar              on ink           1.34  FAIL (decorative only)
mar              on ground        1.21  FAIL (decorative only)
mar              on g1            1.12  FAIL (decorative only)
mar              on well          1.28  FAIL (decorative only)

used pairs: 37, failures: 0
```

The `FAIL (decorative only)` lines are the checker asserting the rule: a token
that fails 3:1 must not be reachable as a boundary, so the check fails if one
starts passing by accident and the document's classification goes stale.

### What the method found, and the rules that fix it

1. **Text on glass.** The prototype's panel fill (line 140) is
   `radial-gradient(…rgba(255,255,255,.05),transparent 70%) +
   linear-gradient(150deg,rgba(255,255,255,.085),rgba(255,255,255,.025) 55%,rgba(255,255,255,.055))`
   over a blurred backdrop. Over the cool orb that composite leaves muted text
   below 4.5:1 and orange below 3:1, and the hover specular (`.16`, line 143)
   drops muted further below the floor. Adding the `--glass-scrim` ink layer at 78 % lifts the
   worst case to 6.03:1 and 4.69:1. Rule: **a glass panel paints
   `--glass-scrim` first**, then the gradient.
2. **The specular.** The pointer-tracked white radial may not sit under text at
   all: at `.16` even the cream drops to 5.02:1 and muted text fails. Rule: the
   specular layer is clipped to the panel's top 44 px, above the content box, so
   the sampled text region contains no specular. The checker's glass row is
   therefore sampled with the specular excluded, and the inventory records the
   clip for each panel.
3. **The 13 % hairline.** At 1.32–1.44:1 the prototype's `--ln` cannot be the
   only cue for a control. Rule: `--ln` stays decorative; a control whose
   resting shape is not otherwise legible takes `--edge`, and focus always takes
   the 2 px `--or2` outline.
4. **Light text on a fill fails.** `--cr` on `--or` is 2.76:1. Rule: text on a
   fill is always `--ink-card`, 6.11:1 on orange and 7.73:1 on the bright step.
5. **`--navy`, `--mar` and `--rose`.** Navy and maroon are logo gradients; they
   render at 1.07–1.34:1 on every surface and never become text, an icon or a
   boundary. Rose is a status tint at 8.94–10.74:1, so it may carry the Derived
   mark, but the mark is also half-filled and labelled.

## 4. Glass, neumorphism and the blur budget

Glass is a **floating layer**; neumorphism is a **tactile control**. The design
uses both, so the budgets replace the old "no backdrop-filter" rule.

| Layer | Classes that may blur | Blur | Why it is allowed |
| --- | --- | --- | --- |
| Header | `#hd` | `blur(16px) saturate(1.8) brightness(1.05)` | one floating bar; text is `--cr` at 12.95:1 worst case |
| Result rows and panels | `.led li`, `.tm li`, `.tr>div`, `.lim`, `.dg`, `.arch` | `blur(16px) saturate(1.7) brightness(1.06)` | each is a content panel over the orb field; scrimmed to 78 % |
| Ladder rows and detail | `.c`, `.l`, `.dt` | `blur(6px)` | small floating controls inside the ladder; 55 % fill plus scrim |

No other selector may declare `backdrop-filter`. `scripts/site-check.py docs`
fails on a selector that is not in this table.

**Panel stack, in paint order.** `--glass-scrim` (ink 78 %) → the glass
gradient (§7) → the 1 px mask-composited edge → the specular, clipped to the
top 44 px → content. The scrim is what makes the contrast numbers in §3 hold;
it is not decoration.

**Neumorphic controls** (buttons, wells, tabs, tracks, keyword chips, the page
index label) use `linear-gradient(145deg,var(--g1),var(--g2))` with the raised
shadow at rest and `--in` when pressed. They are the tactile layer: they never
blur, and they never sit on a glass panel without the scrim.

**Lite mode** (specified in full in `docs/site/skin-plan.md` §7, hooks only in
MS0) is selected by `saveData`, `deviceMemory <= 2` or `?lite=1`. It drops the
orbs, the glass blur, the specular, the scroll particle field and the hero
orbit field, and shows every animated value in its final state. The hooks
(`data-lite` on `<html>`, a `lite` module read at boot) exist in the plan so the
components do not have to be rewritten when it lands.

## 5. Shadow tokens

Copied verbatim from the prototype (lines 125–128). The four names are the
skin's elevation scale; a component uses one of them, not a new shadow.

| Token | Value | Use |
| --- | --- | --- |
| `--up` | `9px 9px 20px rgba(0,0,0,.55),-7px -7px 18px rgba(255,255,255,.04)` | raised panels, the mobile menu |
| `--up-s` | `5px 5px 11px rgba(0,0,0,.5),-4px -4px 10px rgba(255,255,255,.04)` | buttons at rest |
| `--up-xs` | `3px 3px 7px rgba(0,0,0,.5),-2px -2px 6px rgba(255,255,255,.04)` | small controls, tabs, chips |
| `--in` | `inset 6px 6px 13px rgba(0,0,0,.55),inset -5px -5px 12px rgba(255,255,255,.035)` | wells, tracks, pressed controls |

Composite shadows that are not tokens, because they are one component's
identity and are reproduced literally:

| Selector | Shadow | Source |
| --- | --- | --- |
| glass panels | `inset 0 1px 0 rgba(255,255,255,.26),inset 0 -1px 0 rgba(255,255,255,.05),inset 0 0 22px rgba(255,255,255,.03),0 14px 34px -16px rgba(0,0,0,.65)` | line 142 |
| `#hd` | `inset 0 1px 0 rgba(255,255,255,.3),inset 0 0 26px rgba(255,255,255,.04),0 18px 40px -16px rgba(0,0,0,.7)` | line 152 |
| `#pb` progress | none; 2 px `--or` bar | line 30 |
| `.rt` ladder root | `0 0 90px rgba(255,84,0,.3)` | line 63 |
| `.btn.p` | `6px 6px 14px rgba(0,0,0,.55),-4px -4px 10px rgba(255,255,255,.05),inset 1px 1px 0 rgba(255,255,255,.28)` | line 174 |
| `.btn.p:active` | `inset 5px 5px 11px rgba(120,25,0,.55),inset -3px -3px 8px rgba(255,150,80,.25)` | line 176 |
| `.fill` | `2px 2px 6px rgba(0,0,0,.5),inset 0 1px 0 rgba(255,255,255,.28)` | line 185 |
| `.c[aria-pressed=true]`,`.l[aria-pressed=true]` | `0 0 0 1px var(--or),0 0 30px rgba(255,84,0,.22)` | line 69 |
| `.nd.on` dot | `drop-shadow(0 0 6px #ff5400)` | line 62 |
| `.lad path.on` | `drop-shadow(0 0 5px rgba(255,84,0,.8))` | line 61 |

## 6. Radius scale

Read from every `border-radius` and `rx` in the file. The scale is the set of
values the design actually uses; a new radius is a design change.

| Token | Value | Selectors (source line) |
| --- | --- | --- |
| none | `0` | `.lad` full-bleed at ≥901 px (213) |
| `--r-2` | `2px` | `#pb`, `#nv a.on::after`, `.lim::after` (30, 156, 165) |
| `--r-3` | `3px` | `.cd .mo`, `.fill` at rest (55, 80) |
| `--r-4` | `4px` | `.cmd button`, `#rl a` old value, `#nv .btn` mobile, SVG rect `rx` (74, 101, 270) |
| `--r-6` | `6px` | `.btn`, `.cmd`, `#mn` (18, 73, 113) |
| `--r-8` | `8px` | `.c`, `.l`, `.dg`, `.arch` old value, `#rl a` (67, 82, 84, 188) |
| `--r-10` | `10px` | `.gs .cmd button` (179) |
| `--r-12` | `12px` | `.dt`, `#nv .btn`, `.tb button`, `.kw li` (71, 177, 182, 187) |
| `--r-14` | `14px` | `.btn`, `.lad` old value, `.gs .cmd` (171, 59, 178) |
| `--r-18` | `18px` | `.tb`, `.tm li` (181, 167) |
| `--r-20` | `20px` | `.led li`, `.tr>div`, `.lim`, mobile `#nv` (159, 163, 164, 191) |
| `--r-22` | `22px` | `#hd`, `.dg`, `.arch` (149, 161) |
| `--r-pill` | `999px` | `.trk`, `.fill` (184, 185) |
| `--r-round` | `50%` | `.rt`, `.rt::before/::after`, `.ev::before`, `.orbs i`, `.cd::after` (63, 64, 21, 130, 53) |

The paper capability card's top-right notch is a `clip-path`, not a radius:
`polygon(0 0,calc(100% - 72px) 0,calc(100% - 56px) 18px,100% 18px,100% 100%,0 100%)`
(line 52). The 72/56/18 px are the notch geometry.

## 7. Gradients

Every gradient in the file, with its literal declaration. Component code uses
these through tokens where a token exists; only the ladder-root and orb
gradients keep raw stops, because they are the illustration.

| Where | Declaration | Source |
| --- | --- | --- |
| Ladder root `.rt` | `radial-gradient(circle at 35% 30%,var(--or2),var(--or) 42%,var(--mar) 100%)` | 63 |
| Result row 1 | `radial-gradient(300px circle at var(--mx,20%) var(--my,-30%),rgba(255,255,255,calc(var(--sp,.05) + .03)),transparent 70%),linear-gradient(150deg,rgba(255,110,30,.2),rgba(255,255,255,.03) 60%,rgba(255,255,255,.06))` | 160 |
| Glass panels | `radial-gradient(280px circle at var(--mx,50%) var(--my,-30%),rgba(255,255,255,var(--sp,.05)),transparent 70%),linear-gradient(150deg,rgba(255,255,255,.085),rgba(255,255,255,.025) 55%,rgba(255,255,255,.055))` | 140 |
| Header `#hd` | `radial-gradient(420px circle at var(--mx,50%) var(--my,-60%),rgba(255,255,255,.12),transparent 70%),linear-gradient(160deg,rgba(255,255,255,.1),rgba(255,255,255,.03) 60%,rgba(255,255,255,.07))` | 150 |
| Primary button `.btn.p` | `linear-gradient(145deg,#ff6c22,#e84a00)`; hover `linear-gradient(145deg,#ff7a33,#f05200)` | 174, 175 |
| Neumorphic control | `linear-gradient(145deg,var(--g1),var(--g2))`; hover `linear-gradient(145deg,#20242c,#12151a)`; pressed `linear-gradient(145deg,#0e1115,#151920)` | 170, 172, 173 |
| Tab, selected | `linear-gradient(145deg,#1f232b,#13161b)` | 183 |
| Capability card fill | `var(--cr)`; hover `#fff`; the `.or` variant `var(--or)`; hover `var(--or2)` | 52, 56, 57 |
| Ladder panel | `radial-gradient(70% 60% at 0 50%,rgba(77,17,17,.38),transparent 70%),rgba(242,237,228,.02)`; full-bleed variant `radial-gradient(55% 55% at 12% 50%,rgba(77,17,17,.28),transparent 70%)` | 59, 214 |
| Orb, warm | `radial-gradient(circle,rgba(255,84,0,.30),transparent 62%)` | 131 |
| Orb, cool | `radial-gradient(circle,rgba(24,96,200,.46),transparent 62%)` | 132 |
| Orb, rose | `radial-gradient(circle,rgba(255,166,166,.16),transparent 62%)` | 133 |
| Derived evidence mark | `linear-gradient(90deg,currentColor 50%,transparent 50%)` | 22 |
| Hero canvas mask | `linear-gradient(#000 55%,transparent)` | 36 |
| Band canvas mask | `linear-gradient(90deg,transparent,#000 14%,#000 86%,transparent)` | 105 |
| Glass 1 px edge | `linear-gradient(135deg,rgba(255,255,255,.55),rgba(255,255,255,.07) 32%,rgba(255,255,255,.02) 62%,rgba(255,255,255,.34))` masked to the border box | 145 |
| Page-index link | `linear-gradient(145deg,var(--g1),var(--g2))` | 188 |
| Logo | the SVG's own gradients: `c-edge`, `m-orange`, `m-rose`, `m-base`, `foot-navy`, `foot-rose`, `foot-orange` | 9 |

Gradient text is forbidden: no `background-clip:text`, no `text-fill-color`
anywhere. The checker fails on either.

## 8. Evidence marks

Evidence is a **shape plus a word**; colour is decoration on the shape, never
the only channel. The chip is an inline-flex row, 13 px/500, `--cr`, gap 8 px,
with a 10 px mark (`border-radius:50%`, `1.5px solid currentColor`) before the
label (lines 20–23).

| Class | Mark shape | Mark colour | Label | Source |
| --- | --- | --- | --- | --- |
| Measured | solid disc (filled ring) | `--cr` | `Measured` | `.ev.m::before{background:currentColor}` |
| Derived | disc split half solid, half empty | `--rose` | `Derived` | `.ev.d` + `linear-gradient(90deg,currentColor 50%,transparent 50%)` |
| Analytical | dashed ring | `--cr` | `Analytical` | `.ev.a::before{border-style:dashed}` |
| Extrapolated | dotted ring, 2 px | `--or2` | `Extrapolated ≈` | `.ev.x::before{border-style:dotted;border-width:2px}` |
| no class | mark hidden | — | empty string | `.ev.n{display:none}` |

The legend at the foot of the results section spells the four out in words
(`Measured: recorded directly`, `Derived: computed from measurements`,
`Analytical: closed-form model`, `Extrapolated: unit cost × count`). A value
with no measurement class carries a provenance word instead: `Constant` (a
fixed protocol or model input) or `Generated` (a count a tool emits), both in
`--mut` with no mark.

## 9. Duration and easing inventory

Every transition, animation and timed effect in the file. The animation
inventory (`docs/site/animation-inventory.md`) is the effect-level view; this is
the value-level one, so a component can be ported without re-reading the
prototype.

| Selector / effect | Property | Duration | Easing | Delay | Source |
| --- | --- | --- | --- | --- | --- |
| `.cd::before` | `background` | `.3s` | ease | — | 52 |
| `.cd .mo` | `max-height,opacity,padding` | `.35s` | ease | — | 55 |
| `.rt` | `opacity` | `1s` | ease | `.2s` | 63 |
| `.rt::before` | rotation | `50s` | linear, infinite | — | 64 |
| `.rt::after` | rotation, reversed | `80s` | linear, infinite | — | 64 |
| `.c`,`.l` | `border-color,box-shadow,transform,background,opacity` | `.25s` / `opacity .7s` | ease | `calc(var(--i,0)*70ms + .3s)` | 67 |
| `.dt` | `opacity` | `1s` | ease | `.5s` | 71 |
| `.tb button` | state change | none | — | — | 77 |
| `.fill` | `width` | `1s` | `cubic-bezier(.2,.7,.2,1)` | — | 80 |
| `.arch rect` | `stroke,fill` | `.3s` | ease | — | 109 |
| `.js .r` | `opacity,transform` | `.9s` | `cubic-bezier(.2,.7,.2,1)` | `calc(var(--d,0)*80ms)` | 96 |
| `.js .cl .cs` | `clip-path` | `1.1s` | `cubic-bezier(.7,0,.2,1)` | — | 98 |
| `#rl a` | `opacity` | `.25s` | ease | — | 101 |
| `.dg svg rect` | `grw` grow | `.7s` | `cubic-bezier(.2,.7,.2,1)`, `both` | `.12s` / `.24s` per rect | 107 |
| `.dg .fw` | `fw` dash offset | `.9s` | linear, infinite | — | 108 |
| `.arch path` | `fl` dash offset | `1.2s` | linear, infinite | — | 109 |
| `.arch rect[stroke="#ff5400"]` | `pl` pulse | `2.6s` | ease-in-out, infinite | — | 109 |
| `.orbs i:nth-child(1)` | `dr1` drift | `30s` | ease-in-out, alternate, infinite | — | 131 |
| `.orbs i:nth-child(2)` | `dr2` drift | `36s` | ease-in-out, alternate, infinite | — | 132 |
| `.orbs i:nth-child(3)` | `dr3` drift | `42s` | ease-in-out, alternate, infinite | — | 133 |
| ladder connector path | `stroke-dashoffset` | `1.2s` | `cubic-bezier(.5,0,.2,1)` | `0`, then `.08s` per capability, `.1s + .1s` per branch, `.4s` to the detail | 324, 326–328 |
| ladder travelling dot | SMIL `animateMotion` | `2.6s` | — | — | 329 |
| count-up | text | `1400ms` | cubic ease-out `1-(1-p)^3` | — | 341 |
| bar fill (scroll) | `width` | `1s` (CSS) | `cubic-bezier(.2,.7,.2,1)` | — | 343 |
| copy confirmation | label text | `1400ms` until reset | — | — | 302 |
| hero ripple | canvas frame | `4000ms` life | linear age | spawn every `5200ms` | 355, 367 |
| hero orbit | canvas frame | per-frame `dt ≤ .05s` | — | — | 366 |
| scroll particles | canvas frame | per-frame | — | — | 393 |
| heading burst | canvas particles | `.012` life per frame | gravity `.02`, drag `.98` | — | 391, 395 |
| Merkle band | canvas frame | spawn `380ms`, packet `110px/s`, lit decay `1.1` | — | — | 407, 408, 411 |
| Ladder band | canvas frame | spawn `650ms`, `70–120px/s` | — | — | 407 |

`prefers-reduced-motion:reduce` removes the smooth scroll, the `.fill`
transition, every `animation` (rings, diagram grow, dashed flow, pulse, orbs),
the reveal and clip-wipe transitions, and every JavaScript loop; the JavaScript
guards are listed per effect in the animation inventory. SMIL is not ported at
all: the two `animateMotion` uses (lines 264, 329, 399) are replaced by the same
dot drawn on the existing canvas or by a static final state.

## 10. Orbs

Three fixed, `aria-hidden`, non-interactive washes behind everything
(`.orbs`, line 129): `46vmax` circles at `-14vmax/6vh`, `right -16vmax/34vh`
and `left 28vw/bottom -26vmax`, drifting on `dr1`/`dr2`/`dr3`. They are the
reason the glass budget exists. Lite mode hides them entirely; reduced motion
freezes them (their final state is the untransformed circle). Their colours are
`--orb-warm`, `--orb-cool`, `--orb-rose` in §1.

## 11. Assets and metadata

- Header and hero wordmark: the prototype's inline SVG (`--logo`, line 9),
  replaced by the tracked vector assets under `site/public/logo/` so the file is
  cacheable and the alt text is real text, not a data URI.
- Favicon: the prototype's inline data URI (line 6) is replaced by the tracked
  `favicon.svg` / `favicon.ico` set.
- `theme-color` becomes `#13161B` (the document ground), not `#07080A`.
- Open Graph card: the existing L1 card, unchanged by this skin.

## 12. Rules for the site build

1. A colour used as text, an icon or a boundary comes from §1 and nowhere else.
   No raw channel triples in component code, except the illustration gradients
   §7 names.
2. Text on a fill is `--ink-card`. Light text never sits on the orange or the
   paper card.
3. Every glass panel paints `--glass-scrim` before its gradient; a panel without
   the scrim fails §3 and the checker.
4. The pointer specular is clipped to the top 44 px of a panel and never
   underlies text.
5. `--ln` is decorative; anything a reader must perceive to operate takes
   `--edge` or the `--or2` focus ring.
6. Evidence is a mark shape plus a word (§8); colour is never the only channel.
7. At most two accent colours are visible at once: `--or` and `--or2` are the
   same hue's two steps and count as one accent; `--rose` is a status tint.
8. Every effect in `docs/site/animation-inventory.md` has a reduced-motion state
   and a parity assertion; SMIL is not allowed.
9. Blur is allowed only for the selectors in §4; at most five `<canvas>` tags
   are declared and at most nine canvas elements exist, and the five band
   canvases run only while visible.
10. Every token pair used in the built site is asserted by
    `scripts/site-check.py contrast` from this table, and the table in §3 is
    that command's output.
