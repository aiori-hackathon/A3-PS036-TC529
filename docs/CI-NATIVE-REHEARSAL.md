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
