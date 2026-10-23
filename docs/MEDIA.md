# Media and README badges

## README badge block

The badge row in `README.md` is generated from a single source of truth and kept in sync by a drift check.

### Configuration

`docs/readme-config.toml` defines every badge slot (labels, messages, links, alt text, colours, logo slugs and style). The configuration uses `#[serde(deny_unknown_fields)]`; unknown keys are rejected.

Brand colours come from `docs/media/brand.toml` (measured palette):
- `orange` — `#C55E33`
- `slate` — `#38454E`
- `teal` — `#5C8E8D`

### Generation

```bash
cargo xtask readme-badges
```

By default the command:
1. Reads `docs/readme-config.toml` and `docs/sample-output/climb-results.json` (committed, provenance-stamped).
2. Builds six badges in two centred rows inside the markers `<!-- badges:begin -->` and `<!-- badges:end -->` in `README.md`:
   - **Identity row:** AIORI 3 Hackathon, Problem statement PS-036, Track 2 Cyber Security (portal links, slate label/orange message)
   - **Proof row:** Built with Rust (rust-lang.org, slate label/teal message, `logo=rust`), Intro video YouTube (video URL, slate label/teal message, `logo=youtube`), Claims verified (links to `docs/deck-facts.md`, teal label/orange message)
3. Renders each badge as an `<a>` wrapping an `<img>`, using the shields.io static endpoint `https://img.shields.io/static/v1` with query parameters `label`, `message`, `color`, `labelColor`, `style`, `logo`, `logoColor`. Every value is percent-encoded in code. `&` in attribute values is escaped as `&amp;`.
4. Prints the six badge URLs to stdout.

Override inputs:
- `--config <path>` — alternate config file
- `--readme <path>` — alternate README
- `--results <path>` — alternate results artifact (the committed sample is deterministic across clones)

### Claims badge

The `claims verified` message is `<passed>/<total>` computed from the claims registry that the report uses. For each registry claim the generator re-resolves the claim's JSON pointer against the raw results artifact JSON with float tolerance (mirroring report semantics). If the results artifact is missing the message is `unavailable`. The badge links to `docs/deck-facts.md`; the number does not appear as prose in the README outside the managed block.

### Drift check

`cargo xtask docs-check` regenerates the badge block in memory and fails if it differs from the block in `README.md`, printing a clear unified diff. The numeric scan inside the managed badge block is skipped (only that block), so percent-encoded values like claims counts do not trigger documentation number checks. A missing config, missing results artifact, or missing markers are all violations.

In strict mode the generator does not write; a drifted block fails with a non-zero exit.

### Asset tracking

Generated PNG/SVG/brand assets live in `docs/media/`. `docs/media/README.md` documents how they were produced and the palette.

## Commit graph artwork

`cargo xtask commit-graph` reads the **public** commit history and writes five
artifacts into `docs/media/`:

- `commit-graph.json` — the machine-readable document: totals, one row per day
  (including quiet days), one row per author (display names only, never e-mail
  addresses) and the milestone list with a `done` or `upcoming` status. Schema
  version 3 adds the first and last commit dates, the busiest day, the longest
  streak's range and the milestone status.
- `commit-graph.svg` — the wide card (840 by 540, shown at 840 px).
- `commit-graph-narrow.svg` — the phone card (400 by 640, shown at 390 px).
- `commit-graph-compact.svg` — the wide strip (840 by 156).
- `commit-graph-compact-narrow.svg` — the phone strip (400 by 200).

The README shows the compact strip directly under the badges and the full card
under "Built in the open". Each block is a `<picture>` with one image per
viewport: the narrow variant under `(max-width: 600px)` and the wide variant as
the fallback. The generator rewrites the permalink in both blocks to the public
as-of commit, so neither link needs a hand edit.

### Layout

The wide card is an editorial page rather than a dashboard. A masthead carries
an eyebrow line, the title set in Instrument Serif and a two-line stamp with
the commit and its date. Four headline totals sit in columns separated by short
rules, with no boxes around them. The cumulative staircase runs across the
middle of the page: the data region takes a 70/30 share of the width, the
runway the rest, separated by an axis break and labelled "runway not to
scale". Daily bars sit below it, and milestone flags rise from the line; the
flag placer tries several baselines and refuses to render rather than overlap a
label. The footer names the contributors and the public-history source, and
adds the refresh note only when the history contains graph refreshes.

The narrow card keeps the same data in a single column: a two-by-two total
grid, the staircase with numbered milestone dots and a numbered legend, the
last five weeks of daily bars, and a stacked footer.

Both strips are single-row summaries: a large serif numeral for the commit
total, a caption, three supporting statistics, a small sparkline and the stamp.
The phone strip is the same drawing at a smaller size.

The type is Instrument Serif for the title, the large numerals and the compact
total, and JetBrains Mono for every caption, label and small number. Both faces
are embedded as outlines: no font is requested at view time, so the picture
cannot change with the viewer's font set. The palette is `#0E1217` for the
card, `#E6EDF3` for primary ink, `#8B98A5` for secondary ink and
`#5C8E8D`/`#7FB5B3` for the present day, the streak and completed milestones.
Orange (`#C55E33`, sampled in `docs/media/brand.toml`) is reserved for data:
the staircase, the bars and the area under the line. It never paints a glyph.

Motion is CSS-only: the staircase draws in, the bars rise in a short stagger,
the flags fade in after the line, and only the ring around the present day
loops. Every element carries its final geometry in the markup, and
`prefers-reduced-motion: reduce` removes the animation entirely.

### Configuration

The rules live in the `[commit_graph]` table of `docs/readme-config.toml`: the
timezone (fixed offset only), the public `source_url`, `source_ref` and
`source_cache`, the `repo_web_url` used for the README permalink, hidden author
names, the refresh-commit pattern, the milestones (with an optional short
label), the size budgets and the staleness thresholds. The
`[commit_graph.style]` table carries the presentation knobs: `accent`, `title`,
`show_flags`, `show_bars` and `display_font`.

### Provenance

The committed graph must describe the public repository, never a private
development checkout whose history has been rewritten. The command therefore
reads history from a bare cache of `source_url` at `source_ref` — cloned on
first use, fetched afterwards, without credentials — unless the checkout's own
`origin` already is that public repository. `--source public` forces the cache,
and `--source local` reads this checkout for offline inspection; a graph built
with `--source local` must not be committed, because it may describe a private
log whose commit ids do not exist publicly.

`totals.commits` counts **every** commit reachable from the stamped
`as_of_sha`, so it equals `git rev-list --count <as_of_sha>` and therefore the
number the public repository shows at that commit. Media-only graph refreshes
are included in that total and reported separately as `totals.refresh_commits`
(“of which N graph refreshes” in the footer); active days, streaks and per-day
counts include them too. Nothing is hidden.

Dates are bucketed in the configured zone from each commit's author timestamp,
and the document is anchored to one commit: `as_of_sha`, with `as_of_date` taken
from that commit rather than from the wall clock. Generation is deterministic —
the same history and configuration produce the same bytes — and it refuses to
run in a shallow clone, where the history is incomplete.

Every label is drawn as vector outlines from the embedded faces, and every
artifact carries its text summary in `<title>`/`<desc>` for assistive
technology. There are no scripts, no external requests and no web fonts.
`cargo xtask docs-check` reproduces the JSON byte for byte from this checkout
when it contains the as-of commit, otherwise from the public cache, and skips
the history checks with a printed reason when neither does; it also checks each
artifact's size budget, its `<desc>` numbers against the document, the
outline-only text, the absence of external references, and the README's link to
the public commit. It never fetches, so a public clone passes with no network;
a graph that is merely behind `HEAD` produces a warning, not a failure.

### Readability contract

The artwork is tested, not eyeballed. Each text group carries `data-name`,
`data-font`, `data-size`, `fill` and `data-bbox`, and every run inside it
carries its own `data-bbox`. The tests parse the committed artifacts and check:

- the effective size at the display width: at least 11 px for small copy,
  28 px for the KPI numerals, 32 px for the title and 52 px for the compact
  total;
- contrast of every text ink against `#0E1217`: at least 4.5:1 (`#8B98A5`
  measures 6.4:1 and `#7FB5B3` 8.2:1) and at least 12:1 for the primary ink;
- group caps of 16 on the wide card, 12 on the narrow card and 8 per strip;
- no text run overlapping another run, and no run leaving the canvas or its
  margin; the overlap check is proven non-vacuous by a test that reproduces a
  collision;
- no `<filter>`, at most two gradients (one area fill, never on a glyph or a
  stroke), at most two typefaces, no `<text>`, no scripts and no external
  references;
- the 70/30 split, the bar baselines, the milestone statuses, the 8 px
  structural grid and the `<desc>` numbers all match the JSON document;
- byte-identical regeneration, and the size budgets: fifty kilobytes per card
  and twenty-five kilobytes per strip.

## Refreshing the graph

1. Commit the work locally.
2. Run `tools/sync-aiori.sh` to publish it to the public repository.
3. Run `scripts/refresh-commit-graph.sh`. It fetches the public history first,
   prints the public SHA and the commit count at that SHA, regenerates the
   artwork (so it includes the commits just synced) and stages only the five
   media artifacts plus the README permalinks the generator rewrites.
4. Read the `git diff --stat --cached` it prints, then commit with
   `docs: refresh commit graph` — the script never commits or pushes.
5. The next `tools/sync-aiori.sh` publishes the refreshed artwork.
6. To check without writing, run `cargo xtask commit-graph --check`: it fails
   only when the graph is inconsistent, warns (exit zero) when it is merely
   behind `HEAD`, and skips the history checks with a reason when the as-of
   commit is in neither this checkout nor the cache.
7. To warn automatically, run `scripts/install-hooks.sh` once; it prints exactly
   what it installs and adds a pre-push hook that warns on staleness without
   blocking a push and never fetches.
8. The README's `<!-- commit-graph-strip:begin -->` and
   `<!-- commit-graph:begin -->` blocks point at the public commit the artwork
   was generated from; their captions never need a hand edit, because the
   generator rewrites the permalinks, not the prose.

## Honest limits

- **Stale by design.** The card is anchored to one commit (`as_of_sha`), so the
  commit that lands it always leaves it one commit behind. Staleness is a
  warning past the configured thresholds, never a failure.
- **Activity is not quality.** Commit counts, streaks and lines changed measure
  how the repository was worked on, not how good the work is; a busy week and a
  slow week look different in the picture and say nothing about either.
- **Public history only.** The graph is a pure function of the public commits
  reachable from `as_of_sha` and the `[commit_graph]` rules; it carries no
  wall-clock date, no e-mail address and no claim that any milestone was met on
  time. An offline `--source local` graph describes the local checkout instead
  and must not be committed.
