# Demo fallbacks — four plans, and the sentence to say while switching

The demonstration is designed to survive failure: every live path has a
recorded fallback, and every fallback is already inside the travel kit. Work
down this page from Plan A to Plan D. Never claim a live run that did not
happen: say “this is the recorded run” and carry on. The printed pages are the
deck and [`EVIDENCE-SHEET.html`](EVIDENCE-SHEET.html); the packing checklist is
[`KIT.md`](KIT.md).

## The plan ladder

| Plan | Trigger | First command | Who opens |
| --- | --- | --- | --- |
| **A — offline kit** | The laptop works, the drive is present and `verify.sh` is green. This is the default. | `sh verify.sh` then `sh serve.sh` | Mohit runs the commands; Pratyay takes the first number. |
| **B — site only** | The drive is unreadable, the dashboard binary will not run on this machine, or only a browser is available, and the venue network works. | `xdg-open https://climb-pqc.duckdns.org/` (macOS: `open`) | Mohit opens the route; Pratyay frames it as recorded results. |
| **C — PDF and printed sheet** | No browser or terminal can render, or the projector accepts only a PDF. The deck and the printed evidence sheet are on the table. | `xdg-open A3-PS036-TC529.pdf` | Pratyay walks the deck; Mohit reads the sheet. |
| **D — spoken only** | No device, no readable PDF and no projector; the session continues verbally. | None. If any machine returns, the first command is `cargo xtask demo --offline` from a checkout. | Pratyay leads; Mohit supplies the evidence rows from memory of the sheet, not from invented numbers. |

A plan is not a failure. Say which plan you are switching to out loud, so the
audience knows the evidence did not change, only the screen did.

## Plan A — the offline kit

**Trigger.** The default. The drive mounts, `verify.sh` exits clean, and the
laptop can run a shell, a browser and the bundled dashboard binary.

**First command.**

```bash
sh verify.sh
sh serve.sh
```

`verify.sh` checks every file against `SHA256SUMS`, including the report
bundle's own manifest. `serve.sh` prints the loopback address it bound. If a
port is taken, pass another one: `sh serve.sh 8081`.

**Who says what.**

- **Mohit:** “Everything you are about to see is on this drive; it was verified
  before we walked in. I will open the dashboard and the site.”
- **Pratyay:** “The first number is the wire result: the amortised configuration
  sends 96.508 % fewer bytes than plain SLH-DSA at the Medium size. The class is
  derived, and the baseline is A.”
- **Mohit:** “The report and the dashboard read the same recorded artifact. The
  Integrity tab fails if they ever disagree.”
- **Pratyay:** “This is a measurement of cost, not a security proof.”

**What you show.** The site from `serve.sh`, `report/report.html`, the terminal
dashboard from `bin/climb-tui`, and the printed evidence sheet. The five numbers
on the sheet are the five to quote.

## Plan B — the site only

**Trigger.** The drive will not mount, the dashboard binary will not execute on
this machine, or only a browser is available and the venue network works. The
public site is permitted by AIORI's written reply of 2026-11-10 and serves
recorded results and a client-side dashboard only.

**First command.**

```bash
xdg-open https://climb-pqc.duckdns.org/
```

On macOS use `open`; if the browser is already open, type the address. Open
`/report/`, `/verify/`, `/reproduce/` and `/tui/` from the home page.

**Who says what.**

- **Mohit:** “The drive is not cooperating, so I am switching to the recorded
  results site. Nothing is being measured live; every number is the same
  committed artifact.”
- **Pratyay:** “The site is a viewer, not a deployment of the signing pipeline.
  Signing and the DNS resolver stay local, as the compliance note records.”
- **Mohit:** “Here is the browser dashboard at `/tui/`; it is the same model as
  the terminal one.”

**What you must not do.** Do not imply the site is performing a live
measurement. If the network is also down, move to Plan C.

## Plan C — PDF and printed sheet

**Trigger.** No browser or terminal can render, or the projector accepts only a
PDF. The deck `A3-PS036-TC529.pdf` and the printed evidence sheet are on the
table.

**First command.**

```bash
xdg-open A3-PS036-TC529.pdf
```

On macOS use `open`; if no laptop can open it, open the printed deck by hand.
Keep the printed evidence sheet beside it, not in a bag.

**Who says what.**

- **Pratyay:** “The screen is not available, so we will use the paper. The deck
  carries the argument; the sheet carries the five numbers with their class and
  baseline.”
- **Mohit:** “The sheet's first row is the wire reduction at Medium, derived,
  baseline A: 96.508 %. The second is signing, derived, baseline A: 9,434 ×.
  The third is cached verification, derived, baseline A: 77.34 ×.”
- **Pratyay:** “The fourth is the modelled UDP fit, analytical, baseline C:
  11 of 12 rows. The fifth is the live DNS capture, derived, baseline B:
  97.307 %.”
- **Mohit:** “The scope sentence on the sheet says it: empirical measurements
  only, not a security proof.”

**What you must not do.** Do not quote a number that is not on the printed
sheet. If a judge challenges one, point to its JSON pointer on the sheet and
offer to open the artifact later.

## Plan D — spoken only

**Trigger.** No working device, no readable PDF and no projector. The session
continues verbally.

**First command.** None. If any machine returns, run
`cargo xtask demo --offline` from a checkout; it blocks every non-local request
and proves the demonstration needs no network.

**Who says what.**

- **Pratyay:** “We will do this without a screen. CLIMB measures what MTL mode
  saves when DNSSEC signs with SLH-DSA, and what it costs in return.”
- **Mohit:** “We ran plain SLH-DSA, MTL without amortisation, and MTL amortised
  on the same deterministic datasets, the same key and the same pinned host.
  Only batching differs.”
- **Pratyay:** “At Medium, amortised MTL reduces wire bytes by 96.508 % against
  plain SLH-DSA. Signing is 9,434 × faster. Cached verification is 77.34 ×
  faster; cold verification is not, and we say so.”
- **Mohit:** “The live DNS capture shows the same shape: 24,438 B full against
  658 B condensed, 97.307 %. It is a local capture over TCP.”
- **Pratyay:** “The UDP fit is a model: 11 of 12 rows fit the payload for the
  amortised configuration. It is labelled analytical, not measured.”
- **Mohit:** “The repository is github.com/aiori-hackathon/A3-PS036-TC529. The
  evidence sheet, the report and the audit table are there.”
- **Pratyay:** “No new cryptography, no resolver from scratch, no Internet
  deployment and no security proof. Those are the boundaries.”

**What you must not do.** Do not invent a number because the sheet is out of
reach. Offer the repository and the evidence sheet instead; a missing number is
recoverable, an invented one is not.

## The failure matrix, inside any plan

| Symptom | Recovery | Say |
| --- | --- | --- |
| The laptop will not start, or the battery is flat | Move the drive to the second laptop or the organiser's machine. The kit needs only a shell, a browser and the bundled binary. | “The evidence is on the drive; let me switch machines.” |
| `serve.sh` cannot bind the port | Another process owns it. Run `sh serve.sh 8081` and open the printed address. | “One moment — a different port.” |
| The site will not load at all | Open `report/report.html` directly from the drive. It is self-contained: no server, no network fonts, no CDN scripts. | “The report is a single file; we don't need the site.” |
| The dashboard binary will not run on this machine | Use the browser dashboard at `/tui/` from `serve.sh`, or open the report and walk the same numbers there. | “The browser build is the same model; here it is.” |
| The browser has no working WebAssembly | Use the terminal dashboard, then the report. The report needs no WebAssembly. | “The report carries the same claims.” |
| The projector shows nothing | Switch to Plan C: the printed deck and the printed evidence sheet; keep the laptop screen for yourself. | “Let me talk you through the printed evidence.” |
| Wi-Fi or the venue network is down | Nothing in Plan A needs it. If a page tries to fetch, `cargo xtask demo --offline` from a checkout blocks every non-local request and proves it. | “This demonstration was designed to run offline.” |
| Docker is unavailable, or the live DNS pipeline will not start | Show the recorded capture in the dashboard's Live tab and the recorded RTT run. The live pipeline is optional; the measurement of record is the artifact. | “The live pipeline is optional; here is the recorded run.” |
| A number is challenged | Open `report/SHA256SUMS` and re-verify it, then open the claims registry in the report or the Integrity tab. Every claim carries its JSON pointer. | “Here is the claim and the pointer; the artifact is on the drive.” |
| The deck PDF will not open | Open [`PITCH.md`](PITCH.md) and the printed deck instead. The pitch carries the same argument without the layout. | “I have the same material in text.” |
| A file is missing from the drive | Do not improvise from memory. Use the second USB drive, or the copied report bundle. Then re-run `sh verify.sh`. | “Let me use the second drive and verify it.” |

## After the session

- Re-run `sh verify.sh` on both drives before they are packed away.
- If anything changed on a drive during the session, rebuild the kit from the
  source checkout with `cargo xtask kit` rather than editing files by hand.
- Regenerate the printed sheet with `cargo xtask evidence-sheet` rather than
  editing it, and check it with `cargo xtask evidence-sheet --check`.
- The packing checklist and restore steps are in [`KIT.md`](KIT.md). The
  questions and evidence-backed answers are in [`QA.md`](QA.md).
