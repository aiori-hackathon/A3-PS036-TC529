# Commit-graph regeneration: root-cause analysis

**Date:** 2026-11-17
**Status:** cause proven; no source or test file changed in this investigation.
**Scope:** `cargo xtask commit-graph` (the writing form). `cargo xtask commit-graph --check`
and `cargo xtask docs-check` are green and are not the failing path.

## 1. Symptom

Run in a clean worktree of `HEAD` (`git worktree add ~/tmp/cg-scratch HEAD`), with
`RUST_BACKTRACE=1`:

```
$ cargo xtask commit-graph
Error: commit-graph milestone label `Reg - 18 Nov` cannot be placed directly above its pole; shorten its `short` label or widen the card
Stack backtrace:
...
EXIT=1
```

`cargo xtask commit-graph --auto --source public` fails with the same line and also exits 1;
it stages nothing. `scripts/refresh-commit-graph.sh` would stop there (it runs under
`set -euo pipefail`); `scripts/publish.sh` reports
`error: the generator failed (exit 1)` from its third step.

Nothing is written and nothing is staged: the four cards are rendered before any file is
written (`xtask/src/commit_graph/mod.rs:1417-1454`), and `git status --short` in the scratch
tree stays empty after the failure.

## 2. What the command was asked to draw

The committed document (`docs/media/commit-graph.json`) describes the public history at
`831faec567cbec90ba1bc3b93d21e7f538eef919` (2026-11-13): `totals.commits` 137 (136 reachable
plus the pending refresh it counts), `refresh_commits` 8, `pending_refresh` true,
`schema_version` 4, `config_hash` `5ee7513…a5799`. The public tip is now
`3d5f057b54d9a794615ed0d7ec7882737de28b0b` with 146 commits, so the document is stale — but
staleness is not the failure (see §5).

## 3. Reproduction

All commands below were run in the scratch worktree with the pinned native prefix exported
(`CLIMB_MTL_PREFIX` pointing at the checkout's `.mtl-install`), which the worktree does not
carry itself.

```
$ cargo xtask commit-graph --check
note: commit graph: history checks skipped because as_of_sha 831faec is not in this checkout
and the cache ./target/commit-graph-source.git does not contain it; run
`cargo xtask commit-graph --source public` to populate the cache. An offline checkout must
still pass, so this is not a failure.
commit-graph --check: the committed graph is consistent with its as-of commit
EXIT=0

$ cargo xtask commit-graph --source public --check      # --check never fetches
commit-graph --check: the committed graph is consistent with its as-of commit
EXIT=0

$ cargo xtask docs-check
...
docs-check: 16 documentation file(s) clean against ./docs/deck-facts.md (529 facts)
EXIT=0

$ cargo test -p xtask commit_graph -- the_committed_cards_pass_the_documentation_check \
    the_committed_document_reproduces_from_the_history_that_contains_it
test result: ok. 113 passed; 0 failed; 0 ignored
EXIT=0
```

The first writing-form run in the same worktree clones the credential-free bare cache
(`target/commit-graph-source.git`, tip `3d5f057`, 146 commits) and then fails with the error
in §1.

## 4. Proven cause

**Class: BUG** in the wide card's future-milestone label placement
(`xtask/src/commit_graph/svg.rs`).

When the chart's last day comes within a few days of a milestone that is still in the
future, the milestone's flag is drawn in the runway to the right of the plot. Its label is
always centred on the pole and the placement search only varies the baseline, never `x`
(`svg.rs:1780-1783`). The pole is placed 24 px right of `data_right`
(`svg.rs:1633-1636`), so a label wider than about 48 px reaches back over the plot. There it
crosses the filled staircase's right-hand edge, which is a registered non-text mark spanning
the whole plot height (`svg.rs:1320-1330`, `mark.intersects` at `svg.rs:1802`). Every
candidate baseline still crosses that vertical edge, so the search ends in
`svg.rs:1709-1713`.

The rejection is exact, not inferred. Instrumenting `try_place_flag_label` in the scratch
worktree and generating at the public tip prints one reason for every attempt — 84 in total:
the two preferred levels and the full 82-baseline fallback search.

```
DEBUG flag="Reg - 18 Nov" ... pole_top=362 base_x=622.2 baseline=354 box=(576.609,344.38,668.298,356.34) margin=84 right=808
DEBUG   rejected: mark kind=fill-edge owner=None gap=0
...
84 rejections, every one: mark kind=fill-edge owner=None gap=0

DEBUG geom left=96 top=208 data_right=592 right=808 bottom=384 days=53 column=9.358...
DEBUG fill-edge segment (592,211.52)->(592,384)
```

The label's advance width is 93.6 px at 13 px mono, so it spans `575.4…669.0`; its left
16.6 px lie over `data_right = 592`, and the fill edge at `x = 592` runs from `y = 211.52` to
`y = 384`, i.e. through every baseline in the legal band. The pole would have to sit at
`data_right + 8 + width/2 = 646.8` for the centred label to clear the edge; it sits at 622.2.

The same input at an earlier range end renders fine. With the chart ending 2026-11-13 the
pole is at `643.35`, the label's left edge at `596.55` (4.55 px clear of the edge), and the
second staggered baseline is accepted. The runway maps a future milestone by
`(date - end) / (last_future_date - end)` (`svg.rs:1627-1641`), so as the end approaches the
milestone the pole slides towards `data_right + 24`.

Sweeping the public history with the pending refresh disabled (so `range.end` equals the
as-of day) shows the boundary:

| public as-of | chart end | writing form |
|---|---|---|
| `831faec` | 2026-11-13 | exit 0 |
| `84b01a9` | 2026-11-15 | exit 1, same message |
| `4433bc9` | 2026-11-16 | exit 1, same message |
| `3d5f057` (tip) | 2026-11-17 | exit 1, same message |

**Why it happens today even for an old as-of.** The generator counts the refresh commit that
will carry the document, and that commit is dated with the wall clock
(`Pending::Today`, `mod.rs:1147`) and extends the plotted range
(`end_day = pending_day.map_or(as_of_day, |day| day.max(as_of_day))`, `mod.rs:1209`). Run at
the same as-of, the two configurations differ only in that rule:

```
as_of 831faec, self_count = true : DEBUG range=2026-09-26..2026-11-17 as_of=2026-11-13
                                   pending=Some("2026-11-17") -> Error: ... cannot be placed
as_of 831faec, self_count = false: DEBUG range=2026-09-26..2026-11-13 as_of=2026-11-13
                                   pending=None -> EXIT=0
```

So the chart's last day is 2026-11-17 either way: from the pending day when the document is
stale, or from the tip when it is fresh. On 2026-11-15 and 2026-11-16 the pending day alone
put the end inside the failing window, which is why the committed card has been stuck at the
13 November state since then: regeneration has been impossible, not merely neglected.

Only `commit-graph.svg` (the 840×520 wide card) draws text labels for flags; the phone card
uses numbered dots and the two strips have no chart (`svg.rs:879-1029`). The failure aborts
the whole write before any artifact is produced.

### 4.1 Messages on this path

| Message the human sees | Printed at | What it means | Class |
|---|---|---|---|
| ``Error: commit-graph milestone label `Reg - 18 Nov` cannot be placed directly above its pole; shorten its `short` label or widen the card`` | `svg.rs:1709-1713` | No candidate slot above an upcoming flag's pole clears every data mark; generation aborts. | **BUG** |
| `Error: this checkout is a shallow clone, so its history is incomplete; …` | `mod.rs:596-603` | The chosen history is shallow; regeneration refuses. | USAGE (by design) |
| `note: commit graph: history-dependent checks skipped because this is a shallow clone; …` | `check.rs:101-106` | `--check` cannot verify history here and says so. | STALE-BY-DESIGN |
| `note: commit graph: history checks skipped because as_of_sha … is not in this checkout and the cache … does not contain it; …` | `check.rs:267-273` | Offline checkout without the as-of commit; explicitly not a failure. | ENVIRONMENT |
| `note: commit graph: reproduction checked against the public-history cache …; exactness is not checked because the cache holds no refresh commit whose first parent is …` | `check.rs:253-265` | `--check` reproduced the JSON from the cache; no refresh child to cross-check. | BY DESIGN |
| `commit-graph --check: the committed graph is consistent with its as-of commit` | `mod.rs:1708-1712` | No violations; a stale graph appends a warning to this line. | BY DESIGN |
| `… config_hash does not match the current [commit_graph] configuration` | `check.rs:71-74` | The document was produced by different rules. | STALE (config drift) |
| `… the document does not reproduce byte for byte from its as_of_sha …` | `check.rs:176-180` | A hand edit, or a generator change that altered output. | BUG |
| `… totals.commits is X but HEAD is Y commits …` | `check.rs:189-193` | The document does not count its own refresh commit. | BUG |
| `… <desc> says commits=N but the JSON says M` | `check.rs:460-463` | A card disagrees with the document. | BUG |
| `Error: commit-graph artifacts exceed the self-imposed size budget…` | `mod.rs:1428-1440` | A card or strip is over budget; nothing is written. | BUDGET |
| ``Error: commit-graph streak label `…` has no collision-free place over its bracket`` | `svg.rs:1512` | The other annotation placement bail; same failure family as this one. | BUG |
| `Error: cannot read the public history from …` / `cannot refresh the public history from …` | `mod.rs:779-790` / `746-757` | The cache clone or fetch failed (network, credentials, rewriting). | ENVIRONMENT |
| `Error: … exists but is not a bare git repository; remove it …` | `mod.rs:764-769` | A stale non-bare path occupies the cache location. | ENVIRONMENT |
| `Error: the graph start date (…) is after the graph end date (…)` | `mod.rs:1209-1216` | No commits fall inside the range. | USAGE |
| `Error: --auto chooses its own as-of commit (the public tip); omit --as-of` | `mod.rs:1615` | Conflicting flags. | USAGE |
| `Error: --auto reads the public history so the card can be published; …` | `mod.rs:1620-1625` | `--auto` with `--source local`. | USAGE |
| `commit-graph --auto: up to date; …` / `changed …` and exit 10 | `mod.rs:1645`, `mod.rs:1659`, `mod.rs:39`/`1679` | Nothing to do, or the refresh was generated and staged. | BY DESIGN |
| `Error: commit_graph.source_url must name the public repository` and the neighbouring refusals | `mod.rs:437-450` | An invalid `[commit_graph]` table. | USAGE |

## 5. Hypotheses, ruled out in the requested order

- **H1 compile error in xtask** — ruled out: `cargo check -p xtask` exits 0
  (`Finished dev profile ... in 3.13s`).
- **H2 shallow clone** — ruled out: `git rev-parse --is-shallow-repository` is `false` in the
  main checkout and in the scratch worktree. A genuinely shallow clone produces a different,
  explicit refusal (`mod.rs:596-603`), reproduced in a `--depth 1` clone of the public
  repository.
- **H3 cache clone or fetch fails** — ruled out: the scratch worktree cloned the bare cache
  on first use (tip `3d5f057`, equal to `git ls-remote` of `source_url`); `git ls-remote`
  succeeds both with the normal global config and with `GIT_CONFIG_GLOBAL=/dev/null`;
  `git config --get-regexp 'url\..*\.insteadof'` returns nothing and the only credential
  helper is `gh` for `github.com`. No `cannot read/refresh the public history` message
  appeared; the failure happens after the fetch, in SVG rendering.
- **H4 config hash mismatch after a config change** — ruled out: printing
  `config_hash(&config)` in the scratch build gives
  `5ee751328f75e061609463c7926565f2eb5301db2c5302864b75cd05aaba5799`, byte-identical to the
  committed document, and `docs-check` reports no `config_hash does not match` violation (that
  message, `check.rs:71-74`, is not the one seen).
- **H5 stale or unreachable as-of / public history longer than the document** — real but not
  the cause: the document is stale (137 commits at `831faec`; the tip has 146), and
  `831faec` is not reachable from this private checkout at all, so `--check` can only
  reproduce from the cache. The writing form fails identically at the fresh public tip, so
  staleness explains why the command was run, not why it fails. Staleness itself is a warning
  by design.
- **H6 a dirty tree makes the refresh refuse** — ruled out: `git status --short` was empty in
  the scratch worktree before, during and after the failing run; a dirty-tree refusal lives
  only in `scripts/publish.sh:71-76`, and it prints a different message.
- **H7 a script treats exit 10 as failure** — ruled out: `scripts/publish.sh:150-151` captures
  `|| auto_status=$?` and branches on 0 and 10 explicitly; `scripts/install-hooks.sh` runs the
  read-only check; `scripts/refresh-commit-graph.sh` runs the writing form directly. The
  generator exits 1 here and never reaches exit 10 (`mod.rs:39`).
- **H8 the two contract tests fail** — ruled out: both pass in the scratch worktree (113
  commit-graph tests, 0 failed). They cannot catch this: one re-generates only the document
  with the document's own `pending_date`, the other parses the committed SVG bytes.
- **H9 TZ or LANG changes the output** — ruled out: generating the five artifacts twice with
  `TZ=UTC LANG=C.UTF-8 LC_ALL=C.UTF-8` and with `TZ=Asia/Kolkata LANG=en_US.UTF-8` yields
  byte-identical files (JSON `00cefc08…`, wide card `77cf1e47…`, narrow `b9596340…`, strips
  `55112f11…` and `3c0a2cf9…`). The pending-refresh rule was disabled for this pair because no
  configuration with it enabled can generate on 2026-11-17 at all; the date, not the
  environment, is the variable.
- **H10 private checkout only** — ruled out: a full-depth
  `git clone --no-local https://github.com/aiori-hackathon/A3-PS036-TC529.git` passes
  `--check` and fails the writing form with the identical message; a `--depth 1` clone is
  refused as shallow by design.

## 6. The smallest correct fix

Move the pole, not the label: an upcoming milestone's pole is free to move (the runway is
explicitly not to scale, and the code already pulls it inwards for right-edge overflow,
`svg.rs:1663-1668`). Clamp it into the band in which its centred label cannot reach the plot:

```
safe_left  = geom.data_right + 8.0 + width / 2.0      // today: 592 + 8 + 46.8 = 646.8
safe_right = geom.right - 2.0 - width / 2.0          // today: 808 - 2 - 46.8 = 759.2
base_x     = clamp(runway_x, safe_left, safe_right)
```

With two runway labels this puts the 18 November label at 646.8 and the 17 December label at
747.5; their ink boxes are then only 2.7 px apart horizontally, so they must be stacked
vertically. The second staggered baseline is currently `pole_top - 26` (`svg.rs:1753`), 18 px
below the first, which leaves 6 px between two 12 px boxes — below the documented 8 px.
Raising it to `pole_top - 30` restores it. Both changes together were validated in the scratch
worktree (no repository file was kept):

```
$ cargo xtask commit-graph --source public --as-of 3d5f057 --out-dir target/cg-probe/fix30
EXIT=0
milestone-3 box: 601.2 322.4 91.7 12      (pole x 646.8, fill edge x 592)
milestone-4 box: 690.2 343.4 114.8 10.2   (pole x 747.5)
vertical gap between the two labels: 9.0 px
```

The fix changes two checkable expectations and neither may be weakened silently:
`flag_labels_prefer_the_staggered_levels_then_cover_the_band`
(`xtask/src/commit_graph/svg/tests.rs:1578`) pins `pole_top - 26`, and the next refresh moves
both runway labels on the committed card. Shortening the configured `short` label would make
today's single label fit (`Reg` would have to become roughly seven characters or fewer), but
the 15-character `Finals - 17 Dec` label will hit the same wall when the chart end comes
within about four days of 17 December, so a configuration change is a postponement, not a
fix.

## 7. Regression test that would have caught it

A unit test over a rendered graph, not over committed bytes: build a synthetic repository
whose last commit is one day before an upcoming milestone and whose milestone label is wider
than `2 * 24 px`, call `svg::render_full`, and assert (a) it returns `Ok`, (b) the milestone
group's box does not intersect any `fill-edge` mark, and (c) two upcoming labels keep at
least 8 px between their boxes. Suggested name:
`a_future_milestone_label_never_crosses_the_staircase_edge`. The existing contracts cannot
see the case: `flag_labels_sit_directly_above_their_poles` and
`milestone_labels_keep_eight_px_clearance_from_every_pole` parse the committed (fixed) SVG,
and `the_committed_document_reproduces_from_the_history_that_contains_it` regenerates only the
JSON with the recorded `pending_date`, so it never exercises a wall-clock pending day or the
card renderer.

## 8. Would the native CI job hit this on a clean runner?

**No.** The native job runs `cargo run -p xtask -- docs-check`
(`.github/workflows/ci.yml:159`) after a `fetch-depth: 0` checkout. `docs-check` validates the
committed document against the history that contains its as-of commit by re-generating **the
JSON only** (`check.rs:278-290`), then parses the committed SVGs as XML and compares their
`<desc>` values and shapes (`check.rs:104-150`, `342-424`). It never calls
`svg::render_full`, so the label placement that fails here is not on that path. The job also
never runs the writing form, `--auto`, or the refresh scripts. That is why the native job is
green while a developer cannot regenerate the card, and why the failure went unnoticed for
four days.

## 9. Reproducing this analysis

In a scratch worktree of `HEAD` with `CLIMB_MTL_PREFIX` exported:

```
cargo xtask commit-graph --check                    # exit 0
cargo xtask commit-graph --source public            # exit 1, the error in §1
cargo xtask commit-graph --source public --as-of 3d5f057 --out-dir target/cg-probe/tip
```

To separate the date rule from the history, copy the configuration, set `self_count = false`,
and pass it with `--config`: the same as-of then exits 0 and renders the five artifacts. The
sweep in §4 uses that copy with `--as-of` values from the public cache
(`git --git-dir=target/commit-graph-source.git log --format='%h %ad'`).

## 10. Limits of this analysis

- GitHub's own runners were not executed; the clean-runner conclusion in §8 is read from
  `.github/workflows/ci.yml` and the check code paths, not from a new workflow run.
- The three `--as-of` probes on 2026-11-15 and 2026-11-16 use the pending-refresh rule turned
  off, because the wall clock cannot be moved. The conclusion they support — that the failure
  is a function of `range.end` relative to the milestone date — is independently confirmed by
  the `self_count` pair in §4, which changes only that rule.
- The proposed fix was validated in the scratch worktree by rendering and measuring the
  output; it has not been committed, and no test was added yet.
