# CI audit — `main`, 18–24 November 2026

Scope: every run of every workflow on `aiori-hackathon/A3-PS036-TC529` whose commit is at or
after `7c1ff01` (18 November 2026, 10:25Z), through the scheduled Security run of 23 November.
Everything below was read from the repository, the GitHub API or the retained Actions logs with
the commands shown. No tracked file other than this report and the progress log was changed.

`gh` was authenticated as `profm0r14rty` with `repo` scope. The run windows are complete: the
oldest run retained for this branch is 25 (13 November), so nothing in scope has aged out.

---

## 1. Runs in the window

`gh run list --repo aiori-hackathon/A3-PS036-TC529 --workflow CI --branch main --limit 30 …`
plus the same list for Site, Security and Deploy.

| Workflow | Run | Commit | Conclusion | Duration | Commit subject |
|---|---|---|---|---|---|
| CI | 50 | `7c1ff01` | success | 21m08s | docs: refresh commit graph |
| CI | 51 | `1cf158a` | **cancelled** | 1m06s | site_data bug fixed |
| CI | 52 | `8dca32b` | **failure** | 12m46s | commit graph refreshed |
| CI | 53 | `2917681` | **cancelled** | 4m46s | Added the live website link in the README.md |
| CI | 54 | `9f9e2a0` | **failure** | 7m33s | docs and commit graph updated |
| Site | 29 | `7c1ff01` | success | 41s | docs: refresh commit graph |
| Site | 30 | `1cf158a` | success | 36s | site_data bug fixed |
| Site | 31 | `8dca32b` | success | 37s | commit graph refreshed |
| Site | 32 | `2917681` | success | 32s | Added the live website link in the README.md |
| Site | 33 | `9f9e2a0` | success | 38s | docs and commit graph updated |
| Security | 13 | `9f9e2a0` | **failure** (schedule, 23 Nov) | 4m29s | — |
| Deploy | 6 | `1cf158a` | skipped | — | site_data bug fixed |

`7c1ff01` is the private commit `d3ac8e4`, the last commit published by `scripts/publish.sh`
itself (`publish: done; the card and the public commit count agree.` in `target/publish.log`).

### 1.1 CI 50 — success

```
viewer (no native libraries)   success   10:25:54Z → 10:31:05Z
native (pinned Docker toolchain) success  10:31:08Z → 10:46:53Z
```

The native job restored the `.mtl-install` cache and passed build, workspace tests, docs-check,
verify-numbers and the audit pack. This is the baseline against which 52 and 54 are read.

### 1.2 CI 51 — cancelled by CI 52

```
viewer: cancelled at step "Test viewer-scope crates" (4m46s)
native: cancelled while queued
```

The cancellation is not a failure. `publish.sh` pushes a base commit and then a graph-refresh
commit; the second push (`8dca32b`, run 52) started at 06:47:32Z, 62 seconds after run 51, and
`ci.yml`'s `cancel-in-progress: true` (`.github/workflows/ci.yml:11-15`) cancelled the first.

### 1.3 CI 52 — failure, class **test**

First failing step: `native (pinned Docker toolchain)` / **Build audit pack**. Verbatim:

```
audit-pack: gate build -> exit 0 in 0.50s
audit-pack: gate fmt -> exit 0 in 0.70s
audit-pack: gate clippy -> exit 0 in 10.13s
audit-pack: gate test -> exit 101 in 78.97s (1683 passed, 1 failed, 3 ignored)
[one audit-pack line omitted: it names a planning file that is not mirrored]
audit-pack: gate docs-check -> exit 0 in 1.89s
audit-pack: gate verify-numbers -> exit 0 in 0.40s
audit-pack: gate source-hygiene -> exit 0 in 0.13s
audit-pack: gate check-pins -> exit 0 in 0.05s
audit-pack: gate viewer build -> exit 0 in 0.25s
audit-pack: gate mermaid lint -> exit 0 in 0.00s
audit-pack: FAIL gate(s): test
wrote ./target/audit-pack.md (132 lines)
##[error]Process completed with exit code 1.
```

The immediately preceding `Test workspace` step on the same runner and the same tree passed
(`cargo test --workspace`, `target/test-logs/workspace.log`: every one of the 54 `test result:`
blocks is `0 failed`). `run_test_gate` (`xtask/src/audit_pack.rs:393-413`) runs
`cargo test --workspace --no-fail-fast` through `proc::run_raw`, which captures stdout and keeps
only the counts, so the name of the failing test is **not in any retained artifact**. It is a
nondeterministic test, not a deterministic gate: the same commit and container passed the same
suite without `--no-fail-fast` minutes earlier. The only loopback/port test in the tree with a
documented prior flake is
`kit::tests::the_serve_script_serves_loopback_only_with_the_deployment_404` (recorded in the
progress log as a one-off `ConnectionReset`), but naming it would be a guess. The proving command
that would name it is:

```
gh run rerun 44474757155 --failed   # is not read-only; not run in this audit
```

or, locally, on a checkout of `8dca32b`:

```
cargo test --workspace --no-fail-fast 2>&1 | tee /tmp/audit-pack-test.log
```

### 1.4 CI 53 — cancelled by CI 54

```
viewer: cancelled at step "Test viewer-scope crates" (4m26s, started 08:32:29Z)
native: cancelled while queued
```

The next push (`9f9e2a0`, run 54) started at 08:36:48Z, 4m23s later, and cancelled it.

### 1.5 CI 54 — failure, class **docs / links**

First failing step: `viewer (no native libraries)` / **Check relative links in README and docs**.
The native job was skipped because `needs: viewer` did not pass. Verbatim:

```
Running `target/debug/xtask link-check`
broken: README.md: climb-pqc.duckdns.org
link-check: 137 target(s): 105 relative, 32 external, 1 broken
Error: link-check: 1 broken relative link(s)
##[error]Process completed with exit code 1.
```

Reproduced locally on the current tree (this report adds one `https://` target, so the total is
one higher than the CI tree that failed):

```
$ cargo run -q -p xtask --no-default-features -- link-check
link-check: 138 target(s): 105 relative, 33 external, 1 broken
broken: README.md: climb-pqc.duckdns.org
Error: link-check: 1 broken relative link(s)
exit=1
```

The same tree is red for the Rust suite as well, because the integration test in
`xtask/src/link_check.rs` scans the committed documents:

```
$ cargo test -q -p xtask the_committed_documents_carry_no_broken_relative_link
link_check::tests::the_committed_documents_carry_no_broken_relative_link --- FAILED
assertion `left == right` failed: broken relative links: ["README.md: climb-pqc.duckdns.org"]
  left: 1
 right: 0
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 663 filtered out
```

That test is not run by the viewer job (it tests only `climb-dns`, `climb-analytical`,
`climb-tui` and `climb-report` — `scripts/ci/test-viewer.sh:9-10`), and the native job that would
run it was skipped. So the link-check shell step was the first place the defect surfaced in CI.

**Consequence: `main` is still red.** The public tip `9f9e2a0` carries the same `README.md` blob
as the local `3a64f98` (`fd83808d6d923d57704af6a10350dad9a3d615b0` on both), and the link is
still present. Any new push that reaches the viewer job will fail at the same step.

### 1.6 Security 13 — failure, class **supply chain** (not code)

Scheduled run on `9f9e2a0`, 23 November. First failing step: **Run cargo audit per deny.toml
ignores**. Verbatim:

```
Scanning Cargo.lock for vulnerabilities (429 crate dependencies)
Crate:    url
Version:  2.5.8
Title:    Stack overflow in `Url::origin()` on nested `blob:` URLs
Date:     2026-11-20
ID:       RUSTSEC-2026-0592
URL:      https://rustsec.org/advisories/RUSTSEC-2026-0592
Solution: Upgrade to >=2.5.9 (patched)
error: 1 vulnerability found!
warning: 3 allowed warnings found
##[error]Process completed with exit code 1.
```

`deny.toml` ignores `RUSTSEC-2023-0071`, `RUSTSEC-2026-0387` and `RUSTSEC-2026-0192` only.
`Cargo.lock` pins `url 2.5.8`. The advisory was published on 20 November, after the last commit,
so no code change caused this failure; it is the weekly scan doing its job. The three warnings
(`lru 0.16.4`, `rustix 1.1.5`, `yoke-derive 0.8.3`) are already allowed.

### 1.7 Deploy 6 — skipped, not a failure

All six Deploy runs in the repository (#1–#6) are `skipped`. The job condition is
`github.repository == 'aiori-hackathon/A3-PS036-TC529' && vars.DEPLOY_HOST != ''`
(`.github/workflows/deploy.yml:38`), and `gh variable list` returns no variables. The Deploy
workflow's gates (Lighthouse, `deploy/tests/release.sh`, `site-check`, live smoke) have therefore
**never run on GitHub**.

---

## 2. Root causes

| # | Run(s) | Class | Root cause | Proving command / file:line |
|---|---|---|---|---|
| RC1 | 54, and the current tip | docs / links | `README.md:68` links the bare host `climb-pqc.duckdns.org`. The checker treats anything without an `http(s)://` scheme as a relative path and stats it (`xtask/src/link_check.rs:52-62`); no such file exists. GitHub renders it as `/…/blob/main/climb-pqc.duckdns.org`, which returns **404**. | `curl -sSL …/blob/main/README.md \| grep climb-pqc` shows an anchor target of `/aiori-hackathon/A3-PS036-TC529/blob/main/climb-pqc.duckdns.org`; `curl -o /dev/null -w '%{http_code}'` on that path prints `404`. The host itself is live: `https://climb-pqc.duckdns.org/` → `200`. |
| RC2 | 53 → 54, 51 → 52 | gate bypass | The last two commits of 19 November were pushed and published **after** the pre-push gate had failed and without `publish.sh`. | `target/publish.log` (mtime 13:58) ends with `ci-local: step [14] … FAILED` and `error: the public-export CI replica failed; nothing was pushed or published.` `git reflog show origin/main` records pushes at 14:00:46 and 14:06:21, after that line. No `--skip-ci` warning exists in the log, no hook is installed (`.git/hooks` holds only `*.sample`; `core.hooksPath` unset) and no `target/autopublish.log` exists. |
| RC3 | structural | gate gap | The pre-push replica runs the **viewer** steps only. It never runs `docs-check`, `verify-numbers`, the native build, the native `cargo test --workspace`, or the audit pack's all-features clippy. The viewer test step also omits `xtask`, `climb-mtl`, `climb-bench` and `climb-web`. | `scripts/ci-local.sh:722` runs the feature matrix after the parsed steps; `scripts/ci/test-viewer.sh:9-10`; `.github/workflows/ci.yml:195-221` (native-only steps). |
| RC4 | 52 | test | One nondeterministic workspace test failed only on the second run of the same binary. Its name was discarded by `proc::run_raw`. | `xtask/src/audit_pack.rs:393-413`; the two runs' counts in §1.3. |
| RC5 | 51, 53 | cancelled-by-concurrency | `ci.yml` cancels an in-progress run on the same ref. Every publish pushes two commits ~30 s apart, so the first run of each pair never completes. | `.github/workflows/ci.yml:11-15`; run 52 started 62 s after 51; run 54 started 4m23s after 53. |
| RC6 | Security 13 | supply chain | A new advisory (`url 2.5.8`, published 2026-11-20) is not in `deny.toml`. | `deny.toml:11-13`; `Cargo.lock` (`name = "url"`, `version = "2.5.8"`). |
| RC7 | Deploy 4–6 | infrastructure / config | The deploy job is disabled by an unset repository variable; the live site is updated only by hand. | `.github/workflows/deploy.yml:38`; `gh variable list` prints nothing; `https://climb-pqc.duckdns.org/` answers 200. |
| RC8 | 54 | gate gap | The Site workflow has no link check (and no Playwright/axe/Lighthouse), so it stayed green on the same commit. | `.github/workflows/site.yml:24-49` (lint, typecheck, build, `npm test` only). |
| RC9 | all | process | `main` has no branch protection or ruleset, so a red push is accepted. | `gh api repos/…/branches/main/protection` → `404`; `gh api repos/…/rulesets` → `[]`. |
| RC10 | local publishes | infrastructure | `publish.sh` pushes over SSH port 22; the log records two `ssh: connect to host github.com port 22: Connection timed out` after otherwise green replicas. | `target/publish.log` (the two `==> 1/6` blocks after `ci-local: all viewer steps passed`). |

---

## 3. Provenance of every public commit since `7c1ff01`

Public and private hashes differ because the publishing sync rewrites all history with
`git filter-repo` (private paths removed, redaction rules applied). The mapping below was
established by subject, author date, file list and parent chain from
`gh api repos/…/commits`, and by `git reflog`/`git show` locally.

`gh api repos/…/commits/7c1ff01` (and the four 19 November commits) returns the same author and
committer identity, `Pratyay Mukherjee`, for every one — **no
commit was made in GitHub's web editor** (that would show GitHub's own noreply identity as the
committer). The sync creates no commits; it only rewrites and pushes.

| Public | UTC | Subject | Private | Origin | Made by | Reached public by |
|---|---|---|---|---|---|---|
| `7c1ff01` | 18 Nov 10:25:07 | docs: refresh commit graph | `d3ac8e4` | publish refresh | `scripts/publish.sh` step 3/4 (fixed message, six generated files) | push 10:25:44Z, then sync; CI 50 |
| `1512076` | 19 Nov 01:21:19 | make the commit graph independent of the date | `e293942` | phase work | hand/phase commit | carried in the batch push of `2384d33` (12:15:52 IST), then sync |
| `6183f2a`, `324bbf3`, `82663a7`, `f11ebd9`, `0ecc08a` | 18 Nov 11:20–19:19 | skin-swap chain | `aa26eb6`, `2bb1bad`, `21065e4`, `4019491`, `a9bad79` | phase work | hand/phase commits | same batch push |
| `66c8f99` | 19 Nov 03:15:23 | Merge branch 'main' into skin-swap | `cfa243b` | phase work | hand merge | same batch push |
| `2cc6750` | 19 Nov 05:01:17 | commit graph refreshed | `4120072` | graph refresh | hand (message is not the one `publish.sh` writes) | same batch push |
| `1cf158a` | 19 Nov 06:16:54 | site_data bug fixed | `2384d33` | hand fix | hand | private push 12:15:52 IST; sync; CI 51 |
| `8dca32b` | 19 Nov 06:46:51 | commit graph refreshed | `59da351` | graph refresh | hand refresh + hand commit | private push 12:17:00 IST; sync; CI 52 |
| `2917681` | 19 Nov 08:22:20 | Added the live website link in the README.md | `f0f4c13` | hand edit | hand | private push 14:00:46 IST; sync; CI 53 |
| `9f9e2a0` | 19 Nov 08:36:12 | docs and commit graph updated | `3a64f98` | hand refresh + README trailing-space fix | hand | private push 14:06:21 IST; sync; CI 54 |

Evidence for the times:

```
$ git reflog show origin/main --date=iso
3a64f98 refs/remotes/origin/main@{2026-11-19 14:06:21 +0530}: update by push
f0f4c13 refs/remotes/origin/main@{2026-11-19 14:00:46 +0530}: update by push
59da351 refs/remotes/origin/main@{2026-11-19 12:17:00 +0530}: update by push
2384d33 refs/remotes/origin/main@{2026-11-19 12:15:52 +0530}: update by push
d3ac8e4 refs/remotes/origin/main@{2026-11-18 15:55:11 +0530}: update by push
```

`git show --stat` for the four 19 November commits: `1cf158a`/`2384d33` one file
(`crates/climb-report/src/site_data.rs`, +2/−1); `8dca32b`/`59da351` README + five graph
artifacts; `2917681`/`f0f4c13` one README line; `9f9e2a0`/`3a64f98` README + five graph
artifacts. The public push actor for `2917681` and `9f9e2a0` is `profm0r14rty`
(`gh api repos/…/events`, PushEvent at 08:32:24Z and 08:36:47Z).

### 3.1 Did `publish.sh` run with or without `--skip-ci`?

**Neither succeeded on 19 November, and no `--skip-ci` invocation is recorded.** The script
appends every run to `target/publish.log` and prints a loud banner for `--skip-ci`
(`scripts/publish.sh:130-135`); no such banner exists. The log contains:

- three failed replicas at step 09, all on `site_data::tests::claims_class_counts_match_the_claims_document`
  (lines 9942, 11073, 12529), before `2384d33` fixed it;
- several green replicas (`feature-matrix: 54 passed, 0 failed, 0 skipped`) followed by
  `ssh: connect to host github.com port 22: Connection timed out` at `==> 1/6 push`;
- a final segment from `f0f4c13` that reached step 14 and failed at the link check; the log's last
  write is 13:58.

The pushes at 14:00:46 and 14:06:21 are after the log's last write, so they did not come from
`publish.sh`. With no hook installed, the only remaining route is a manual private push followed
by a direct run of the sync script. The sync script does no builds and runs no tests; its only
gates are the private-file/redaction rewrite and the leak scan
(`tools/sync-aiori.sh`, outside the repository). That is the gate bypass behind runs 53 and 54.

### 3.2 Have the histories diverged?

Not in content, at the tip:

- local `3a64f98:README.md` and public `9f9e2a0:README.md` are the same blob
  (`fd83808d6d923d57704af6a10350dad9a3d615b0`);
- the public repository has exactly **170** commits (`Link: …&page=170; rel="last"`), and
  `docs/media/commit-graph.json` says `"commits": 170` with `"as_of_sha": "2917681…"`. The card
  counts the refresh commit that carries it, so “as of 2917681, 170 commits” is correct, not drift;
- no private commit exists after `3a64f98` (`git status --short` is empty and `git log -1` is
  `3a64f98`).

Hashes differ by design: the sync rewrites every commit (filter-repo + redaction), so equality is
by content, not by object id. The next `sync-aiori.sh` clones `origin/main` (now `3a64f98`),
rewrites it with the same rules and, because the result is deterministic, fast-forwards the
public branch. If the redaction list or private-path list changes, the rewritten history changes
and the script refuses to force-push unless `FIRST_RUN=1` is set. It would also happily publish a
tree that fails every test, because it has no CI gate of its own.

---

## 4. `climb-pqc.duckdns.org` — every occurrence

`grep -rn climb-pqc.duckdns.org` over tracked files (excluding `target/`, `node_modules/`,
`.git/`):

| Path:line | Written or generated | Source / marker context |
|---|---|---|
| `README.md:68` | **hand-written** | Added by `2917681` (`f0f4c13`), team table, **outside every block marker**. This is the one broken link. |
| `README.md:81,84-87` | **hand-written** | Inside `<!-- live:begin -->` … `<!-- live:end -->`. `docs-check` only requires the markers to exist (`xtask/src/docs_check.rs:684`); it does not regenerate the block. Line 81 is an autolink `<https://…>`, which the link extractor ignores; 84-87 use `https://`. |
| `README.md:30,167` (permalinks) | generated | `cargo xtask commit-graph` from the public history; inside `<!-- commit-graph-strip:begin/end -->` and `<!-- commit-graph:begin/end -->`. |
| `docs/DEPLOYMENT.md:88,304,350` | hand-written | Deployment document; `https://` everywhere. |
| `docs/DEMO-FALLBACKS.md:15,63` | hand-written | Fallback plan; `https://` everywhere. |
| `scripts/smoke-live.sh:5,36` | hand-written | Default `CLIMB_LIVE_URL`, `https://`. |
| `.github/workflows/deploy.yml:46,136` | hand-written | Environment URL and smoke default, `https://`. |
| `deploy/tests/run.sh:357,361,425,504` | test fixture | `CLIMB_SITE_ADDRESS` assertions, `https://`/bare address as expected strings. |
| `dist/kit/docs/DEMO-FALLBACKS.md` | generated | Kit copy of the document; `dist/` is gitignored (`/dist/` in `.gitignore`). |
| private planning log (not published):275 | private log | Not published. |

There is **no occurrence in `docs/readme-config.toml`, `docs/site/`, `site/src/`, site data or
`docs/EVIDENCE-SHEET.html`.** The live URL is not a generated site fact.

### 4.1 How the checker classifies a target

`xtask/src/link_check.rs`:

- `#fragment` and empty targets are ignored;
- targets starting `http://` or `https://` are counted as **external** and never fetched
  (`:53-55`) — the check is offline by design, which is why 32 external targets produce no
  network traffic;
- everything else is joined to the document's directory and must be an existing file
  (`:57-58`); if not, it is **broken** (`:60`).

A scheme-less host is therefore neither external nor a file: it is a broken relative link. The
unit tests cover relative files, fragments and `https://` externals, but **no test covers a
scheme-less host**. GitHub itself resolves it as a relative path: the rendered README carries an
anchor whose target is `/aiori-hackathon/A3-PS036-TC529/blob/main/climb-pqc.duckdns.org`, and
that URL answers 404.

---

## 5. Gate coverage matrix

Legend: **✓** runs there; **partial** = a subset (scope or feature set); **—** does not run;
**n/a** = the workflow cannot run (job disabled).

| Gate (command) | Git hook | `publish.sh` | `ci-local --public` | CI viewer | CI native | Site | Deploy | Security |
|---|---|---|---|---|---|---|---|---|
| `cargo fmt --check` | — | ✓ (replica step 04) | ✓ | ✓ | — | — | — | — |
| clippy, viewer crates, `--all-targets` | — | ✓ | ✓ | ✓ | — | — | — | — |
| clippy, `--all-targets --all-features`, whole workspace | — | partial (matrix: all features × all targets, per crate) | partial (matrix after the steps, with native prefix) | partial (matrix `--check-only`) | ✓ (audit pack) | — | — | — |
| feature matrix, 54 cells | — | ✓ | ✓ | partial (20 pass / 16 skip without native) | — | — | — | — |
| `sample-check` | — | ✓ | ✓ | ✓ | — | — | — | — |
| source hygiene | — | ✓ | ✓ | ✓ | — | — | — | — |
| supply-chain pins (`check-pins`: Docker digests, action SHAs) | — | ✓ | ✓ | ✓ | — | — | — | — |
| Mermaid lint | — | ✓ | ✓ | ✓ | — | — | — | — |
| link-check | — | ✓ | ✓ | ✓ | — | **—** | — | — |
| workflow lint | — | ✓ | ✓ | ✓ | — | — | — | — |
| commit-graph consistency | opt-in pre-push (not installed) | ✓ (steps 3–5) | — | — | — | — | — | — |
| `cargo test --workspace` (all 9 crates + doctests) | — | **—** | **—** (viewer test step runs 4 crates) | partial (dns, analytical, tui, report) | ✓ | — | — | — |
| `docs-check` | — | **—** | **—** | **—** | ✓ | — | — | — |
| `verify-numbers` | — | **—** | **—** | **—** | ✓ | — | — | — |
| audit pack (build, fmt, clippy all-features, test) | — | **—** | **—** | **—** | ✓ | — | — | — |
| native toolchain build | — | **—** | **—** (only `--native` manually) | — | ✓ | — | — | — |
| `cargo audit` + `cargo deny` | — | **—** | **—** | **—** | **—** | — | — | ✓ (on lock/deny changes + weekly) |
| `site-check` + Python selftest | — | — | — | — | — | **—** | ✓ (skipped) | — |
| site lint / typecheck | — | — | — | — | — | ✓ | ✓ (skipped) | — |
| site unit (Vitest) | — | — | — | — | — | ✓ | ✓ (skipped) | — |
| Playwright e2e | — | — | — | — | — | **—** | installs Chromium but does not run `npm run e2e` | — |
| axe (inside Playwright specs) | — | — | — | — | — | **—** | — | — |
| Lighthouse | — | — | — | — | — | — | ✓ (skipped) | — |
| deploy release tests (`deploy/tests/release.sh`) | — | — | — | — | — | — | ✓ (skipped) | — |
| kit `verify.sh` | — | — | — | — | — | — | — | — (manual only) |
| live smoke (`smoke-live.sh`) | — | — | — | — | — | — | ✓ (skipped) | — |

**Gates that exist in CI but not before the push:** the whole-workspace test suite beyond four
crates, `docs-check`, `verify-numbers`, the audit pack, the native build, `cargo audit`/`deny`,
`site-check` (and the Python selftest), site unit/lint/typecheck, Lighthouse, the deploy release
tests, Playwright/axe and kit verification. Playwright and axe run in **no** workflow at all.

### 5.1 `ci-local` versus the viewer job

`ci-local` parses the viewer job's `run:` steps at invocation time and executes exactly 15, in
order: load env, fetch remotes, environment report, format, viewer clippy, feature matrix, fetch
main, sample-check, viewer tests, viewer build, hygiene, pins, Mermaid, link-check, workflow lint.
It does not execute `uses:` steps (checkout, toolchain, caches, uploads); those are reproduced by
`scripts/ci/env.sh` and the pre-created worktree. After the 15 steps it runs the full feature
matrix (`scripts/ci-local.sh:722`).

It **stops at the first failure** (`failed=1; break`, `scripts/ci-local.sh:672-680`) and therefore
does not report later failures. The premise that it stopped at step 09 is true of the three runs
before the `site_data` fix, but **not** of the run that preceded the 14:00 push: that one passed
09 and failed at **step 14** (`target/publish.log`, tail). Either way, step 15 (workflow lint) and
the matrix did not run in the failing attempts.

`--public` only runs the viewer steps. The native job's gates are reached only with
`--native` (manual) or on GitHub after the push.

---

## 6. Workflow audit

### `ci.yml`

- Triggers: `push` to `main`, every `pull_request`, `workflow_dispatch` (`.github/workflows/ci.yml:3-9`).
- Concurrency: `CI-${{ github.ref }}`, `cancel-in-progress: true` (`:11-15`). On `main` this is
  what cancels the first run of every `publish.sh` push pair (51→52, 53→54). The cancelled run
  is not a failure, but its commit never gets a completed CI result.
- Timeouts: viewer job 30 min (`:26`), native job 45 min (`:129`), every `run:` step bounded
  2–20 min. No step can hang for the default six hours.
- Caches: `Swatinem/rust-cache` (viewer/native, `cache-on-failure: true`), `.mtl-install`
  cache/restore keyed on the Dockerfile, the extraction script and the pinned submodule commits,
  saved in its own step so it survives a later failure.
- Durations: green runs 44–50 were 14, 11, 13 and 21 minutes; 52 was 12m46s (viewer 5m, native
  7.5m); 54 was 7m33s (viewer only, native skipped). Viewer alone is ~5–7 min; native adds
  ~7–15 min depending on the toolchain cache.
- No rate limit, runner-image, expired-action or out-of-disk failure appears in any log. The only
  environment warning is `rustup: auto-installation is deprecated`, which does not fail a step.

### `site.yml`

- Triggers: `push` to `main`, every `pull_request`; concurrency with `cancel-in-progress: true`
  (`:9-12`); **no job or step timeouts**.
- Runs `npm ci`, lint, typecheck, build and `npm test` (Vitest) only. It does **not** run
  Playwright, axe, Lighthouse, the Python site checker or the link check. That is why run 33 is
  green on the commit whose README is broken.

### `security.yml`

- Triggers: `push`/`pull_request` limited to `Cargo.lock`, `deny.toml` and itself, plus
  `schedule: 0 3 * * 1` (`.github/workflows/security.yml:3-19`).
- Concurrency with `cancel-in-progress: true` (`:19-22`). A push on `main` in the same group can
  cancel a scheduled scan; there is no evidence of that happening here, but the group is shared.
- Run 13 failed on a newly published advisory, not on code (§1.6). This is the only workflow that
  looks at dependency advisories; nothing runs it before a push.

### `deploy.yml`

- Triggers: `push` limited to `site/**`, `crates/climb-web/**`, `docs/sample-output/**`, the two
  deploy scripts, the Caddyfile and itself, plus `workflow_dispatch`
  (`.github/workflows/deploy.yml:18-27`).
- Concurrency `deploy-production`, `cancel-in-progress: false` (`:25-30`) — deliberate, so a
  publish is never cut in half.
- Job condition requires the public repository **and** `vars.DEPLOY_HOST != ''` (`:38`). No
  variables are set, so all six runs are skipped. The Deploy gates and the live smoke have never
  executed on GitHub; the live site exists (HTTP 200), so it was deployed by hand.
- It also installs Chromium for a smoke test but never runs the Playwright suite, axe, or a full
  `cargo test`.

### Dependabot

There is no Dependabot workflow. `.github/dependabot.yml` exists **only in the private
repository**: it is in the sync's private-path list and the public repository answers 404 for it
(`gh api repos/…/contents/.github/dependabot.yml` → `404`). The public `.github/` contains only
`workflows/`. Dependabot branches exist on the private remote, but a merged Dependabot PR would be
overwritten by the next sync, so the public history never carries one.

### Does anything fail for a non-code reason?

Only Security 13 (a 20 November advisory against an unchanged lock file). The cancelled runs are
concurrency, not failures. Deploy's skips are configuration. There is no rate-limit, runner-image
or expired-action failure in the retained logs. Locally, `publish.sh` failed twice on an SSH
timeout to `github.com:22`; that is the developer's network, not CI.

---

## 7. Ranked fix list

| # | Fix | Size | Why first |
|---|---|---|---|
| 1 | Change `README.md:68` to `[CLIMB Website](https://climb-pqc.duckdns.org/)` | **XS** (1 line) | Unblocks the current `main`; the host already answers 200 over HTTPS. |
| 2 | Add a `link_check` unit test for a scheme-less host and run the `xtask` tests in the viewer replica (`cargo test -p xtask --no-default-features --all-targets`) | **S** (~10 s) | Turns this exact class into a pre-push failure; the integration test already exists and is red today. |
| 3 | Make the audit pack keep each gate's output (`target/test-logs/audit-pack-<gate>.log`) and print the first failing test name; upload those logs on failure | **S** | Run 52 is currently undiagnosable; this is the only reason its test name is unknown. |
| 4 | Add `docs-check`, `verify-numbers` and `cargo test --workspace` (or the native replica) to the pre-push path, or require the native CI job before publishing | **M** (5–15 min) | These gates have never run before a push; they are the largest structural gap. |
| 5 | Protect `main` with a ruleset requiring CI viewer + native (and Site), and make `publish.sh` the only publish route (enable the pre-push hook or document the manual route as forbidden) | **S** config + **M** process | The 14:00/14:06 pushes bypassed a red gate because nothing on GitHub required it. |
| 6 | Fix or quarantine the run-52 flake once step 3 names it | **M** | A 1-in-1683 flake that reddens a 45-minute native job is expensive. |
| 7 | Decide on Deploy: set `DEPLOY_HOST`/`DEPLOY_USER`/`DEPLOY_URL` and let the workflow run, or remove the job and document the manual deploy | **S** config, **S** code | Six skipped runs and a live site deployed by hand is ambiguity, not a gate. |
| 8 | Add the fast Playwright suite and axe to a workflow (the 15-minute glass spec can be nightly) | **L** | The a11y and parity rules currently run only on a developer machine. |
| 9 | Bump `url` to ≥ 2.5.9 (and review `rustix`/`lru` warnings) or add a justified ignore; run `cargo audit` when `Cargo.lock` changes before pushing | **S–M** | The weekly scan is red on the current tip; an unpatched advisory is not a documentation problem. |
| 10 | Retry `publish.sh`'s push over HTTPS or add a bounded retry/fallback for port 22 | **S** | Two green replicas were thrown away by a network timeout, which is what tempted the manual path. |

---

## 8. What could not be determined

- **The name of the test that failed in run 52.** `xtask` captures the child's stdout and keeps
  only the counts; the audit-pack artifact was skipped because the step failed; the job has one
  annotation (`Process completed with exit code 1`). The exact command to recover it is in §1.3.
- **The exact command sequence of the 14:00:46 and 14:06:21 pushes.** The reflog and the public
  PushEvents prove the times and the actor; the absence of a log line proves `publish.sh` did not
  do it; the absence of hooks proves no hook did it. Whether the sync was invoked by
  `tools/sync-aiori.sh` directly or through another wrapper is not recorded.
- **Whether the private repository's settings differ from this clone's.** Remote rules, secrets and
  the private Actions page are outside this checkout.
- **The public branch's protection at the organisation level.** The repository API returns no
  branch protection and no repository rulesets; an organisation-level ruleset would need an org
  admin token to read.
- **When and how the live VPS was deployed.** The site answers 200, but no Deploy run ever
  executed and the deployment is not recorded in this repository.
- **Browser, device and GPU behaviour.** This audit read logs and files; it did not run a browser,
  a screen reader or a physical device.
- **Anything GitHub retains only in its UI.** Actions minutes, billing, runner assignment and
  environment approvals are not visible through the API calls used here.

Only GitHub can prove: the organisation-level ruleset state, the repository variable values as
the runner sees them, and that no web-UI action created a commit (the API's committer identity is
strong evidence, but it is GitHub's record, not ours).
