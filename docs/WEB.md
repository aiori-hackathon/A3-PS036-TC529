# The dashboard in a browser tab

The terminal dashboard (`crates/climb-tui`) also runs in a browser. This page
explains what that build contains, how to make one, how the recordings reach
it, what it deliberately does not do, and how to check a build without opening
a browser by hand.

The summary: it is the same Rust model, widgets and key table compiled to
WebAssembly with `wasm-bindgen`, drawn through `ratzilla` (WebGL2, then Canvas
2D, then DOM) into a plain static page. There is no server, no account, no
network beyond the page's own origin, and no live cryptography.

## 1. What is in the module

- `crates/climb-web` is a `cdylib` + `rlib` workspace member that depends on
  `climb-tui` with default features off. The terminal feature (crossterm,
  signal handling, tracing to a file) is compiled out; the model, the view
  models, the widgets, the theme and the key table are compiled in.
- The browser opens on the `brand` theme, matching the site palette. `T`
  cycles dark, light, contrast and brand exactly as it does in a terminal.
- The clock is `web_time::Instant` behind the same `Clock` enum the terminal
  uses; the draw loop delivers the model's ticks at 30 Hz.
- One keyboard adapter (`climb_tui::map_web_key`) turns a DOM key into the same
  `Action` the terminal keymap produces. A window embedded in a longer page
  sets `data-keys="focus"` on its grid, so it only takes keys while it has
  focus; the standalone page keeps the whole-page keyboard.
- `?tour=1` (or `?tour`) starts the guided tour on load.

## 2. Build the module

Prerequisites:

- Rust with the `wasm32-unknown-unknown` target:
  `rustup target add wasm32-unknown-unknown`
- `trunk`, the WebAssembly bundler (`cargo install trunk --locked`, or a
  release binary).
- `wasm-opt` from [Binaryen](https://github.com/WebAssembly/binaryen) on
  `PATH`. Trunk runs it for a release build; without it, trunk tries to
  download Binaryen and the build fails on an offline machine.

```bash
cd crates/climb-web
trunk build --release
```

The build writes `crates/climb-web/dist/`: one hashed JavaScript entry, one
hashed `_bg.wasm`, and the standalone `index.html`. `Trunk.toml` selects the
`wasm-release` Cargo profile — `opt-level = "z"`, fat LTO, one codegen unit and
`panic = "abort"` — so the literal release command gets the size profile
without changing the native release profile the benchmark uses.

Open the standalone page with `trunk serve` (it binds `127.0.0.1:8080`) and
visit `http://127.0.0.1:8080/`. That page is the terminal's own layout: a
full-viewport grid, no site chrome.

## 3. Publish it beside the site

The module is a build artifact, not a committed source file. The site build
integration in `site/astro.config.mjs` reads `crates/climb-web/dist` and, when
a build is present, copies the entry and the binary into the built site under
`/tui/` and writes `/tui/manifest.json` naming them. It also publishes the
three recordings under `/data/`. During `npm run dev` the same integration
serves both paths from the repository, so the page can be developed without a
site build.

```bash
node --version          # the site needs the version in site/.nvmrc
cd crates/climb-web && trunk build --release && cd ../..
cd site && npm ci && npm run build
```

The built page is `site/dist/tui/index.html`; the module sits beside it. If
the module has not been built, the site still builds: the page shows its
build state instead of a broken canvas, and the build log prints a warning
naming `crates/climb-web/dist`.

The home route embeds the same window below the hero. It loads the module only
after the reader has scrolled to it (and after the page is idle), so the hero's
first paint never waits on a wasm download. A reader who saves data gets a
load button instead of an automatic download.

## 4. Where the data comes from

The module fetches three documents from its own origin:

| Path | Contents |
| --- | --- |
| `/data/climb-results.json` | The consolidated results the terminal dashboard reads |
| `/data/ladder-verification.recorded.json` | The recorded ladder model-versus-library check |
| `/data/dns-capture.recorded.json` | The recorded DNS exchange the Live view replays |

The files are copied from `docs/sample-output/` at site build time, so there is
one source of truth: refresh the recordings there (for example with
`cargo xtask reproduce` and `scripts/make-sample-output.sh`), rebuild the site,
and the browser reads the new copies. The module itself is not rebuilt when the
data changes.

Each document is fetched independently. A failed request falls back to the copy
compiled into the module, and the status bar inside the dashboard names the
source: `[site]` for the fetched file, `[embedded]` for the compiled-in copy. A
document that arrives but does not parse is a hard error naming the request
path — never a silent fallback to older numbers. Pressing `r` re-parses the
bytes the page already has.

## 5. Check a build

```bash
scripts/wasm-smoke.sh
```

The script builds the module with `trunk build --release`, checks the release
budget, serves `crates/climb-web/dist` on `127.0.0.1`, requests the page and the
module over HTTP, and asserts that the module begins with the WebAssembly magic
bytes. When Playwright's Chromium is installed under `site/node_modules`, it
also drives the page: waits for the canvas, presses every view key, opens the
tour, and fails on any console error. `SKIP_BUILD=1` verifies an existing
`dist/`; `PORT=…` moves the local server.

The budget is 6 MB raw and 2.5 MB compressed, enforced by the script. <!-- docs-check:allow-number -->

The release build at the time of writing is 2,837,697 bytes raw and 1,411,134 bytes brotli, with a 62,851-byte JavaScript entry. <!-- docs-check:allow-number -->

The integration's `/data` copies are the committed recordings, byte for byte;
`site/tests/tui.test.ts` fails if the file list drifts from the paths the Rust
loader fetches.

## 6. Limits

- **Recorded data only.** The module reads artifacts. It does not sign, verify
  or benchmark anything, and it never links libMTL, liboqs or OpenSSL. The
  Ladder tab's badge compares the model with the recorded library output.
- **No server, no network.** Everything is static files on one origin. There
  are no API calls, no analytics and no third-party scripts.
- **No filesystem.** The browser cannot open `target/climb-results.json`; the
  embedded and `/data` copies are the only sources.
- **A wide window.** The dashboard is designed for at least eighty columns.
  Below that the window scrolls sideways rather than drawing a smaller,
  unreadable grid. The touch row below a full-page window sends the same keys
  as the keyboard.
- **Keys belong to the window.** While the grid has focus the dashboard claims
  its keys, Tab included, exactly as a terminal does. Move focus out with the
  browser's own focus shortcut or by clicking elsewhere.
- **Browser support.** The build targets current Chromium, Firefox and WebKit
  engines via WebGL2, Canvas 2D or plain DOM. Only headless Chromium with a
  software renderer has been exercised here; no physical GPU, touch device or
  narrow phone viewport has been verified.
- **The module is not committed.** A checkout needs the wasm toolchain to
  produce a browser build. The terminal dashboard and the rest of the site
  build without it.

## 7. Troubleshooting

- **The page says the module is not in this build.** `crates/climb-web/dist`
  was absent or empty when the site was built. Run `trunk build --release` in
  `crates/climb-web`, then rebuild the site.
- **`wasm-opt` errors.** Install Binaryen or put `wasm-opt` on `PATH`. Trunk's
  fallback is a download, which needs network access.
- **The status bar says `[embedded]`.** A `/data` request failed. Confirm the
  site build published the three documents (the build log names them) and that
  the page is served from the site origin, not opened as a local file.
- **The grid says "terminal too small".** The canvas is narrower than the
  dashboard's minimum. Widen the window; the page deliberately keeps the floor
  instead of compressing the layout.
