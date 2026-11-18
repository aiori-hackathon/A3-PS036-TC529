# Site design rules

The gate list for the CLIMB skin. Every rule states what it forbids, why, and a
**pass test** a reviewer can point at or the build can run, so "looks fine to
me" is not a verdict. Tokens come from `docs/site/brand.md`, routes from
`docs/site/ia.md`, words from `docs/site/copy.md`, effects from
`docs/site/animation-inventory.md`.

This revision moves the design authority to `docs/site/prototype/climb-site.html`.
The prototype is the source of truth for class names, DOM structure, timings and
easings; the rules below are the constraints the port must satisfy while
keeping that design. Where a rule and the prototype disagree, the rule wins only
where accessibility, CSP, reduced motion or a stated budget requires it, and the
deviation is recorded in `docs/site/skin-plan.md` §8.

## 1. Rule verdicts against the previous set

Ten rules existed before. Each is kept, changed or removed for a stated reason.
Rules 11–14 are new.

| # | Rule | Verdict | Reason |
| --- | --- | --- | --- |
| 1 | One idea in the hero | CHANGE | The hero is now the wordmark at full width with a small intro beneath it: one headline, one lede, two actions, one metadata line. The single idea survives; its composition is the prototype's, not a text block beside the artwork. |
| 2 | No gradient text | KEEP | The prototype never fills type with a gradient, and flat cream on ink is what makes the numerals legible at 17.18:1. Unchanged, and now checked in source rather than by eye. |
| 3 | No glass cards in a grid | REMOVE | Glass is the design: floating result rows, panels and the header. The failure this rule prevented (blurred tiles lowering contrast) is answered precisely by the glass budget in §3 and the worst-case sampling in §4, which are stronger than a ban. |
| 4 | No identical three-column feature rows | KEEP | One three-up row (`Transparency`) appears once; no route repeats it. The seven capability cards and the sixteen ladder branches are an index with distinct content, not a template of icons. |
| 5 | One deliberate decorative element per page | REMOVE | The page has three named kinds of atmosphere (orbs, three hero canvases, one scroll field, five section bands) and a bounded light system. A count of "one" was the wrong instrument; §5 caps canvases, pauses them when off-screen, names each one's job, and gives lite mode a switch that removes them. |
| 6 | No icon packs | KEEP | Every mark is drawn in the site's own SVG or CSS (the evidence disc, the notch, the ladder nodes). No icon font, no sprite sheet. |
| 7 | At most two accent colours per screen | CHANGE | Orange has two steps (`--or`, `--or2`) that are one hue and count as one accent. `--rose` is not an accent: it is the Derived evidence tint, always accompanied by a half-filled disc and the word. Teal, red, violet, green and blue are gone from the UI. |
| 8 | Motion has a job and a reduced-motion state | KEEP | Strengthened: every one of the 22 effects has a named job, a reduced-motion state and a parity assertion in the inventory. An effect with no state fails the build, not the review. |
| 9 | Colour is never the only channel | KEEP | Evidence is a mark shape (solid, half, dashed, dotted) plus a word. The four status hues are deleted, so the shape and the word are now the only channels and cannot be lost in a monochrome rendering. |
| 10 | Numbers carry their evidence class and baseline | KEEP | Every number on a page still comes from a fact id through `Fact.astro`, with its chip and the baseline it is compared against. The skin changes the typography, not the provenance. |
| 11 | Text on glass is contrast-tested by sampling the worst case over the orbs | NEW | The panel composite depends on what is behind it. A single token pair cannot prove legibility, so the checker samples the brightest orb wash, composites the scrim and the glass gradient, and asserts the used text pairs against that backdrop. |
| 12 | A glass panel paints its scrim before its gradient | NEW | Without the ink scrim at 78 %, muted text over the cool orb falls below 4.5:1 (the selftest asserts it). The scrim is what makes §4 pass, so it is a rule, not a stylistic default. |
| 13 | Blur is a budget, not a default | NEW | Only the selectors in §3 may declare `backdrop-filter`; every other panel is opaque. The checker fails on a selector outside the allowlist. |
| 14 | Five band canvases, running only while visible; lite mode exists | NEW | The prototype instantiates five section-band canvases plus three hero canvases and one scroll field. The budget caps simultaneous work, pauses off-screen loops, and defines a lite mode that removes the atmosphere on a constrained device. |

## 2. The checklist a screen must pass

1. The hero states one thing and offers at most two actions (rule 1).
2. No `background-clip:text`, no `text-fill-color`; one flat colour per glyph
   (rule 2).
3. Every panel is either on the blur allowlist or opaque (rules 3, 13).
4. No repeated three-up feature row (rule 4).
5. Every canvas is one of the named ones, is paused when off-screen, and shows
   its final frame under reduced motion and lite mode (rules 5, 8, 14).
6. Every effect in the inventory exists, with its selector and constant, and has
   a reduced-motion state (rule 8).
7. Evidence is a shape and a word, never a hue alone (rule 9).
8. Every number has a fact id, a chip and a baseline (rule 10).
9. Text on a glass panel clears 4.5:1 against the sampled worst case (rules 11,
   12).
10. At most two orange steps and one status tint are visible (rule 7).

## 3. The glass budget

Glass is allowed on these selectors and nowhere else. The value is the blur
declaration copied from the prototype; the reason is the text that sits on it.

| Selector | Blur | Text it must carry |
| --- | --- | --- |
| `#hd` | `blur(16px) saturate(1.8) brightness(1.05)` | navigation, the wordmark, one action |
| `.led li` | `blur(16px) saturate(1.7) brightness(1.06)` | the four headline figures, their chips and baselines |
| `.tm li` | same | team rows |
| `.tr>div` | same | the three transparency cards |
| `.lim` | same | the known-limits paragraph |
| `.dg` | same | the three-message diagram and its legend |
| `.arch` | same | the architecture diagram |
| `.c`, `.l` | `blur(6px)` | capability and branch buttons |
| `.dt` | `blur(6px)` | the ladder detail pane |

Panel paint order: `--glass-scrim` → the white gradient → the 1 px masked edge
→ the specular, clipped to the top 44 px → content. The specular is clipped
because at `.16` it drops muted text below the floor; the clip keeps it out of the
text box, so the sampled region is the scrim plus the gradient.

Neumorphic controls (buttons, wells, tabs, tracks, keyword chips, the index
label) never blur: they are the tactile layer, and a blurred control would put
the light system on top of the content system.

## 4. Text on glass is sampled, not assumed

`python3 scripts/site-check.py contrast` computes, for each orb:

```
backdrop = orb (its own alpha) over --ground
         → --glass-scrim over that
         → rgba(255,255,255,.085) over that      (the gradient's top stop)
```

and prints `cr`, `mut` and `or` against that backdrop plus the `--edge`
boundary. The current worst case is the cool orb: `cr` 12.95:1, `mut` 6.03:1,
`or` 4.69:1, `--edge` 4.13:1. The checker's selftest removes the scrim and
asserts that muted text then fails, so the rule cannot be satisfied by deleting
the scrim and editing the table.

The method is a sample of the CSS colour stops, not a screenshot: no browser
runs in the gate. The pixel-sampling version (a rendered frame, the brightest
pixel under each text run) is the MS3 acceptance test in
`docs/site/skin-plan.md`; until then this composite is the contract, and it is
deliberately pessimistic (worst orb, top gradient stop, no dithering).

## 5. The canvas budget

| Canvas | Instances | Job | State under reduced motion | State in lite mode |
| --- | --- | --- | --- | --- |
| `#gc` hero grid | 1 | shows the wordmark radiating into the grid; a click sends one ripple | one static frame, no loop, no pointer listener | hidden |
| `#ob`/`#of` hero orbits | 2 | a particle field split around the logo so the mark sits inside it | one static frame | hidden |
| `#fx` scroll field | 1 | drifting particles whose speed follows the scroll; a burst on a heading | not started | hidden |
| `.bd canvas[data-m="0"]` | 3 | hashes climbing a Merkle tree | one static frame | hidden |
| `.bd canvas[data-m="1"]` | 2 | packets hopping the ladder rungs | one static frame | hidden |

Rules:

- At most **nine** canvas elements exist on the page: one hero grid, two orbit
  canvases (the logo sandwich), one scroll field and five section bands. They
  are declared by at most **five** `<canvas>` tags, because the band component
  is instantiated five times and the orbit pair is one component. The checker
  counts `<canvas` in `site/src` and fails above five.
- At most **four** animation loops run at once: the hero loop (grid plus
  orbits), the scroll field, and one loop per band that is currently visible.
  The bands are 160 px dividers, so two are visible at most in practice, and
  every loop stops when its canvas leaves the viewport.
- Every band canvas runs only while it is intersecting the viewport; the
  `IntersectionObserver` that reveals it also starts and stops the loop.
- The hero canvases pause as soon as the hero leaves the viewport.
- A canvas is `aria-hidden="true"` and carries no figure. If it shows a number,
  the number is HTML with a fact id.
- No canvas may be created outside the component that owns it, and no loop may
  run without a visibility gate and a reduced-motion branch.

## 6. Lite mode

Selected when any of: `navigator.connection.saveData`, `navigator.deviceMemory
<= 2`, or `?lite=1`. It sets `data-lite="on"` on `<html>` before first paint and
exposes a store the components read once at boot. In lite mode:

- the orbs, the two ambient canvases, the five band canvases and the pointer
  specular are not created;
- the glass keeps its scrim and gradient but drops `backdrop-filter`;
- every value is final: no count-up, no reveal, no clip wipe, no ladder
  draw-in;
- the page keeps its layout, its facts and its evidence marks.

The hook is the contract: MS1–MS3 components read `lite()` instead of testing a
media query inline, so enabling the mode is a one-line change and the
inventory's lite column is already populated. The mode is specified here and in
`docs/site/skin-plan.md` §7; the switch lands last.

## 7. Motion in scope

The 22 effects, their jobs and their reduced-motion states are the table in
`docs/site/animation-inventory.md`; that table is the authority and this section
does not restate it. Four rules apply to all of them:

- **No SMIL.** The prototype's `animateMotion` elements (the diagram packets,
  the ladder dot, the architecture packets) are replaced by the same motion
  drawn on a canvas the page already owns, or by a static final state. The
  checker fails on `<animateMotion` in `site/src`.
- **No animation may gate content.** Server HTML is complete; a hidden start
  state is armed only after the reduced-motion and lite checks.
- **A scroll handler writes only transforms.** The page index and progress bar
  are the only scroll-driven writes; the hero's particle field reads scroll
  velocity from a value the same frame computed.
- **Every effect has a final state.** Under reduced motion the effect shows its
  last frame, not a blank one.

## 8. How a reviewer checks a screen

1. Run `python3 scripts/site-check.py` and read the four sections.
2. Cover the copy: does the page still say one thing per section (rules 1, 4)?
3. Search the stylesheet: `background-clip`, `backdrop-filter` (rules 2, 3, 13).
4. Count canvases, then scroll every one out of view and confirm its loop
   stopped (rules 5, 14).
5. Set `prefers-reduced-motion: reduce`, reload, and confirm every figure,
   bar, ladder and diagram is in its final state (rule 8).
6. Set `?lite=1`, reload, and confirm the atmosphere is gone and the facts
   remain (rule 14, §6).
7. Read every figure and find its fact id, chip and baseline (rules 9, 10).
8. Read one glass panel over the cool orb and confirm the scrim is painted
   (rules 11, 12).

## 9. What is checked mechanically

| Check | Where |
| --- | --- |
| Token table matches `docs/site/brand.md`; shadows verbatim; 20 colour tokens | `scripts/site-check.py docs`, run by `cargo xtask site-check` |
| 36 used contrast pairs, including glass worst case and the boundary | `scripts/site-check.py contrast` |
| Banned words, unbacked numbers, unresolved fact ids, missing chips, scope sentence | `scripts/site-check.py copy` |
| 22 inventory rows, every keyframe named, every selector present in the prototype, reduced-motion state, parity assertion | `scripts/site-check.py docs` |
| No gradient text, no SMIL, blur allowlist, canvas count, CSP settings | `scripts/site-check.py docs` |
| No typed benchmark number in `site/src` | `site/src/lib/guard.ts`, run by `cargo xtask site-check` |
| Fact-gap register agrees with the checker's registry | `scripts/site-check.py docs` |

The rest of the checklist is a review gate recorded here so a later session
applies the same standard.
