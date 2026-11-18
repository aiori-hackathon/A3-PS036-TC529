# Demo fallbacks — what to do when something on stage fails

The demonstration is designed to survive failure: every live path has a
recorded fallback, and every fallback is already inside the travel kit. Work
down this page from the top; each row names the symptom, the fastest honest
recovery, and the sentence to say while recovering. Never claim a live run that
did not happen — say "this is the recorded run" and carry on.

## Before the first row: check the kit

```bash
sh verify.sh          # every file matches SHA256SUMS; silence means clean
sh serve.sh           # offline site on http://127.0.0.1:8080/
```

`verify.sh` prints every mismatch and exits non-zero, and it checks the report
bundle's own manifest too. The terminal dashboard is `bin/climb-tui`; copy it to
a local directory and `chmod +x` it first when the drive is FAT or exFAT, which
cannot carry an executable bit.

## The failure matrix

| Symptom | Fallback | Say |
|---|---|---|
| The laptop will not start or the battery is flat | Move the drive to the second laptop or the organiser's machine. The kit needs only a shell, a browser and the bundled binary. | "The evidence is on the drive; let me switch machines." |
| `serve.sh` cannot bind the port | Another process owns it. Run `sh serve.sh 8081` (or any free port) and open the printed address. | "One moment — a different port." |
| The site will not load at all | Open `report/report.html` directly from the drive. It is self-contained: no server, no fonts from the network, no scripts from a CDN. | "The report is a single file; we don't need the site." |
| The dashboard binary will not run on this machine | Use the browser dashboard at `/tui/` from `serve.sh`, or open the report and walk the same numbers there. | "The browser build is the same model; here it is." |
| The browser has no working WebAssembly | Use the terminal dashboard, then the report. The report needs no WebAssembly. | "The report carries the same claims." |
| The projector shows nothing | Switch to the printed deck and the printed evidence sheet, and keep the laptop screen for yourself. | "Let me talk you through the printed evidence." |
| Wi-Fi or venue network is down | Nothing in the kit needs it. If a browser extension or a page tries to fetch, `cargo xtask demo --offline` from the source checkout blocks every non-local request and proves it. | "This demonstration was designed to run offline." |
| Docker is unavailable or the live DNS pipeline will not start | Show the recorded capture in the dashboard's Live tab and the recorded RTT run. The live pipeline is an optional demonstration; the measurement of record is the recorded artifact. | "The live pipeline is optional; here is the recorded run." |
| A number is challenged | Open `report/SHA256SUMS` and re-verify it, then open the claims registry in the report or the Integrity tab. Every claim carries its JSON pointer. | "Here is the claim and the pointer; the artifact is on the drive." |
| The deck PDF will not open | Open `docs/PITCH.md` and the printed deck instead. The pitch carries the same argument without the layout. | "I have the same material in text." |
| A file is missing from the drive | Do not improvise from memory. Use the second USB drive, or the copied report bundle. Then re-run `sh verify.sh`. | "Let me use the second drive and verify it." |

## After the session

- Re-run `./verify.sh` on both drives before they are packed away.
- If anything was changed on a drive during the session, rebuild the kit from
  the source checkout with `cargo xtask kit` rather than editing files by hand.
- The packing checklist, the restore steps and the kit's measured size are in
  [`KIT.md`](KIT.md). The five hardest judge questions are in [`QA.md`](QA.md).
