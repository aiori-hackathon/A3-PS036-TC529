# Site design rules

The anti-slop checklist for the CLIMB website. Every rule here is a gate, not a
preference: a screen that breaks one is not finished. Each rule states what it
forbids, why, and a **pass test** — something a reviewer can point at, or a
check the build can run, so "looks fine to me" is not the verdict.

The rules assume the tokens in `docs/site/brand.md`, the routes in
`docs/site/ia.md` and the words in `docs/site/copy.md`. A rule that names a
token means the token, never a raw hex.

## 1. The checklist

| # | Rule | Forbidden | Pass test |
| --- | --- | --- | --- |
| 1 | One idea in the hero | A hero that states the product, the feature list and a call to action at once | The hero has one headline, one supporting line and at most two actions. Reading only the headline tells the reader what was measured. |
| 2 | No gradient text | Any `background-clip: text` fill, or text painted with more than one colour | Search the stylesheet for `background-clip` and `text-fill-color`; zero hits. Every glyph is one flat token colour. |
| 3 | No glass cards in a grid | Translucent, blurred "glassmorphism" panels, especially tiled | A panel background is a flat surface token (`bg-1`, `bg-2`, `bg-3`). Search for `backdrop-filter`; zero hits. |
| 4 | No identical three-column feature rows | A row of three equal cards with an icon, a heading and a sentence, repeated down the page | No route repeats the same three-up card block more than once, and no two feature rows share a layout. A section that needs three items uses a table, a list or one wide figure instead. |
| 5 | One deliberate decorative element per page | Competing flourishes: a gradient orb, a glow, a particle field and a grid overlay on the same screen | The page names its single decorative element (for example the ladder motif in the home hero) and every other element carries information. Removing the one element leaves the page working and calmer. |
| 6 | No icon packs | A downloaded icon set, a sprite sheet, or a glyph font | Icons are drawn from the site's own SVG set and use only token colours. A page with no icon is fine; a page with ten different stroke styles is not. |
| 7 | At most two accent colours per screen | Orange, teal, violet, green and blue all fighting on one view | The accents are `accent-orange` and `secondary-teal`. The four evidence colours are status colours, not accents, and are counted separately (they are labelled, patterned and small). |
| 8 | Motion explains, guides or confirms — and can be turned off | Decorative motion, parallax, an entrance animation that delays the content, or any animation with no `prefers-reduced-motion` branch | Every animation has a stated job (show a value change, move focus, confirm a copy) and a reduced-motion state that removes it or reduces it to a non-animated change. |
| 9 | Colour is never the only channel | A status shown by hue alone | Each evidence class ships with a fill pattern and a written chip (`Measured`, `Derived`, `Analytical`, `Extrapolated`), so it survives a monochrome or colour-blind reading. |
| 10 | Numbers carry their evidence | A bare figure, or a figure with no baseline | Every number on the page comes from a fact id and shows its evidence chip and the baseline it is compared against. A number a reader cannot trace is a bug. |

## 2. Why these, in one paragraph

The site's job is to make a small set of measured claims legible and
checkable. The failure mode for a page like this is decoration that competes
with the data: gradient text that makes a figure hard to read, glass panels
that lower contrast, a three-up card row that turns four distinct results into
a template, an icon pack that adds a second visual language. Each rule removes
one way the page can look like a marketing site instead of a record. The
tokens in `docs/site/brand.md` already fix the palette and the type; these
rules fix the composition.

## 3. The one decorative element

The home route has exactly one deliberate decoration: the ladder motif in the
hero — the wordmark set over a ladder of Merkle-tree rungs, drawn in `line` and
`accent-orange`, echoing the Merkle path the page is about. It is inert, it
carries the hero's alt text, and it is the only ornament on the route. Every
other route has none. If a second flourish appears anywhere, rule 5 fails.

## 4. Motion

Two animations are in scope, and no others:

- **The value change.** When the dashboard's dataset switch re-renders a
  figure, the bar or numeral moves to the new value over a short, single
  transition, so the reader sees *what changed*. Reduced motion: the value
  swaps with no transition.
- **The copy confirmation.** The verify and reproduce routes confirm a copied
  command with a short state change on the button. Reduced motion: the label
  changes with no transition.

Both jobs are "explain" or "confirm". No animation runs on load, none is
decorative, and none blocks the reader from content.

## 5. How a reviewer checks a screen

1. Cover the copy: does the page still show one idea per section (rules 1, 4)?
2. Search the stylesheet: `background-clip`, `backdrop-filter` (rules 2, 3).
3. Count accents and count the decorative elements (rules 5, 7).
4. Turn on `prefers-reduced-motion` and re-run every animation (rule 8).
5. Read every figure and find its fact id, chip and baseline (rules 9, 10).

A screen that passes all five passes the checklist. The checks that a test can
run mechanically — the contrast table and the copy scan — live in
`scripts/site-check.py`; the rest are review gates recorded here so a later
session applies the same standard.
