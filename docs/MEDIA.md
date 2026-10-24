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
  version 4 adds `pending_refresh` and `pending_date`, which record the refresh
  commit the totals already count; version 3 added the first and last commit
  dates, the busiest day, the longest streak's range and the milestone status.
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
label), the size budgets and the staleness thresholds. `self_count` (default
true) makes the totals include the refresh commit that will carry the document;
`refresh_commits_allow_readme` (default true) lets that commit carry the README
permalink beside the graph artifacts, while a README-only commit is never
classified as a refresh. The `[commit_graph.style]` table carries the
presentation knobs: `accent`, `title`, `show_flags`, `show_bars` and
`display_font`.

### Provenance

The committed graph must describe the public repository, never a private
development checkout whose history has been rewritten. The command therefore
reads history from a bare cache of `source_url` at `source_ref` — cloned on
first use, fetched afterwards, without credentials — unless the checkout's own
`origin` already is that public repository. `--source public` forces the cache,
and `--source local` reads this checkout for offline inspection; a graph built
with `--source local` must not be committed, because it may describe a private
log whose commit ids do not exist publicly.

`as_of_sha` is the public tip the card was generated from, and it stays a real,
linkable commit. When `self_count` is enabled (the default), the totals also
count the refresh commit that will carry the document. Before that commit
exists `totals.commits` equals `git rev-list --count <as_of_sha> + 1`; once it
lands it equals the number the public repository shows. The document records
`pending_refresh: true` and `pending_date` in the configured zone, and the
documentation check replays that recorded day instead of reading the wall
clock, so the bytes reproduce later. Media-only graph refreshes are included in
that total and reported separately as `totals.refresh_commits` (“includes N
graph refreshes” in the footer); active days, streaks and per-day counts
include them too. Nothing is hidden.

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

## Publishing the card

`scripts/publish.sh` publishes the branch, refreshes the card when needed, and
proves the card and the public repository agree. It prints every step and
appends the whole run to `target/publish.log`:

1. Push the private commits. The sync script clones the private *remote*, not
   the working tree, so an unpushed commit would be invisible to it.
2. Run `tools/sync-aiori.sh` to publish a cleaned copy. If its gate refuses,
   the publish stops before anything is committed.
3. Run `cargo xtask commit-graph --auto --source public`. The command compares
   the committed document with the public tip. When the tip is already the
   refresh commit the document counted — its first parent is `as_of_sha` — it
   prints `up to date`, touches nothing and exits 0. Otherwise it regenerates
   from the tip, stages the five artifacts and `README.md`, prints the old and
   new counts, and exits 10.
4. On exit 10, commit the generated files as `docs: refresh commit graph`,
   push, and sync again. Then run `--auto` again and require `up to date`. This
   second pass is the convergence proof: the refresh commit the card counted
   now exists, so there is nothing left to stage.
5. Compare `totals.commits` in the JSON with `git rev-list --count` on the
   cached public branch, print the two numbers side by side, and fail if they
   differ.

The script takes a lock (`target/publish.lock`), refuses a dirty working tree,
requires an upstream branch, never force-pushes, never rewrites history and
never edits the sync script. Its only commit is the generated refresh.

### Failure modes

- **Dirty tree** — refused before anything is pushed. Commit or stash first.
- **No upstream** — refused: the sync reads the private remote, not this tree.
- **Another publish running** — the lock makes the second invocation exit
  immediately instead of interleaving pushes.
- **Sync gate failure** — the publish stops; nothing is committed or pushed
  after that point.
- **Generator failure** (an exit status other than 0 or 10) — the publish stops
  and prints the generator's output.
- **No convergence** — the second `--auto` did not print `up to date`, so the
  public tip is not the refresh the document counted. Inspect the public branch
  before retrying rather than editing the card by hand.
- **Card and repository disagree** — step 5 prints both counts and fails.
- **Missing history** — a clone without the public as-of commit and without the
  cache skips the exactness comparison with a printed reason. Publishing still
  needs the cache, because the card must describe public history.

`cargo xtask commit-graph --check` remains the offline checker. It reproduces
the document and, in a public clone whose `HEAD` is the refresh commit,
requires `totals.commits == git rev-list --count HEAD`. In a private checkout
it finds the unique refresh child of `as_of_sha` in the cache and compares
against that child's count, or skips with a printed reason when the cache does
not hold it. A present but unparseable artifact is a hard failure that names
the path; an absent cache is not.

### Hooks

`scripts/install-hooks.sh` installs the pre-push consistency check.
`scripts/install-hooks.sh --autopublish` also installs a post-commit hook that
runs `scripts/publish.sh` after every commit. It is off by default and carries
a recursion guard, because the refresh commit would otherwise start another
publish. A hook failure cannot undo a commit, so it prints a warning and
appends the full output to `target/autopublish.log`.
`scripts/install-hooks.sh --remove` deletes the hooks this script installed and
leaves any other hook untouched.

### The honest side effect

One publish with new work adds one small refresh commit to the public history.
That commit is counted in the card, which is why `--auto` needs the second pass
to converge, and it appears as a graph refresh rather than being hidden.
Publishing after every commit therefore adds a refresh commit to the public
history every time. Publishing in batches is cheaper: collect several commits,
publish once, and the card gains one row.

### Variants not built here

**CI variant (described, not built).** Five trade-offs:

- It runs the same sync → `--auto` → commit → sync → `--auto` sequence on a
  runner, so no developer machine needs repository credentials.
- It needs a full clone or the bare cache for the exactness check, plus a
  concurrency group: two runs must never publish at once.
- A failed gate is a red build with the previous public tip intact, which is
  easier to audit than a half-finished laptop publish.
- A run with no new work adds no refresh commit, because `--auto` exits 0 and
  the publish does nothing further.
- Its token needs push rights to both repositories, a larger scope than a
  read-only dashboard job.

**Live Cloudflare Worker variant (described, not built).** Five trade-offs:

- It serves the counts on request, so the card is never stale, at the cost of a
  runtime dependency and GitHub API rate limits.
- It must label every number with the commit it was computed from, or a reader
  cannot tell which history the count describes.
- It needs its own cache and authentication; a failed fetch must show the last
  good commit rather than a blank card.
- It duplicates the counting rules unless both paths read the same committed
  JSON, so drift is the main risk.
- It removes the refresh commit entirely, which also removes the public history
  row that shows the card was regenerated; the trade is freshness for a smaller
  audit trail.

## TUI tour GIF

The README references `docs/media/climb-tour.gif` as a placeholder for a TUI tour recording. This file does not exist yet.

### Recording with asciinema and agg

1. **Record the tour** (terminal session):
   ```bash
   asciinema rec docs/media/climb-tour.cast
   ```
   In the recording session, run:
   ```bash
   cargo run -p climb-tui -- --tour
   ```
   Let the tour complete, then exit asciinema (`Ctrl+D` or `exit`).

2. **Convert to GIF with agg**:
   ```bash
   agg docs/media/climb-tour.cast docs/media/climb-tour.gif
   ```
   Adjust speed if needed: `agg --speed 1.2 docs/media/climb-tour.cast docs/media/climb-tour.gif`

### Notes
- Use a clean terminal with appropriate size (e.g., 120x40) for readability
- The tour uses recorded sample data from `docs/sample-output/` so no live benchmark needed
- Ensure the GIF is reasonably sized for README display

## Honest limits

- **Exact after convergence, not before.** While the refresh commit is pending
  the card is one commit ahead of `as_of_sha`; once published it equals the
  public tip. `--auto` reports that difference instead of guessing, and a card
  that is merely behind `HEAD` is a warning, never a failure.
- **Activity is not quality.** Commit counts, streaks and lines changed measure
  how the repository was worked on, not how good the work is; a busy week and a
  slow week look different in the picture and say nothing about either.
- **Public history only.** The graph is a pure function of the public commits
  reachable from `as_of_sha`, the recorded pending day and the `[commit_graph]`
  rules; it carries no wall-clock date, no e-mail address and no claim that any
  milestone was met on time. An offline `--source local` graph describes the
  local checkout instead and must not be committed.
