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

## Commit graph card

`cargo xtask commit-graph` reads the **public** commit history and writes three
artifacts into `docs/media/`:

- `commit-graph.json` — the machine-readable document: totals, one row per day
  (including days with no commits), one row per author (display names only,
  never e-mail addresses), and the milestone list.
- `commit-graph.svg` — the wide card (840 by 520, shown at 840 px): the
  cumulative staircase, calendar heatmap, four KPI tiles, contributors and the
  milestone roadmap.
- `commit-graph-narrow.svg` — the phone card (400 by 640, shown at 390 px): a
  two-by-two KPI grid, the staircase, the last ten weeks of the heatmap,
  one-column milestones and contributors. The README selects it with
  `<source media="(max-width: 600px)">` and keeps the wide card as the `<img>`
  fallback. No viewer ever shows both cards.

The rules live in the `[commit_graph]` table of `docs/readme-config.toml`: the
timezone (fixed offset only), the public `source_url`, `source_ref` and
`source_cache`, the `repo_web_url` used for the README permalink, hidden author
names, the refresh-commit pattern, the milestones and the size budget.

The committed card must describe the public repository, never a private
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
(“of which N graph refreshes” in the card footer); active days, streaks and
per-day counts include them too. Nothing is hidden.

Dates are bucketed in the configured zone from each commit's author timestamp,
and the document is anchored to one commit: `as_of_sha`, with `as_of_date` taken
from that commit rather than from the wall clock. Generation is deterministic —
the same history and configuration produce the same bytes — and it refuses to
run in a shallow clone, where the history is incomplete.

Every label is drawn as vector outlines from an embedded font, and both cards
carry their text as `<title>`/`<desc>` for assistive technology. There are no
scripts, no external requests and no web fonts. `cargo xtask docs-check`
reproduces the JSON byte for byte from this checkout when it contains the
as-of commit, otherwise from the public cache, and skips the history checks with
a printed reason when neither does; it also checks the `<desc>` numbers, the
size budget, and the README's link to the public commit. It never fetches, so a
public clone passes with no network; a graph that is merely behind `HEAD`
produces a warning, not a failure.

### Readability contract

Both cards are tested, not eyeballed. Each text group carries `data-size`,
`data-name` and `data-box` attributes, and every run inside it carries its own
`data-box`; the tests parse the committed cards and compute:

- the effective size at the README display width — at least 12 px for every
  group, at least 13 px for body labels (stamp, KPI labels, milestones,
  contributors), at least 20 px for the title and at least 32 px for KPI
  numbers;
- WCAG contrast of each run's ink against the surface underneath it — at least
  4.5:1 (the faint `#6E7681` ink was removed precisely because it measured
  3.8:1 on the panel);
- at most 14 text groups per card and at least 8 px between any two groups;
- no text run overlapping a data mark — a heatmap cell, a contributor bar, a
  milestone dot, the endpoint dot or a segment of the staircase.

The tests also prove determinism and that both `<desc>` elements carry the
same totals as the JSON document. `cargo xtask docs-check` regenerates both
cards and reports drift; the fixed `rsvg-convert` rasters under `target/` are a
local visual check, not a substitute for the contract.

## Refreshing the graph

1. Commit the work locally.
2. Run `tools/sync-aiori.sh` to publish it to the public repository.
3. Run `scripts/refresh-commit-graph.sh`. It fetches the public history first,
   prints the public SHA and the commit count at that SHA, regenerates the card
   (so it includes the commits just synced) and stages only the three media
   artifacts plus the README permalink the generator rewrites.
4. Read the `git diff --stat --cached` it prints, then commit with
   `docs: refresh commit graph` — the script never commits or pushes.
5. The next `tools/sync-aiori.sh` publishes the refreshed card.
6. To check without writing, run `cargo xtask commit-graph --check`: it fails
   only when the graph is inconsistent, warns (exit zero) when it is merely
   behind `HEAD`, and skips the history checks with a reason when the as-of
   commit is in neither this checkout nor the cache.
7. To warn automatically, run `scripts/install-hooks.sh` once; it prints exactly
   what it installs and adds a pre-push hook that warns on staleness without
   blocking a push and never fetches.
8. The README's `<!-- commit-graph:begin -->` block points at the public commit
   the card was generated from; its caption never needs a hand edit, because the
   generator rewrites the permalink, not the prose.

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
