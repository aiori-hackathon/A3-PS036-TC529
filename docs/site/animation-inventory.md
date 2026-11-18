# Animation inventory

The effects in `docs/site/prototype/climb-site.html` (429 lines, SHA-256
`0b0822338e004daba100bde1f98bcd9f2e5aa4c1f7b08198973d966a9570f108`), as the
port must reproduce them. Every number in the **constants** column is read from
the file. The **selector** column names elements that exist in the file:
`scripts/site-check.py docs` asserts each one is present and that every
`@keyframes` name appears in a row.

Columns: effect, selectors, what starts it, the literal constants, the timing,
the reduced-motion state, and the parity assertion that proves the port kept the
behaviour. A parity assertion is a test that fails when the effect's constants
or its reduced-motion state drift; the file that will own it is named in
`docs/site/skin-plan.md` §4.

| # | Effect | Selector(s) | Trigger | Constants (verbatim) | Duration / easing | Reduced motion | Parity assertion |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | Orb drift | `.orbs i:nth-child(1)`, `.orbs i:nth-child(2)`, `.orbs i:nth-child(3)` | paint, always on | `46vmax` circles; `dr1 30s`, `dr2 36s`, `dr3 42s`; `translate3d(42vw,26vh,0) scale(1.15)`, `translate3d(-38vw,-22vh,0) scale(.9)`, `translate3d(-30vw,-16vh,0) scale(1.2)` | `ease-in-out`, `infinite alternate` | `animation:none`; the circles stay at their untransformed size (final state) | parity: three orb elements exist, each carries its own keyframe name, and the computed transform is `none` under `prefers-reduced-motion:reduce` |
| 2 | Hero grid ripple | `#gc` | one automatic ripple on load, then every `5200ms`; a pointer press adds one at the press point | cell `CELL=56`, wave speed `260`, band width `70`, ripple life `4000`, automatic amplitude `.8`, press amplitude `.6`, line alpha `.05`, ripple alpha cap `.5` on lines and `.7` on nodes | one frame per animation frame, `dt` clamped to `.05` | one static frame: the ripple at `performance.now()-1200`, no loop, no pointer listener | parity: the grid pitch equals `56` CSS px at dpr 1 and 2, the ripple radius grows by `260 px/s`, and the canvas has no animation frame scheduled under reduced motion |
| 3 | Hero orbit field | `#ob`, `#of` | paint, plus pointer proximity repel | `720` particles, `360` below `700px`; rings `{A:215,B:70,L:-22*Math.PI/180}` and `{A:185,B:104,L:26*Math.PI/180}`; repel radius `84`, strength `11`, lerp `.12`; dpr cap `2`; tint split `.72`/`.88` | one frame per animation frame, `dt` clamped to `.05` | one static frame; no pointer listener; the two canvases keep the logo sandwich | parity: particle count is `720` at width 1280 and `360` at width 640; the repel starts inside `84` px and moves a particle at most `11` px |
| 4 | Header progress line | `#pb` | scroll, coalesced into one frame | height `2px`; `transform:scaleX(p)`; `p = min(1, scrollY/(scrollHeight-innerHeight))` | none (direct write, no transition) | same values, no transition | parity: the scale equals the clamped ratio after a scripted scroll to 50 % and to the end |
| 5 | Page index rail | `#rl`, `#ix`, `#pr` | scroll and resize | `72` ticks (`N=72`); tick width `(major?16:7) + 20*Math.exp(-e*e/9)`; rail `40px` wide, `min(64vh,540px)` tall; label offset `50px`; percentage text | tick and cursor writes are direct; label opacity `.25s` | same geometry; no opacity transition | parity: exactly `72` tick elements and one link per section, the cursor sits at the scroll ratio, and the readout is the rounded percentage |
| 6 | Scroll particle field | `#fx` | paint and scroll velocity | `130` particles, `60` below `700px`; size `1.6` for the orbit dots; alpha `.12+.35`; speed `1+min(abs(sv2)*.06,3)`; trail elongation `.6`; one in six is orange | one frame per animation frame | not started at all | parity: the field is empty under reduced motion, and the speed multiplier caps at `4` after a fast scroll |
| 7 | Heading burst | `h2` | a heading entering the viewport | `34` particles; speed `.5+2.2`; gravity `.02`; drag `.98`; life step `.012`; size `1+1.4`; tint paper 60 % / orange 40 % | one frame per animation frame, life 1 → 0 in ~83 frames | `burst()` returns before creating anything | parity: a heading adds exactly `34` particles once, and zero under reduced motion |
| 8 | Reveal fade-up | `.js .r` | element enters the viewport at `threshold:.15`, `rootMargin:0px 0px -6% 0px` | `translateY(24px)` → none; `opacity:0` → `1`; stagger `calc(var(--d,0)*80ms)` | `.9s cubic-bezier(.2,.7,.2,1)` | `opacity:1!important; transform:none!important; transition:none!important` | parity: every reveal target ends at opacity `1` and transform `none`, and the delay is `80ms` times the child index |
| 9 | Heading clip wipe | `.js .cl .cs` | heading enters the viewport | start `clip-path:inset(-.2em 100% -.2em 0)`, end `inset(-.2em -.1em -.2em 0)` | `1.1s cubic-bezier(.7,0,.2,1)` | `clip-path:none!important; transition:none!important` | parity: the end inset is `-.2em -.1em` and the wipe is absent under reduced motion |
| 10 | Result count-up | `.nm` (the `[data-n]` spans inside it) | the bars block at `threshold:.25`, the results list at `threshold:.2` | `1400ms`; progress `p=min((t-t0)/1400,1)`; value `n*(1-(1-p)^3)`; `toLocaleString('en',{minimumFractionDigits:d,maximumFractionDigits:d})` | cubic ease-out, one rAF per frame | spans keep the server-rendered text | parity: the last frame equals `formatFactValue`, and no frame runs under reduced motion |
| 11 | Wire bar fill | `.fill` | the bars block at `threshold:.25` | widths `94.3`, `100`, `3.3` from the byte facts; label offsets `8px`; value text from the fact | `width 1s cubic-bezier(.2,.7,.2,1)` | width set immediately, `transition:none` | parity: each bar's width equals its `data-w` and its label equals its fact |
| 12 | Merkle band | `.bd canvas[data-m="0"]` | the canvas is at least partly visible | `8` leaves below `520px`, else `16`; levels `log2(n)`; spawn `380ms`; packet cap `8`; packet speed `110`; lit decay `1.1`; node size `3+l*1.2`; trail `9` dots at `6` px spacing | one frame per animation frame | `sz()` draws one static frame, then returns before the loop | parity: 16 leaves produce 3 levels and 15 edges, and 8 leaves at a 500 px canvas |
| 13 | Ladder band | `.bd canvas[data-m="1"]` | the canvas is at least partly visible | `3` rungs; column spacing `SP=46`; spawn `650ms`; packet speed `70+50` random; reroute chance `.45`; trail length `30`; rung y `H*(.26+r*.24)`; packet cap `8` | one frame per animation frame | one static frame, no packets | parity: three rungs at the documented y ratios, and no packet after the first frame under reduced motion |
| 14 | Ladder connector draw-in | `#sv`, `.lad path` | the ladder enters the viewport (`revLad`) | `pathLength=1`; `stroke-dashoffset` `1` → `0`; node radius `3.5`; delays `0` then `i*.08` per capability, `.1+j*.1` per branch, `.4` to the detail pane; bezier midpoint `(x1+x2)/2`; width breakpoint `900` | `stroke-dashoffset 1.2s cubic-bezier(.5,0,.2,1)`, `stroke .3s`, `stroke-width .3s` | paths are released to offset `0` with no transition | parity: the path count is capabilities + branches + 1, every path ends at offset `0`, and no path is drawn at or below `900` px |
| 15 | Ladder travelling dot | `.pd` | after the connector draw-in | radius `3.5`; one dot on the selected capability's path, placed by script with `getPointAtLength` on that path; lap `2.6s` | one lap per `2.6s`, looped by `requestAnimationFrame` | not created | parity: exactly one dot exists, its path is `pc[ac]`, two samples `400ms` apart differ and stay within `2px` of the path, and none exists under reduced motion or lite mode |
| 16 | Ladder ring rotation | `.rt` | paint, always on | `::before` inset `-20px`, `::after` inset `-40px` dotted; `sp 50s` forward and `80s` reversed | `linear infinite` | `animation:none`; both rings stay at rotation `0` | parity: two pseudo-element rings exist with the two durations, and no rotation animation runs under reduced motion |
| 17 | Ladder row stagger | `.c`, `.l` | the ladder gets `.in` at `threshold:.15` | `--i` is the row index; opacity delay `calc(var(--i,0)*70ms + .3s)`; hover `translateX(5px)` | `opacity .7s`, `border-color .25s`, `box-shadow .25s`, `transform .25s`, `background .25s` | opacity `1`, no transform, no delay | parity: row `i` has delay `i*70+300` ms, and the hover translate is absent under reduced motion |
| 18 | Glass pointer light | `#hd`, `.led li` | pointer movement over a panel | panel radial `280px`, header radial `420px`; `--mx`/`--my` in px from the panel's top-left; `--sp` `.05` at rest, `.16` on hover; specular clipped to the top `44px` | one rAF per move event; no transition | no listener; `--sp` stays `.05` | parity: `--mx` equals the pointer offset inside the panel, the specular rect is at most `44px` tall, and no listener is attached under reduced motion or in lite mode |
| 19 | Copy confirmation | `.cmd button` | a click on `[data-c]` | label `Copy` → `Copied`; reset after `1400ms`; the clipboard receives `dataset.c` verbatim | state change only | same state change, no transition | parity: the button reads `Copied` for `1.4s`, returns to `Copy`, and the clipboard text equals the command |
| 20 | Capability card notch and command | `.cd` | hover or keyboard focus | notch `polygon(0 0,calc(100% - 72px) 0,calc(100% - 56px) 18px,100% 18px,100% 100%,0 100%)`; fill `--cr` → `#fff`; command `max-height:40px`, `padding:6px 8px`; `@media(hover:none)` opens it permanently | `background .3s`, `max-height/opacity/padding .35s` | fill and command state change with no transition | parity: the notch polygon is byte-identical, and a touch device shows the command without hover |
| 21 | Configuration switch | `.tb`, `.fill`, `#gA`, `#gB`, `#gC`, `#gt` | tab click, Arrow Left/Right, Home/End; `sel(0)` on load | three configurations; roving `tabindex`; `aria-selected`; group display toggles; bar widths from the byte facts; caption text from the table | bar `1s cubic-bezier(.2,.7,.2,1)`; the rest is a state change | bar width set immediately; no tab transition | parity: the selected tab, the visible diagram group and the caption agree for A, B and C, and Arrow Right wraps `2` → `0` |
| 22 | Diagram and architecture flow | `.arch path`, `.arch rect`, `.dg .fw` | paint, always on while the drawing is on screen | arch dash `4 6`; diagram dash `5 5`; `fl 1.2s`, `fw .9s`, `pl 2.6s`; grow `grw .7s` with delays `.12s` and `.24s`; hover fill `#1a0f09`, stroke `#ff5400`; packets placed by script: `3` in the diagram (`r 3`, white, `2s`, begins `0`, `.35s`, `.7s`) and `9` in the architecture (`r 3`, `#ff5400`, `2.6s`, begin `j*.5+gi*.3`s, one per path segment) | `linear infinite`, `ease-in-out infinite`, `cubic-bezier(.2,.7,.2,1)` for the grow; packets loop once per duration | every `animation:none`; no packet is created; dashes and fills stay at their static values | parity: the two dash patterns, the three keyframe names (`grw`, `fw`, `pl`), `3` + `9` packets that move, stay within `2px` of their path and are absent under reduced motion and lite mode, and a static diagram under reduced motion |

## Coverage

Every animated declaration in the prototype belongs to exactly one row above.
The mapping, so a new declaration cannot slip in unrecorded:

| Prototype line | Declaration | Row |
| --- | --- | --- |
| L64 | `.rt::before`/`::after` `animation:sp` | 16 |
| L65 | `@keyframes sp` | 16 |
| L106 | `@keyframes grw`, `fl`, `fw`, `pl` | 22 |
| L107 | `.dg svg rect` grow | 22 |
| L108 | `.dg .fw` dash flow | 22 |
| L109 | `.arch path`, `.arch rect`, the pulse rule | 22 |
| L131–133 | `.orbs i` drift | 1 |
| L134–136 | `@keyframes dr1`, `dr2`, `dr3` | 1 |
| L60–62 | ladder path draw-in classes (`hv`, `on`) | 14 |
| L67–69 | `.c`, `.l` stagger and pressed state | 17 |
| L71 | `.dt` opacity on reveal | 8 |
| L96–98 | `.js .r` reveal, `.js .cl .cs` wipe | 8, 9 |
| L101–104 | index rail opacity, cursor, readout | 5 |
| L118, 194 | the two reduced-motion blocks | 8, 9, 1, 22 |
| L302 | copy confirmation timer | 19 |
| L324–331 | connector draw-in and the travelling dot | 14, 15 |
| L341–345 | count-up and bar fill | 10, 11 |
| L346–372 | hero grid and orbit field | 2, 3 |
| L375–387 | index rail geometry and scroll spy | 5 |
| L388–396 | scroll field and heading burst | 6, 7 |
| L264 | diagram packets (three circles, `dur=2s`) | 22 (restored, JS) |
| L399 | architecture packets (nine circles, `dur=2.6s`) | 22 (restored, JS) |
| L401–421 | the two band canvases | 12, 13 |
| L426–428 | glass pointer light | 18 |

## SMIL replaced by script

The prototype's three `animateMotion` uses are kept as motion, but not as SMIL:
a media query cannot stop SMIL, so each packet is a plain `<circle>` moved by
script along the same path, with the same radius, colour, duration and begin
offset. They are created only when reduced motion is not set and lite mode is
off, each set advances only while its drawing is on screen, nothing advances
while the tab is hidden, and leaving the page removes every circle and helper
path. No `<animateMotion>` exists in the source or the built page.

| Prototype | Why not SMIL | What the port does |
| --- | --- | --- |
| diagram packets (`dur="2s"`, `begin="0"`/`".35s"`/`".7s"`, line 264) | SMIL cannot be stopped by a reduced-motion media query, and the audit found it kept moving for a reduced-motion reader | restored, JS: three `r=3` white circles follow `M180 42V69H60V96`, `M180 42V96` and `M180 42V69H300V96`, one lap per `2s` with begins `0`, `.35s`, `.7s` (row 22) |
| travelling dot (line 329) | same | restored, JS: row 15 places one `r=3.5` dot on `pc[ac]` by `getPointAtLength`, one lap per `2.6s` |
| architecture packets (line 399) | same | restored, JS: one `r=3`, `#ff5400` circle per path segment (nine in all), one lap per `2.6s`, begin `j*.5+gi*.3`s (row 22) |
| menu open/close (line 301) | it is a display toggle, not an animation | a class toggle; the only transition is the panel's own |
| tab selection (line 339) | a state change | `aria-selected` and a background, no motion |

All 22 effects are therefore present. Three of them (rows 15, 22 and the
diagram's share of row 22) are drawn by script instead of SMIL, which the
reduced-motion rule requires; the motion itself is the prototype's.
