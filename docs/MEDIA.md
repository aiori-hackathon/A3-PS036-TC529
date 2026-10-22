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

`cargo xtask commit-graph` reads the repository's own history and writes three
artifacts into `docs/media/`:

- `commit-graph.json` — the machine-readable document: totals, one row per day
  (including days with no commits), one row per author (display names only,
  never e-mail addresses), and the milestone list.
- `commit-graph.svg` — the full card: cumulative staircase, calendar heatmap,
  headline figures, contributors and the roadmap.
- `commit-graph-compact.svg` — the same data in a short banner, for the top of
  the README.

The rules live in the `[commit_graph]` table of `docs/readme-config.toml`: the
timezone (fixed offset only), an optional start date, excluded authors, display
names, the path-only commit filter, the milestones and the size budget.

Dates are bucketed in the configured zone from each commit's author timestamp,
and the document is anchored to one commit: `as_of_sha`, with `as_of_date` taken
from that commit rather than from the wall clock. Generation is deterministic —
the same history and configuration produce the same bytes — and it refuses to
run in a shallow clone, where the history is incomplete.

Every label is drawn as vector outlines from an embedded font, and both cards
carry their text as `<title>`/`<desc>` for assistive technology. There are no
scripts, no external requests and no web fonts. `cargo xtask docs-check`
regenerates the JSON from its own `as_of_sha`, compares it byte for byte,
checks the `<desc>` numbers against the document, and enforces the size budget;
a graph that is merely behind `HEAD` produces a warning, not a failure.

## Refreshing the graph

1. Run `scripts/refresh-commit-graph.sh` after any change worth recording; it
   runs the generator and stages only `docs/media/commit-graph.*`.
2. Read the `git diff --stat --cached` it prints, then commit with
   `docs: refresh commit graph` — the script never commits or pushes.
3. To check without writing, run `cargo xtask commit-graph --check`: it fails
   only when the graph is inconsistent, and warns (exit zero) when it is merely
   behind `HEAD`.
4. To warn automatically, run `scripts/install-hooks.sh` once; it prints exactly
   what it installs and adds a pre-push hook that warns on staleness without
   blocking a push.
5. The README's `<!-- commit-graph:begin -->` block points at the latest card;
   it never needs a hand edit, because the card is regenerated, not rewritten.

## Honest limits

- **Stale by design.** The card is anchored to one commit (`as_of_sha`), so the
  commit that lands it always leaves it one commit behind. Staleness is a
  warning past the configured thresholds, never a failure.
- **Activity is not quality.** Commit counts, streaks and lines changed measure
  how the repository was worked on, not how good the work is; a busy week and a
  slow week look different in the picture and say nothing about either.
- **Generated only from this history.** The graph is a pure function of the
  reachable commits and the `[commit_graph]` rules; it carries no wall-clock
  date, no e-mail address and no claim that any milestone was met on time.
