# Deployment and publishing

This note has two parts: how the public dashboard is hosted on a VPS, and why
the public repository's history differs from the development checkout.

Nothing in this repository deploys itself until a human has installed the
release receiver and added the deploy key. After that, a push that touches the
site publishes it: the workflow and the laptop run the same packaging script,
and the host verifies what it receives before anything becomes visible.

## Hosting the dashboard

The host serves one thing: the static dashboard build. It terminates TLS,
serves files from `/srv/climb/current`, and refreshes one DuckDNS record. It
runs no DNSSEC or post-quantum code, exposes no API and no shell, and installs
no Docker. The benchmark testbed stays on the development machine.

The scope was confirmed by the AIORI-3 organisers in writing on **2026-11-10**:
hosting the dashboard on a VPS behind a DuckDNS name is acceptable provided the
host is limited to dashboard access and is not an Internet deployment of the
DNSSEC/PQC implementation. `docs/COMPLIANCE.md` records the same reply beside
the requirement row it affects.

### 1. Buy the plan

Buy from aiccloud.in, India region, with these specifications and monthly
billing:

| Item | Value |
|---|---|
| Region | India |
| vCPU | 1 |
| Memory | 2 GB <!-- docs-check:allow-number --> |
| Disk | 20 GB <!-- docs-check:allow-number --> |
| Operating system | Ubuntu 24.04 LTS |
| Billing | monthly |

Choose the operating system template explicitly; the bootstrap refuses
anything that is not Ubuntu 24.04. Add the SSH public key during checkout if
the panel offers it, otherwise add it in step 3. Keep the invoice: the cost log
below is filled from it.

### 2. Test inside the refund window

Do this before configuring anything, while a refund is still possible. From
the development machine:

```sh
ssh root@<server-address> 'systemd-detect-virt; lscpu | sed -n "1,14p"; free -h; df -h /; nproc'
```

What to look for:

- `systemd-detect-virt` prints `kvm` for a virtual machine or `lxc` for a
  container. The bootstrap branches on this: a container has no swap of its
  own, no independent clock and no kernel to tune, so those steps are skipped
  and `deploy/verify-server.sh` says so.
- `lscpu` names one virtual CPU; a much larger count means the plan was not
  provisioned as ordered.
- `free -h` should show close to 2 GB. In a container the figure may be the <!-- docs-check:allow-number -->
  container limit rather than the host's memory; a limit well below 2 GB is a <!-- docs-check:allow-number -->
  reason to ask the provider before the window closes.
- `df -h /` should show about 20 GB, with several GB free after the OS image. <!-- docs-check:allow-number -->

If the numbers do not match the order, or the host is a container with
unusable networking, request the refund inside the window rather than working
around it. Record what the four commands printed in the deployment notes.

### 3. Add the SSH key

The bootstrap copies `/root/.ssh/authorized_keys` to the new sudo user before
it turns password and root logins off, so the key must be there first. Either
add it in the provider panel or:

```sh
ssh-copy-id -i ~/.ssh/climb_ed25519.pub root@<server-address>
ssh -i ~/.ssh/climb_ed25519 root@<server-address> 'wc -l /root/.ssh/authorized_keys'
```

A non-zero line count is the precondition for the next step. With no key the
bootstrap stops and changes nothing; it never disables password login without
a working key for the sudo user.

### 4. Create the DuckDNS name

Sign in at duckdns.org, create one subdomain (`climb-pqc` for this project), and
copy the account token from the same page. The subdomain becomes
`climb-pqc.duckdns.org`, which is the address the site is published at; the
token is the credential for its updates.

Keep the token in a password manager. It belongs in exactly one place on the
server, `/etc/climb/duckdns.env` (mode 0600), and never in this repository, a
shell history, or a chat message. The bootstrap reads it from a file or a
hidden prompt, never from an argument.

No DNS record is created by hand: the updater sets the A record to the
server's address on its first run and refreshes it every five minutes.

### 5. Run the bootstrap

Copy the `deploy/` directory to the server and run it as root. A container
without netfilter is expected to warn here and continue; that is not a failure.

```sh
scp -r deploy root@<server-address>:/root/
ssh -i ~/.ssh/climb_ed25519 root@<server-address>
umask 077
cat > /root/duckdns.token     # paste the token, then Ctrl-D
bash /root/deploy/bootstrap.sh --domain climb-pqc --token-file /root/duckdns.token
shred -u /root/duckdns.token
passwd climb                  # so sudo can authenticate; SSH passwords stay off
```

The script prints what it does and changes nothing twice. It:

1. installs the base packages, creates the sudo user `climb`, and copies the
   root key to that user;
2. writes `/etc/ssh/sshd_config.d/00-climb-hardening.conf` (key-only, no root
   login), validates it with `sshd -t`, and reloads sshd only after the
   validation passes;
3. sets ufw to deny incoming and allow only 22 (rate-limited), 80 and 443. In
   a container where netfilter is unavailable it prints a warning and the
   instruction to use the provider firewall panel, then keeps going;
4. installs fail2ban with an `sshd` jail and proves the ban action with
   `192.0.2.1`, then unbans it;
5. enables unattended security upgrades without automatic reboots;
6. caps journald at 200 MB; <!-- docs-check:allow-number -->
7. adds the Caddy apt repository and installs Caddy from it;
8. installs `/etc/caddy/Caddyfile` and the site address, and validates the
   configuration before restarting Caddy;
9. installs the DuckDNS updater, its timer, and the root-only environment
   file;
10. on a virtual machine only: creates a 1 GB swap file, enables time sync and <!-- docs-check:allow-number -->
    writes a small sysctl drop-in. On a container these are printed as skipped.

**Before closing the session**, open a second terminal and confirm the new
user can log in with the key:

```sh
ssh -t -i ~/.ssh/climb_ed25519 climb@<server-address> 'sudo -v && echo sudo-ok'
```

Only then close the root session. `--dry-run` prints the whole plan without
changing anything; use it if you want to see the steps first. Keep
`/root/deploy` until step 8 is done; it is removed there.

If the domain or token has to be added later, edit `/etc/climb/duckdns.env`
with `sudoedit` (mode 0600), re-run the bootstrap so Caddy learns the address,
then `sudo systemctl enable --now climb-duckdns.timer`.

### 6. Verify the host

```sh
sudo bash /root/deploy/verify-server.sh   # or from a fresh copy
```

It prints `PASS` or `FAIL` for the OS release, ufw, fail2ban, Caddy and the
certificate, followed by the virtualisation type, the listening ports, the
cgroup limits and every warning the bootstrap recorded. The exit status is
non-zero when any item failed.

Two outcomes are expected on a container and are not defects:

- **ufw `FAIL`** when the container has no netfilter. The compensating control
  is the provider firewall: allow only 22, 80 and 443 there. The same run
  prints the listening sockets so you can confirm nothing else is exposed.
- **certificate `FAIL`** until DuckDNS has propagated and Caddy has obtained
  the certificate. Caddy retries on its own; re-run the check after a minute
  or two. Inspect `journalctl -u caddy` if it stays red.

### 7. Install the release receiver

The host publishes releases through one command, run by one key, reachable only
through a forced command. Copy both release scripts to the host and install
them under the same names the repository uses, because the rollback script
loads its verification helpers from the receiver:

```sh
scp scripts/deploy-receive.sh scripts/rollback.sh \
  climb@<server-address>:/tmp/
ssh -t -i ~/.ssh/climb_ed25519 climb@<server-address>
sudo install -o root -g root -m 755 /tmp/deploy-receive.sh /usr/local/sbin/
sudo install -o root -g root -m 755 /tmp/rollback.sh /usr/local/sbin/
rm -f /tmp/deploy-receive.sh /tmp/rollback.sh
sha256sum /usr/local/sbin/deploy-receive.sh /usr/local/sbin/rollback.sh
```

Compare those digests with `sha256sum scripts/deploy-receive.sh
scripts/rollback.sh` on the development machine. Re-install both whenever the
repository copies change; a release refuses to publish if a tool it needs is
missing, but it cannot tell that it is out of date.

Create the account the key will belong to. It has no password and no shell:

```sh
sudo adduser --disabled-password --gecos "" --shell /usr/sbin/nologin deploy
sudo install -d -m 700 -o deploy -g deploy ~deploy/.ssh
```

On the development machine, make a key for deployments only (never reuse the
personal login key):

```sh
ssh-keygen -t ed25519 -N '' -C climb-deploy -f ~/.ssh/climb_deploy_ed25519
cat ~/.ssh/climb_deploy_ed25519.pub
```

Append that public key to `~deploy/.ssh/authorized_keys` on the host with
a forced command and every forwarding option disabled:

```sh
key='ssh-ed25519 AAAA... climb-deploy'   # the line printed above
sudo tee -a ~deploy/.ssh/authorized_keys >/dev/null <<EOF
restrict,command="/usr/bin/sudo -n /usr/local/sbin/deploy-receive.sh" $key
EOF
sudo chown deploy:deploy ~deploy/.ssh/authorized_keys
sudo chmod 600 ~deploy/.ssh/authorized_keys
```

`restrict` turns off the pty, port, agent and X11 forwarding, and the forced
command replaces whatever the client asks for, so the key cannot open a shell
or run anything else. The sudoers file grants the receiver and nothing beside
it, and keeps the one environment variable that carries the request: sudo
resets the environment, so without this line `SSH_ORIGINAL_COMMAND` never
reaches the script and every publish is refused.

```sh
sudo tee /etc/sudoers.d/climb-deploy >/dev/null <<'EOF'
Defaults:deploy env_keep += "SSH_ORIGINAL_COMMAND"
deploy ALL=(root) NOPASSWD: /usr/local/sbin/deploy-receive.sh
EOF
sudo chmod 440 /etc/sudoers.d/climb-deploy
sudo visudo -c -f /etc/sudoers.d/climb-deploy
```

Finally pin the host key. `ssh-keyscan` is trusted only after its fingerprint
has been compared with the one the provider shows in its console:

```sh
ssh-keyscan -p 22 <server-address> > ~/.ssh/climb_deploy_known_hosts
ssh-keygen -lf ~/.ssh/climb_deploy_known_hosts
```

The publishing script refuses to connect without that file and never accepts
an unknown key, so a machine that answers in the host's place cannot receive a
release.

### 8. Publish a release

One script packages and ships, from the laptop or from CI:

```sh
scripts/deploy-local.sh --host <server-address>
```

It builds the dashboard with `trunk build --release`, builds the site, writes a
brotli and a gzip sibling for every compressible file, records the release in
`site/release.json`, hashes every file into `manifest.json`, hashes the
manifest into `manifest.sha256`, packs the three into `climb-site-<stamp>-<sha>.tar.zst`,
and prints the archive digest before it sends anything. `--package-only` stops
after the archive, `--dry-run` prints the SSH command it would run, and
`--allow-dirty` packages an uncommitted tree (refused by default, because the
release names a commit).

The receiver, on the host, then:

1. refuses an archive whose digest is not the one the sender pinned;
2. refuses a path that is absolute, contains `..`, or is not under `site/`;
3. checks `manifest.sha256` against `manifest.json`;
4. checks every listed file's length and digest, and refuses any file the
   manifest does not list or any listed file that is missing;
5. checks the module's first four bytes against the WebAssembly magic;
6. moves the tree into `/srv/climb/releases/<release>` and replaces
   `/srv/climb/current` with one rename, so no reader sees a half-written
   site;
7. reloads Caddy, then asks `/release.json` which release it is serving, and
   reloads until the answer is the new one or the attempts run out;
8. on any failure restores the previous symlink, reloads, and keeps the failed
   tree as `<release>.failed`;
9. deletes releases beyond the newest five.

The layout is one directory per release and one symlink:

```text
/srv/climb/releases/20261115T041500Z-672e2d1/
/srv/climb/current -> /srv/climb/releases/20261115T041500Z-672e2d1
```

The Caddyfile serves `current`, so a release is live the moment the symlink
moves. It serves the precompressed `.br` and `.gz` siblings, falls back to zstd
or gzip on the fly, marks `/_astro/*` immutable for a year, revalidates
everything else, keeps `/tui/manifest.json` and `/release.json` out of every
cache, sets `application/wasm`, and answers a missing path with `404.html`.

**From CI.** `.github/workflows/deploy.yml` runs the same script after its
gates, on pushes to `main` that touch `site/`, `crates/climb-web/` or
`docs/sample-output/`, and on a manual dispatch. It needs these repository
settings:

| Kind | Name | Value |
|---|---|---|
| Variable | `DEPLOY_HOST` | the server address |
| Variable | `DEPLOY_USER` | `deploy` |
| Variable | `DEPLOY_URL` | `https://climb-pqc.duckdns.org` |
| Secret | `DEPLOY_SSH_KEY` | the whole private key file |
| Secret | `DEPLOY_KNOWN_HOSTS` | the whole pinned host key line |

The workflow refuses to run outside the public repository, so a fork cannot
deploy even if it is handed a key. It builds the module, checks the generated
site data, builds the site, runs lint, typecheck and the unit tests, runs
Lighthouse against the built pages, runs the release-script tests, publishes,
and then runs the live smoke test. A red run after the publish step means the
release is live but the public path failed a check; the previous release is one
rollback away.

### 9. Roll back

```sh
ssh -t -i ~/.ssh/climb_ed25519 climb@<server-address>
sudo /usr/local/sbin/rollback.sh --list
sudo /usr/local/sbin/rollback.sh previous
```

`--list` prints the releases with a star on the current one. `previous` is the
release the current one replaced; a release id works as well. The target is
verified with the same rules a new release is, so a damaged tree is refused
rather than served, and `current` is left where it was.

### 10. Prove the live site

```sh
scripts/smoke-live.sh
```

It checks the certificate chain and that the leaf does not expire inside a
fortnight, that plain HTTP redirects to HTTPS, that the response arrives over
HTTP/2 or HTTP/3, the security headers, every route with its expected title,
the module's media type, compression and magic bytes, that no request on the
home page or the dashboard leaves the site's origin, and a response-time
budget. Each line prints `PASS`, `FAIL` or `SKIP` with the value it saw;
`--require-hsts` adds HSTS to the required headers, `--budget-ms` raises the
time-to-first-byte budget, and `--skip-browser` turns off the origin check
when the site's Playwright package is not installed.

### 11. Enable HSTS

HSTS is deliberately absent from the first deployment: sending it before
HTTPS is known to work can lock a browser out of the site for a year. After
step 6 has reported a passing certificate and a `200` from
`https://climb-pqc.duckdns.org/`, enable it once:

```sh
sudo bash /root/deploy/bootstrap.sh --domain climb-pqc --hsts
```

The flag uncomments the single `Strict-Transport-Security` line in the
installed Caddyfile and reloads Caddy. `deploy/Caddyfile` keeps it commented,
so a fresh deployment always starts without it. The scripts are no longer
needed after this step:

```sh
sudo rm -rf /root/deploy
```

### The free fallback: Cloudflare Pages

The VPS is the primary target because Caddy serves the precompressed
siblings and the exact header set the smoke test checks, and because the
benchmark numbers travel with a host this project controls. Cloudflare Pages is
the free fallback: the same `site/dist` directory at the edge, with no server to
keep patched.

Set the repository variable `CLOUDFLARE_PAGES_PROJECT` to the Pages project
name and add `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` as secrets; the
last step of `.github/workflows/deploy.yml` then publishes the same build with
`wrangler pages deploy`. With the variable unset the step does not run.

What the fallback does not carry: `precompressed` and the shipped header set
belong to Caddy, and the site ships no `_headers` file, so a Pages deployment
answers without that policy until one is added. The certificate, HTTP version
and cache behaviour are Cloudflare's settings rather than this repository's.
Check it directly with `scripts/smoke-live.sh https://<project>.pages.dev`; the
smoke script's default target is the VPS. This is documented as a fallback, not
as an equal second target.

### Cost log

Record each invoice as it arrives. Amounts are entered when the provider
invoices; no price is assumed here.

| Date | Item | Amount | Currency | Notes |
|---|---|---|---|---|
| | VPS plan, India, 1 vCPU / 2 GB / 20 GB, monthly | | INR | first invoice <!-- docs-check:allow-number --> |
| | renewal, monthly | | INR | |
| | DuckDNS | 0 | — | free tier, no card |

### Testing the deployment scripts

The scripts are tested against temporary directories with stand-in commands,
never against the server:

```sh
bash deploy/tests/run.sh
bash deploy/tests/release.sh
```

`run.sh` drives both the virtual-machine and container branches with a fake
`systemd-detect-virt`, checks that a container plan creates no swap file, no
time-sync unit and no sysctl file, asserts the token never appears in
arguments, output or the warning file, and runs `verify-server.sh` against a
healthy and a failing host.

`release.sh` packages a fixture site and publishes it through the receiver, then
checks the refusals: a wrong archive digest, a corrupted manifest, a manifest
that names another release, a tampered file, an unlisted file, a module without
the WebAssembly magic, an archive with a parent-directory path, and a failing
health probe that has to restore the previous release. It also checks pruning,
`rollback.sh` backwards and forwards, and that the smoke script expects the
titles the site actually renders. `shellcheck` is part of both suites when it
is installed.

## Why the two repositories disagree about hashes

CLIMB has two repositories: a development checkout and the public submission
copy. This part explains why their commit hashes differ and which checks are
therefore based on content rather than on history.

The public repository is not a mirror. A publishing sync rebuilds it from the
development history, removing private-only paths from every commit and
redacting references before the tree is published. Removing a file changes
every tree that contained it, and a tree change changes its commit id; a
redacted commit message changes the id as well. Rebasing or filtering also
changes parent links and committer metadata. The result is a deterministic
public history in which every commit has a different hash from its private
counterpart, even though the published files are the same.

A build recorded in the development checkout therefore stamps a commit that a
public checkout cannot contain. A shallow clone has the same problem for older
commits: it holds only the requested depth, so an older recorded commit may be
absent even when the full public history contains it. A source tarball has no
`.git` directory at all and cannot resolve any commit.

## Which checks are content-based

The following checks never require a recorded commit to exist:

- **sample-output provenance** (`cargo xtask sample-check`) verifies the
  per-file digests in `docs/sample-output/SHA256SUMS` and a content identity:
  the SHA-256 of that manifest sorted by file name. The identity changes when a
  file, a digest or the set of files changes, and it is identical in every
  rewritten copy. The recorded commit is verified only when it is present; an
  absent commit prints a reason and the check continues successfully.
- **documentation checks** (`cargo xtask docs-check`) call the same provenance
  verification in its lenient mode, so the documentation gate passes in a
  public copy and in CI.
- **supply-chain pins** (`cargo xtask check-pins`) compare the Dockerfile
  comments with the submodule pins recorded in the commit graph and with
  `environment.json`. Submodule commits are public and stable, so they survive
  the rewrite. When there is no git checkout, the checkout comparison is a
  printed skip and the recorded pins are still checked.
- **the commit graph** is regenerated from its public as-of commit, which is a
  stable public hash. A shallow clone or a tarball without `.git` skips the
  history comparison with a printed reason; the document and its cards are
  still structurally checked.

The strict mode (`cargo xtask sample-check --require-commit`, or
`CLIMB_REPO_ROLE=private`) exists for the development checkout's own tooling.
CI never enables it.

## Publishing

`scripts/publish.sh` performs the whole cycle: it runs the viewer workflow in a
public-export replica, pushes the development commits, runs the sync, refreshes
the commit graph when the public tip moved, publishes the refresh commit, and
then requires the graph to agree with the public commit count. It refuses to
publish when any step fails.

`scripts/ci-local.sh --public` is the replica gate. It asks the sync for a
dry-run export (no push), rebuilds that tree as a fresh single-commit
repository so its hashes differ exactly as they do on GitHub, and runs the
viewer workflow's steps there. The default `scripts/ci-local.sh` keeps the
lighter worktree rehearsal for day-to-day work.

The workflow's `Environment report` step prints the checked-out commit, whether
the clone is shallow and which sample-check mode ran, so a future failure
explains its own history conditions.

## CI jobs

`.github/workflows/ci.yml` has two jobs, and the second waits for the first.
Every `run:` step is a call into `scripts/ci/`, so the workflow file stays a
list of names and the commands have one home.

| Job | What a green run proves | Approximate duration | Rehearse locally |
|---|---|---|---|
| `viewer` (no native libraries) | the viewer-scope crates are formatted, lint clean under `-D warnings`, build in every feature combination the matrix covers, and pass their tests; the committed sample pair is self-consistent; the viewer build links no native library; source hygiene, supply-chain pins, Mermaid, relative links and the workflow files themselves are clean | five minutes | `bash scripts/ci-local.sh` for the working tree, or `bash scripts/ci-local.sh --public` for the published export (the gate the Dependabot and history rewrites make different from a plain checkout) |
| `native` (pinned Docker toolchain) | the pinned libMTL/liboqs toolchain builds from the recipe, the documented system packages are enough to compile the workspace, the whole workspace test suite passes, the documentation agrees with the quote sheet and the artifacts, every published number re-derives from its raw rows, and the audit pack's gates pass | nine minutes, after the viewer job | `bash scripts/ci-local.sh --native` (runs the job's steps in a digest-pinned `ubuntu:24.04` container shaped like the runner; see `docs/CI-NATIVE-REHEARSAL.md`) |

Both jobs are bounded: the job and every step carry `timeout-minutes`, the
workflow cancels a superseded run on the same ref, the token is `contents:
read`, and a failing job uploads its logs as an artifact. The `native` job runs
on every push and pull request today; it is not a manual or on-demand job. The
other workflows are small by comparison: `security.yml` runs `cargo audit` and
`cargo deny check` (rehearse with those two commands), `site.yml` runs the site's
lint, typecheck, unit tests and build (`cd site && npm ci && npm run check`), and
`deploy.yml` is described in the next section.

## The private repository

The development repository (`profm0r14rty/CLIMB`) carries the same four workflow
files and runs the same jobs; the public repository is built from it by the
publishing sync. What differs:

- **Secrets.** `ci.yml`, `security.yml` and `site.yml` read no secret at all.
  `deploy.yml` reads `DEPLOY_SSH_KEY`, `DEPLOY_KNOWN_HOSTS` and optionally the
  two Cloudflare secrets, and it is the only workflow allowed to; its job
  condition also restricts it to the public repository, so it is skipped — not
  failed — in the private one.
- **Submodule access.** The pinned `third_party/MTL` fork is a public
  repository, so `submodules: true` needs no extra credential in either
  repository. A private submodule would need a token in the checkout step.
- **Minutes.** Runs in the private repository draw on the account's included
  GitHub Actions minutes; runs in the public repository are free. One push
  costs roughly fifteen minutes of runner time (the CI run, plus the site and
  security workflows).
- **Checkout contents.** The private checkout carries the planning files that
  the publishing sync withholds, so `docs-check`'s path/identity hygiene scans
  them there. A path recorded from a runner home directory in one of those
  files fails the private `native` job while the public export, which does not
  carry the file, passes.
- **Dependabot.** `.github/dependabot.yml` is private-only: the sync removes it
  from every published commit, so the public repository has no Dependabot
  configuration, no Dependabot branch and no Dependabot pull request. The
  workflow lint treats its absence in a published checkout as the expected
  state.

To stop the private repository running the workflows, the one setting is
**Settings → Actions → General → Actions permissions → Disable actions**
(`gh api -X PUT repos/profm0r14rty/CLIMB/actions/permissions -F enabled=false`).
The public repository's runs are the ones the badge and the submission rely on.

## Runner images, Node runtimes and the local CI replica

Every workflow pins a concrete runner image (`ubuntu-24.04`) instead of the
moving `ubuntu-latest` label, so a runner migration cannot silently change the
compiler, linker or container versions a green run was measured on. The
`workflows` lint and its tests reject `ubuntu-latest` in `ci.yml` and
`site.yml`.

GitHub is retiring the Node 20 runtime for JavaScript actions. Runs 44216360738
and 44216360854 carried the deprecation annotation for the three actions that
still declared `using: node20`; it does not fail a job, but the runtime is
already forced to Node 24 and the annotations are noise on every run. The pins
now all declare Node 24:

| Action and pin | Runtime in `action.yml` |
|---|---|
| `actions/checkout` `v7.0.1` | `node24` |
| `actions/cache` `v6.1.0` (`restore` and `save`) | `node24` |
| `actions/upload-artifact` `v7.0.2` | `node24` |
| `actions/setup-node` `v7.1.0` | `node24` |
| `Swatinem/rust-cache` `v2.9.3` | `node24` |
| `actions-rust-lang/setup-rust-toolchain` `v1.9.0` | composite (no JavaScript runtime) |

The runtime is read from each repository's `action.yml` at the pinned commit,
and the version comment on the `uses:` line names the release that commit
carries. A bump must replace the full forty-character commit SHA in every
workflow that uses the action (the `workflows` lint rejects a tag), then rerun
both CI jobs before the new pins are trusted. `trunk` and `wrangler` are
installed by `cargo` and `npx`, not by an action, so they have no Node-action
runtime of their own.

`scripts/ci-local.sh` reproduces the toolchain action's environment instead of
inheriting whatever the developer's shell has. It reads the workflow's explicit
`rustflags: "-D warnings"` input, exports `RUSTFLAGS` only when it is unset
(exactly as the action does), and sets the action's other defaults
(`CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_TERM_COLOR=always`,
`RUST_BACKTRACE=short`, `CARGO_UNSTABLE_SPARSE_REGISTRY=true`,
`CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse`). A workflow test fails when the
action's variable list and the replica's export block drift apart. After the
parsed viewer steps, the replica runs the full feature matrix; the viewer job
runs the check-and-clippy subset because its runner has no native crypto
toolchain.

## When the deploy workflow does nothing

`deploy.yml` is a no-op until a host is configured. Its job condition is:

```yaml
if: github.repository == 'aiori-hackathon/A3-PS036-TC529' && vars.DEPLOY_HOST != ''
```

An empty or unset `DEPLOY_HOST` therefore **skips** the job instead of failing
it, which is the correct state for a fresh fork, the private repository, or a
public copy whose host has not been set up. The Cloudflare Pages fallback is
gated separately on `vars.CLOUDFLARE_PAGES_PROJECT`. The `workflows` tests pin
both conditions, so removing the guard is a test failure rather than a surprise
red run.
