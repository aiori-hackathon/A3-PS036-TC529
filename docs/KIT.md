# Travel kit — pack, verify, restore

The travel kit is one directory on a USB drive that carries everything the
demonstration needs without a build and without a network: the built site and a
one-command server, the release dashboard binary, the recorded fixtures, the
self-checked report bundle, the submission deck, the pitch, the judge Q&A and
[`DEMO-FALLBACKS.md`](DEMO-FALLBACKS.md).

Build it from the repository with one command:

```bash
cargo xtask kit                # builds the site and the dashboard, then packs dist/kit
cargo xtask kit --skip-site-build   # reuse an existing site/dist
cargo xtask kit --skip-docker       # never look for Docker images
cargo xtask kit --with-docker       # fail when the live-pipeline images are absent
cargo xtask kit --skip-browser-dashboard  # accept a site whose /tui window has no module
```

The site build publishes the browser dashboard from `crates/climb-web/dist`, so
build that once with `cd crates/climb-web && trunk build --release` before the
kit. It needs the `wasm32-unknown-unknown` target and `wasm-opt`; without them
`trunk` tries to download a tool release. When the module is missing the kit
refuses to pack a silently broken `/tui` window and names the command, unless
`--skip-browser-dashboard` says a site-only kit is wanted.

The command prints one line per component and the total kit size at the end;
the same measured table is written into the kit's `README.txt`, so the size on
the drive is the size the command measured, never a number copied into this
document. The command refuses to clear a directory that is not a previous kit,
and it stages the new kit beside the old one so a failed build never leaves a
half-written drive.

## What the kit contains

| Path | Contents |
|---|---|
| `site/dist/` | the built static site, including the browser dashboard, the report, verify and reproduce routes and the published evidence |
| `serve.sh` | starts a loopback-only static server over `site/dist`, with the deployment's 404 behaviour and no directory listings |
| `bin/climb-tui` | the release dashboard binary, viewer-only (no native crypto stack) |
| `data/` | the recorded DNS capture and the recorded ladder-verification fixture |
| `report/` | the committed results document, its HTML report, every chart, and the bundle's own `SHA256SUMS` and `PROVENANCE.txt` |
| `deck/climb-deck.pdf` | the submission deck |
| `docs/` | `PITCH.md`, `QA.md`, `DEMO-FALLBACKS.md` and this page |
| `docker/` | optional: the compressed live-pipeline images, when they were present at packing time |
| `verify.sh` | checks every file on the drive against `SHA256SUMS`, then checks the report bundle against its own manifest |
| `SHA256SUMS` | the kit manifest: one digest and one relative path per file, excluding the manifest itself |
| `README.txt` | the printable one-page restore guide |

Docker images are optional and large. When the packing machine has them, the
command compresses each image it finds, prints its size, and records the total;
`--skip-docker` leaves them out even when they exist, and `--with-docker` turns
a missing image into an error instead of a note. A kit without them is still a
complete demonstration of the recorded evidence; only the optional live DNS
pipeline needs them.

## Packing checklist

Prepare the night before, then verify on the morning of the session.

- [ ] Laptop, charged; charger and the correct plug adapter.
- [ ] Presentation adapters: HDMI, USB-C and any venue-specific dongle.
- [ ] Mouse and a spare cable.
- [ ] Printed deck.
- [ ] Printed evidence sheet: the quote sheet and the criteria card.
- [ ] Two USB drives, both carrying the same kit and both verified.
- [ ] A second laptop, or a confirmed fallback machine, when the venue allows it.
- [ ] The kit's printed `README.txt` in the bag, not only on the drive.
- [ ] `cargo xtask kit` run from the exact commit being presented, and
      `./verify.sh` run on **each** drive after copying.
- [ ] `./serve.sh` opened once and every route visited, offline.
- [ ] `./bin/climb-tui` opened once and the guided tour completed.

## Restore steps at the venue

1. Plug the drive in. Do not copy the whole kit to the venue machine; only the
   dashboard binary needs a local copy, because a FAT or exFAT drive cannot
   carry an executable bit.
2. Open a terminal in the kit directory and run `sh verify.sh`. It prints a line
   only for a failure, a missing file or a file the manifest does not list, and
   exits non-zero when anything is wrong. If it fails, switch to the second
   drive before investigating.
3. Run `sh serve.sh` (optionally with a port: `sh serve.sh 8081`). It prints the
   loopback address it is serving. Open that address for the site, `/tui/` for
   the browser dashboard, and `/report/` for the report.
4. For the terminal dashboard, copy the binary to a directory with Unix
   permissions and run it there:
   `cp bin/climb-tui /tmp/climb-tui && chmod +x /tmp/climb-tui && /tmp/climb-tui`.
   Keys `1` through `9` switch tabs, `t` starts the guided tour, `?` opens help
   and `q` quits.
5. If a browser or the server is unavailable, open `report/report.html`
   directly. It is self-contained.
6. If anything fails, follow [`DEMO-FALLBACKS.md`](DEMO-FALLBACKS.md) before
   improvising.

Nothing in the kit contacts a network service. The server binds the loopback
address only, the dashboard reads files on the drive, and the report embeds its
own charts and evidence.

## Rebuilding and troubleshooting

- The kit directory is a build output. Rebuild it with `cargo xtask kit`; never
  edit its files by hand, because the manifest would no longer describe them.
- The site build needs the site toolchain once (`cd site && npm ci`). The
  dashboard build needs the pinned Rust toolchain. The kit command reports which
  step failed and names the command that fixes it.
- A kit packed from a dirty working tree still verifies; the report bundle's
  `PROVENANCE.txt` names the commit the recorded evidence came from, which is
  the claim that matters to a judge.
- For the source-level checks that back the kit, see the quickstart in the
  repository README and the demo preflight at [`../demo/preflight.sh`](../demo/preflight.sh).
