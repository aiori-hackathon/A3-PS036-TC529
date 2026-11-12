# Site information architecture

The route map, the navigation order, the footer and the 404. Copy for each
route lives in `docs/site/copy.md`, keyed by the same path.

## 1. Routes

Nine routes. Each one has a single job; if a route cannot state its job in one
sentence it is two routes.

| Path | Title | Job | Primary action | Evidence shown |
| --- | --- | --- | --- | --- |
| `/` | CLIMB — post-quantum DNSSEC, measured | Say what was measured, in one headline and four figures, and route to the evidence. | Open the dashboard | The four headline facts |
| `/dashboard` | Dashboard | The client-side dashboard: browse every recorded result interactively, with its evidence class. | Switch dataset | Wire, signing, verifying, memory, UDP fit, live DNS |
| `/calculator` | Break-even calculator | Let a reader put their own message count and response profile against the recorded per-message costs. | Compute | Wire totals, the break-even crossings, the UDP-fit window |
| `/report` | Report | The full HTML report and the results artifact, rendered and downloadable. | Download the artifact | Every recorded fact |
| `/verify` | Verify | Show the claims registry: each claim, its pointer, its value and its pass mark. | Re-run the check locally | The claims count and a worked example |
| `/reproduce` | Reproduce | The exact commands and the pinned environment that rebuild the artifacts. | Copy the command | Environment fingerprint, dataset count |
| `/method` | Method | Definitions: baselines A/B/C, the four evidence classes, the sweep, and the limits. | Read the limits | Parameter set, model constants |
| `/team` | Team | Who built it, the roles and the event. | — | None |
| `/commits` | Commits | The public commit graph. | — | None |

### Removed and added routes

- **Removed: `/lab`.** The prototype's laboratory route is cut; its content is
  either promoted into `/method` (definitions) or `/dashboard` (the
  interactive view), so no capability is lost.
- **Removed: the terminal route.** The site does not ship a terminal-style
  route. The terminal dashboard is a local binary
  (`cargo run -p climb-tui`), documented on `/reproduce`; the browser
  equivalent is `/dashboard`. The removed path is treated as `/tui`; if the
  earlier plan used a different path, the redirect below follows the real one.
- **Added: `/dashboard`.** The client-side dashboard, which is the browser
  counterpart of the terminal dashboard and the primary interactive surface.

A removed path serves a 301 to its replacement (`/lab` → `/method`, `/tui` →
`/dashboard`). No path 404s because of the change.

## 2. Navigation

The header carries the wordmark, the primary navigation and one action. The
primary order is the reading order of the argument: what it is, see it, use
it, check it, understand it.

| Order | Label | Path | Group |
| --- | --- | --- | --- |
| 1 | Home | `/` | Primary |
| 2 | Dashboard | `/dashboard` | Primary |
| 3 | Calculator | `/calculator` | Primary |
| 4 | Report | `/report` | Primary |
| 5 | Verify | `/verify` | Primary |
| 6 | Method | `/method` | Primary |
| — | (action) | Open the repository | Action (external, new tab) |

Secondary links — Reproduce, Team, Commits — live in the footer, not the
header, so the header stays under six items at every width. On a narrow
viewport the primary set collapses into a menu button labelled
`Menu` that expands to the same order; the action stays visible.

The current route is marked with `aria-current="page"` and a visible rule
under the label. There is a skip link as the first focusable element:
`Skip to content`.

## 3. Footer

Four regions, in this order, on every route.

1. **Identity line.** The wordmark and the tagline `Measured, not claimed.`
2. **Scope line.** The scope sentence, verbatim from `docs/site/copy.md`
   (`/footer/scope`): the site presents recorded results and a client-side
   dashboard; the benchmark itself is not deployed on the Internet; the DNS
   testbed runs locally.
3. **Link columns.**
   - *Project*: Dashboard, Calculator, Report, Verify.
   - *Method*: Method, Reproduce, Commits.
   - *Event*: AIORI-3 Hackathon, problem statement PS-036, team A3-PS036-TC529.
4. **Legal line.** `MIT licence`, a link to the repository's `LICENSE`, and
   the line `Results are empirical measurements; nothing here is a formal
   verification or a certification.`

## 4. 404

The 404 is a route like any other: it uses the same header and footer, and its
copy is in `docs/site/copy.md` (`/404`). It offers the primary navigation and
one action back to the dashboard. It never shows a stack trace, a framework
default or a search box that cannot search.

## 5. Accessibility and structure

- One `<h1>` per route; headings descend without skipping a level.
- Landmarks: `header`, `nav` (labelled), `main`, `footer`. The skip link
  targets `main`.
- Every route is reachable by keyboard in the header order above; the ladder
  and chart components follow the interaction rules in
  `docs/site/design-rules.md`.
- The dashboard's interactive controls (dataset switch, view toggle) are
  native controls, not click handlers on `<div>`.
- No route depends on a third-party origin: fonts, scripts and data are
  self-hosted or read from the committed artifact.
