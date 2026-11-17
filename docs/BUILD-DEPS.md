# Build dependencies for the native workspace build

`cargo build --workspace` compiles two dependencies whose build scripts reach
outside Cargo for something the Rust toolchain cannot provide: a small C
library for the chart backend, and `libclang` for the bindings generator. A
stock Ubuntu 24.04 does not carry either of them, so the build fails until they
are installed.

The list below is the whole requirement. It is installed by the native CI step
(`scripts/ci/system-deps.sh` reads this file), it is repeated verbatim in the
README's prerequisites, and a test fails when the two lists differ.

<!-- build-deps:begin -->
```
build-essential
pkg-config
libfontconfig-dev
libclang-dev
```
<!-- build-deps:end -->

Install all four in one command on Ubuntu 24.04:

```bash
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
    build-essential pkg-config libfontconfig-dev libclang-dev
```

## What each package provides, and what fails without it

| Package | Needed by | Provides | Failure without it |
| --- | --- | --- | --- |
| `build-essential` | `freetype-sys` (bundled fallback), the linker | a C and C++ compiler, `make`, the C library headers | `freetype-sys` cannot compile its bundled FreeType; `rustc` has no linker |
| `pkg-config` | `yeslogic-fontconfig-sys`, `freetype-sys` | the `pkg-config` executable their build scripts call | the build script cannot probe any `*.pc` file |
| `libfontconfig-dev` | `yeslogic-fontconfig-sys` | `fontconfig.pc`, `libfontconfig.so`, the headers | `Package fontconfig was not found in the pkg-config search path` |
| `libclang-dev` | `clang-sys` (build-dependency of `bindgen`) | `libclang.so` | `clang-sys` cannot find `libclang` |

The dependency path to the font library is
`climb-report [native] -> plotters [ttf] -> font-kit -> yeslogic-fontconfig-sys`.
`plotters` is a default feature of `climb-report`, so the native workspace
build always reaches it. The dependency path to the bindings generator is
`climb-mtl-sys [build-dependencies] -> bindgen -> clang-sys`.

## Deliberately not on the list

- **`freetype`**: `freetype-sys` probes for it and, when the probe fails,
  compiles the copy it vendors. `build-essential` is what that fallback needs.
- **`libssl-dev`, `liboqs`, libMTL**: the headers and libraries come from
  `docker/extract-libs.sh`, which extracts the pinned build into
  `.mtl-install/`.
- **`cmake`, `ninja-build`, `perl`, `autoconf`, `automake`, `libtool`, `bc`**:
  only the container that builds the pinned native toolchain needs them, and
  `docker/Dockerfile.build` installs them, with versions pinned, inside that
  image.

## Why the native CI step installs the list explicitly

The GitHub-hosted `ubuntu-24.04` image already carries `build-essential`,
`pkg-config`, `clang`/`libclang-dev`, `cmake`, `ninja-build` and `perl`
(actions/runner-images, `images/ubuntu/Ubuntu2404-Readme.md`, image version
20261111.370.1), but it does not carry `libfontconfig-dev`. Relying on the rest
of the image made the job pass or fail according to a base image the repository
does not pin, so the workflow installs this list instead and prints the version
it resolved for each package.
