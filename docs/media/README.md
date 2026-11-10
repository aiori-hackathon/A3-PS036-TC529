# Media assets

Everything here is a build output: re-run the owning script instead of
editing any raster by hand. The logo set in `logo/` is the team's own
design; `source/` keeps the designer's originals beside the Figma export
they were measured against.

## The logo

The wordmark set is built from the team's design. The designer's vector,
`source/CLIMB_logo.svg`, and the Figma export beside it are the original
artwork; the two clean vectors in `logo/` carry its exact geometry and inks
with the renderer-hostile parts removed. The drop shadow under the C is
baked into a plain offset shape at the measured strength, the blend modes
are gone (measured inert on a transparent backdrop), and no filter, no
embedded image and no external reference remains, so every conforming
renderer draws the same picture. `climb-logo-light.svg` is the
light-surface wordmark; `climb-logo-dark.svg` is the dark-surface wordmark,
its black letterforms recoloured to the off-white ink. The raster
derivatives are rendered by resvg and the social cards are composited with
cairo; `python3 scripts/logo/build_logo_assets.py` rebuilds them, refreshes
the table below so the documented sizes stay true, and prints the WCAG
contrast table of every letterform against both dark surfaces. Its
`--check` mode re-renders everything and fails if a committed file has
drifted from the vectors.

<!-- logo-media:begin -->
| File | What it is | Size | Dimensions |
|---|---|---|---|
| `climb-logo-light.svg` | The wordmark for light surfaces; geometry and inks of the team design, cleaned of filters, blend modes and external references. | 4,616 B | 1504 × 912 |
| `climb-logo-dark.svg` | The wordmark for dark surfaces: the black letterforms become the off-white ink; the orange, the red tree strokes and the L foot stay. | 4,344 B | 1504 × 912 |
| `climb-logo-light@1x.png` | The light wordmark at native resolution. | 61,471 B | 1504 × 912 |
| `climb-logo-light@2x.png` | The light wordmark at twice native resolution. | 146,565 B | 3008 × 1824 |
| `climb-logo-light@3x.png` | The light wordmark at thrice native resolution. | 270,395 B | 4512 × 2736 |
| `climb-logo-dark@1x.png` | The dark wordmark at native resolution. | 60,462 B | 1504 × 912 |
| `climb-logo-dark@2x.png` | The dark wordmark at twice native resolution. | 144,183 B | 3008 × 1824 |
| `climb-logo-dark@3x.png` | The dark wordmark at thrice native resolution. | 266,995 B | 4512 × 2736 |
| `social-preview.png` | The social preview card: the dark wordmark centred on the brand ink. | 40,728 B | 1280 × 640 |
| `og-image.png` | The Open Graph card: the dark wordmark centred on the brand ink. | 26,699 B | 1200 × 630 |
| `favicon.svg` | The favicon: the M with the tree on a square canvas; the ink strokes and the root dot follow the viewer's colour scheme. | 2,262 B | 287 × 287 |
| `favicon.ico` | The favicon for browsers without SVG support: the 16, 32 and 48 px renditions packed in one ICO. | 4,716 B | 16 × 16, 32 × 32, 48 × 48 |
| `favicon-192.png` | The favicon at web-app icon size, transparent. | 12,108 B | 192 × 192 |
| `favicon-512.png` | The favicon at large web-app icon size, transparent. | 47,411 B | 512 × 512 |
| `apple-touch-icon.png` | The Apple touch icon: the dark-ink mark on the brand ink, opaque. | 10,387 B | 180 × 180 |
<!-- logo-media:end -->

## The logo palette

`brand.toml` holds the designed palette: the brand orange, the ink, the
off-white, the red reserved for authentication paths, the teal and the
slate, each with the contrast decisions that say where it may be used. The
letterform inks and their measured contrast ratios are listed there and
printed by the build script; every letter body clears 4.5:1 against both
dark surfaces.

## The commit graph

`cargo xtask commit-graph` writes five files here from the repository's public
history: `commit-graph.json` (the document), `commit-graph.svg` (the wide card,
840 × 540), `commit-graph-narrow.svg` (the phone card, 400 × 640),
`commit-graph-compact.svg` (the wide strip, 840 × 156) and
`commit-graph-compact-narrow.svg` (the phone strip, 400 × 200). The cards are
rendered from the document and repeat its headline numbers in their `<desc>`
element. The README shows the compact strip under the badges and the full card
under "Built in the open", one variant per viewport. Re-run the command instead
of editing any of the five by hand. `docs/MEDIA.md` describes the
configuration, the layout and the checks that keep the artifacts honest.
