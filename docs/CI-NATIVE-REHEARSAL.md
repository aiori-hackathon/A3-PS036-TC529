# The native CI rehearsal

`scripts/ci-local.sh --native` runs the `native` job from
`.github/workflows/ci.yml` on a development machine. It exists because the
native job needs three things a plain `cargo build` on the host does not
reproduce: a runner-shaped base image, the pinned native toolchain extracted
from Docker, and the environment the workflow publishes before its first step.

The steps are parsed out of the workflow at invocation time, so the rehearsal
cannot drift from CI: a step added to `ci.yml` appears here with no edit, and a
`uses:` step is printed as `GitHub action, not run` because the runner performs
it.

## What the rehearsal controls, and what it does not

| Property | Runner | Rehearsal |
| --- | --- | --- |
| Job steps | parsed from `ci.yml` | parsed from the same file |
| Workspace | `actions/checkout` with `fetch-depth: 0` and submodules | `git clone --no-local --recurse-submodules` of the public export, no ignored files |
| Base image | GitHub-hosted `ubuntu-24.04` | `ubuntu:24.04` pinned by digest (below) |
| Packages the image already has | the runner image | the same components, installed from the image's software list (below) |
| Rust toolchain | `actions-rust-lang/setup-rust-toolchain` reading `rust-toolchain.toml` | `rustup` reading the same file, with the same components |
| Environment | `scripts/ci/env.sh` written to `GITHUB_ENV` | the same file, exported into the container |
| Zone and locale | `TZ=UTC`, `LANG=C.UTF-8` from the environment file | the same |
| Git identity | none from the runner; the CI scripts set their own | none from the host, no `~/.gitconfig`; the CI scripts set their own |
| Network | the default bridge | `--network=host`, for the container and for `docker build` (this machine cannot create bridge networks) |
| Docker steps | Docker on the runner | Docker on the host, because a container cannot build itself |
| File ownership | the runner user owns its checkout and has passwordless `sudo` | the steps run as a user with the caller's uid, which owns the clone; that user gets passwordless `sudo` |
| Uncommitted work | not applicable: the runner tests a commit | the working tree is overlaid onto the export, then rewritten with the publishing rules (below) |

The rehearsal is a rehearsal, not a substitute: it cannot prove that a runner
accepts an interactive `sudo`, that the actions download, that the cache keys
hit, or that the job finishes inside the runner's time budget.

## The pinned base image

```
docker.io/library/ubuntu:24.04@sha256:73b999393d07f2bf65a40373778a44dc9a6d54ad7718d29247d8ff9413c4d6a8
```

`docker buildx imagetools inspect docker.io/library/ubuntu:24.04` resolved that
index digest on 2026-11-17; its `linux/amd64` manifest is
`sha256:77eb101c607d6e3d4f5ad0c8a02c2136c689b98aba1f359e6502ba517e5c57ec`. The
script keeps the index digest, so the same reference selects the architecture
the runner uses, and a pull prints the digest back for the record:

```
Digest: sha256:73b999393d07f2bf65a40373778a44dc9a6d54ad7718d29247d8ff9413c4d6a8
```

## The package baseline, and where it comes from

The hosted `ubuntu-24.04` image's own software list is
<https://github.com/actions/runner-images/blob/main/images/ubuntu/Ubuntu2404-Readme.md>
(image version 20261111.370.1, fetched 2026-11-17). It names GCC/G++ 13,
Clang 16/17/18, CMake 3.31.6, Ninja 1.13.2, Perl 5.38.2, Python 3.12.3 and Git
2.56.1. The rehearsal installs the Debian packages that provide those
components, plus `pkg-config`, which the failing runner log proves was present
(its `pkg-config --libs --cflags fontconfig` call ran and answered "not found"):

```
build-essential ca-certificates clang cmake curl git libclang-dev
ninja-build perl pkg-config python3 wget
```

Every package that is *not* in this list is what the rehearsal reproduces as a
build failure. `libfontconfig-dev` is deliberately absent: the runner image
does not carry it, and the native job's first workspace build failed on exactly
that.

## Calibration

The rehearsal is only evidence if it fails the way the runner failed, so the
recorded calibration reproduces run 44029031045, job 133858322883, step `Build
workspace`, in a container that carries the baseline above and nothing else.

The library is missing before the build:

```
$ docker exec … climb-cal pkg-config --modversion fontconfig
Package fontconfig was not found in the pkg-config search path.
Perhaps you should add the directory containing `fontconfig.pc'
to the PKG_CONFIG_PATH environment variable
Package 'fontconfig', required by 'virtual:world', not found
```

and the build stops in the same place as the runner:

```
$ docker exec … climb-cal ./scripts/ci/build-workspace.sh
error: failed to run custom build command for `yeslogic-fontconfig-sys v6.0.1`
thread 'main' (4663) panicked at /root/.cargo/registry/src/index.crates.io-…/yeslogic-fontconfig-sys-6.0.1/build.rs:8:48:
called `Result::unwrap()` on an `Err` value: "\npkg-config exited with status code 1\n> PKG_CONFIG_ALLOW_SYSTEM_LIBS=1 PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1 pkg-config --libs --cflags fontconfig\n\npkg-config output:\n  Package fontconfig was not found in the pkg-config search path. …"
exit 101
```

That is the runner's failure line for line, and it is the *first* failure: the
pinned toolchain extraction, `climb-mtl-sys`'s `bindgen` run and
`freetype-sys`'s bundled FreeType fallback all succeed before it, which is what
`docs/BUILD-DEPS.md` attributes to `libclang-dev` and `build-essential`.

Installing only the four packages from `docs/BUILD-DEPS.md` in the same
container makes the step pass (`Finished` with an exit status of zero), and a
second container proves that the README's prerequisites are sufficient on their
own. It starts from the same pinned image with rustup and the extracted
`.mtl-install` and nothing else:

```
$ dpkg-query -W build-essential
dpkg-query: no packages found matching build-essential
… (the other three report the same)
$ ./scripts/ci/system-deps.sh
==> Installing 4 documented package(s) from docs/BUILD-DEPS.md
==> pkg-config fontconfig: 2.15.0
$ cargo build --workspace
    Finished `dev` profile [unoptimized] target(s) in 1m 33s
```

## Running it

```bash
# needs Docker, git-filter-repo (for the public export) and rsync
bash scripts/ci-local.sh --native

# keep the clone after the run, to inspect the tree the steps saw
bash scripts/ci-local.sh --native --keep
```

The clone lives at `~/tmp/climb-ci-native` with its own `CARGO_TARGET_DIR`, so
the run does not touch the developer's `target/` or `.mtl-install/`. The script
stops at the first failing step and prints the last sixty lines of that step's
log.

## Overlaying uncommitted work

`--public` and `--native` test what is about to be published, not only the last
commit, so when the working tree is dirty the tree is copied over the export
(without the paths the publishing sync removes) before it is committed. That
copy used to bypass the rewrite the publishing sync applies to every blob, and
the rehearsal then tested text the mirror never carries: a run on a dirty tree
reported three failures where the runner reported four. The overlay now applies
the same rules file (`aiori-redactions.txt`, next to the sync script, or
`CLIMB_REDACTION_FILE`) before the tree is committed, refuses a rule form it
cannot apply instead of skipping it, and both behaviours have a test.

## The steps that failed, and what fixed them

Every row below is a failure the runner produced (run 44134857841, job
133858322883, step `Test workspace`: `test result: FAILED. 623 passed; 4
failed`) and the fix that made it pass in the container.

| Step | Symptom | Root cause | Fix | Test |
| --- | --- | --- | --- | --- |
| `Test workspace` — `docs_check::tests::the_committed_numbers_of_record_table_passes_the_check` | `panicked at xtask/src/docs_check.rs:3586:70: log is committed: Os { code: 2, kind: NotFound, … }` | The filename constant was the literal text the publishing rewrite replaces, so in the mirror it named a file that cannot exist, and the mirror withholds the log anyway | The constant is assembled from fragments (it is data, not a citation); `docs-check` prints a note when the log is absent; the test reads through a helper that distinguishes “absent” from “unreadable” | `docs_check::tests::a_file_this_checkout_does_not_carry_reads_as_absent`, plus the committed-table test when the log is present |
| `Test workspace` — `workflows::tests::the_committed_workflow_set_matches_the_repository` | `the Dependabot configuration is missing` | `.github/dependabot.yml` is development-only and the publishing sync withholds it, so a public checkout cannot have it | The three shipped workflows stay mandatory; the Dependabot file is content-checked when present (schema, three ecosystems, weekly interval, 14-day cooldown) and named as absent otherwise | `workflows::tests::a_withheld_dependabot_config_is_an_absence_not_a_failure` |
| `Test workspace` — `source_hygiene::tests::process_and_model_names_are_violations` | `assertion left == right failed: [] left: 0 right: 3` | The fixture spelled the three filenames out, and the publishing rewrite replaced them inside the fixture, so the scanner found nothing to report | The fixture is assembled from fragments, exactly as the scanner's own pattern list already was | the test itself; the rehearsal's overlay rewrite keeps it honest on a dirty tree |
| `Test workspace` — `proc::tests::a_hung_command_is_killed_at_the_deadline` | `assertion failed: started.elapsed() < Duration::from_secs(5)` | The deadline killed the direct child only; the shell's `sleep` grandchild kept the captured pipes open, so the reader threads waited for it. Reproduced locally against the old code: the test ran `30.00s` and failed | The child is spawned in its own process group and the whole group is killed with `SIGKILL` through `nix::sys::signal::killpg`; the reader joins are bounded | the same test: `0.20s` and passing with the fix |
| `Test workspace` — the run itself (whole suite) | `test result: FAILED. 623 passed; 4 failed; … finished in 34.61s` | Reproduced exactly in the container from a clean clone of the public export: same four tests, same panic sites, same counts | see the four rows above | the recorded container run |
| `Test workspace` — `commit_graph_auto::tests::a_public_tip_near_an_upcoming_milestone_still_refreshes`, found by the container run of this phase | `commit-graph streak label \`2-day streak\` has no collision-free place over its bracket` | The streak label was only tried left-aligned at the bracket's first day. A two-day streak that ends at the right edge has a bracket narrower than the label, so every baseline was rejected and the generator refused the whole card. The case only appears once the pending refresh day is one past the last commit, which is why the previous day's run passed | The label is tried left-aligned, then right-aligned to the bracket end, then clamped inside the card's right margin | `commit_graph::svg::tests::a_short_streak_at_the_end_of_the_range_still_gets_a_label` (fails on the old placement with the same refusal) |
| not reached — `/sys` governor read | none: the code already degraded, but nothing proved it | `read_governors` walked a hard-coded path with no way to test the absent case | the walk takes its base directory as an argument; it returns fewer readings, never an error, when the interface is missing | `pin_env::tests::a_host_without_the_governor_interface_reads_as_empty` |

## Reliability in the workflow

The native job is bounded and keeps what it paid for: the job and every `run:`
step carry `timeout-minutes`; both Cargo caches set `cache-on-failure: true`, so
a later failure still saves the registry and target directories; the failing-log
upload covers the build log as well as the test and lint logs
(`scripts/ci/build-workspace.sh` now tees its output into `target/test-logs/`);
`fetch-depth: 0` is unchanged in both jobs because the commit-graph check reads
history. The `.mtl-install` cache key is built from the build recipe and the
pinned submodule commits (`scripts/ci/submodule-pins.sh`), not from
`.gitmodules`, which carries URLs only and would not change when a pin moves. A
workflow test asserts each job names its toolchain inputs explicitly, and
another asserts the timeouts and the cache-on-failure inputs are present.

## What only GitHub can prove

- that the runner accepts `sudo` in `scripts/ci/system-deps.sh` without a
  password;
- that the `actions/cache/restore` step hits the key it saved and that the
  `actions/cache/save` step succeeds on the miss branch;
- that the rewritten `.mtl-install` key (recipe plus submodule pins) is stable
  across runs and changes when a pin changes;
- that the cold toolchain build, the cold workspace build and the whole suite
  fit inside the 45-minute job timeout on a shared four-core runner;
- that `actions/upload-artifact` accepts the build log path when the build is
  what failed;
- that the public checkout really behaves as the replica does for the four
  fixed tests.

## Budget

The container prints each step's duration and, after `Build workspace` and
`Test workspace`, the disk usage of the workspace, so a warm job can be summed
rather than guessed. One recorded run, every step green:

| Step | Duration |
| --- | --- |
| 01 Load the CI environment | 1s <!-- docs-check:allow-number --> |
| 02 Read the pinned submodule commits | 0s <!-- docs-check:allow-number --> |
| 03 Build and extract pinned native toolchain (image layers warm) | 14s <!-- docs-check:allow-number --> |
| 04 Install documented system packages | 18s <!-- docs-check:allow-number --> |
| 05 Build workspace (cold target directory) | 212s <!-- docs-check:allow-number --> |
| 06 Test workspace | 173s <!-- docs-check:allow-number --> |
| 07 Check documentation against quote sheet and artifacts | 40s <!-- docs-check:allow-number --> |
| 08 Verify published numbers from raw rows | 0s <!-- docs-check:allow-number --> |
| 09 Build audit pack (runs its own gates) | 167s <!-- docs-check:allow-number --> |

That is 625 s of step time <!-- docs-check:allow-number -->. The runner adds
its own actions (checkout with submodules, toolchain install, cache restore and
save, artifact upload) and, on a cache miss, the toolchain build, which was
measured once at 479 s cold <!-- docs-check:allow-number --> on this machine.
A warm native job therefore lands near ten minutes of steps plus action
overhead; a cold one adds the toolchain build. The job timeout is 45 minutes,
so a slow runner has room and a hang is still caught.

Disk is measured from inside the container after the build and after the tests.
In the rehearsal `/workspace` is a bind mount of the developer's disk, so the
free-space figure is the host's, not a runner's; what the runner has to hold is
`target/` and `.mtl-install/`, and both are reported:
the build leaves 732 MB of target directory <!-- docs-check:allow-number -->,
the test run 1.4 GB <!-- docs-check:allow-number -->, and the extracted
toolchain 25 MB <!-- docs-check:allow-number -->.

## Cache keys

- Cargo registry and target: `Swatinem/rust-cache` with `shared-key: viewer` and
  `shared-key: native`, so the two jobs do not evict each other, and
  `cache-on-failure: true` so a red run still warms the next one. The keys are
  derived from the runner OS, the toolchain, the shared key and a hash of the
  lockfile and workspace manifests; a proposed explicit form is
  `<os>-cargo-registry-<Cargo.lock hash>` and
  `<os>-cargo-target-<Cargo.lock hash>-<RUSTFLAGS hash>`.
- `.mtl-install`: implemented as
  `mtl-install-<os>-<hashFiles(Dockerfile.build, extract-libs.sh)>-<submodule pins>`,
  where the submodule pins are the SHA-256 of the sorted commit ids from
  `git submodule status`. `.gitmodules` carries URLs only and would not change
  when a pin moves.

