# Deployment and publishing

This note has two parts: how the public dashboard is hosted on a VPS, and why
the public repository's history differs from the development checkout.

Nothing in this repository deploys itself. Every step in the first part is run
by a human, in the order written, on a machine they control.

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

Sign in at duckdns.org, create one subdomain (for example `climb`), and copy
the account token from the same page. The subdomain becomes
`climb.duckdns.org`; the token is the credential for its updates.

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
bash /root/deploy/bootstrap.sh --domain climb --token-file /root/duckdns.token
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

### 7. Publish a build

Build on the development machine, precompress the files Caddy can serve
directly, and publish into a new release directory. `brotli` and `gzip` are
needed locally.

```sh
cd site
npm ci
npm run build
find dist -type f \
  \( -name '*.html' -o -name '*.css' -o -name '*.js' -o -name '*.json' \
     -o -name '*.svg' -o -name '*.wasm' -o -name '*.txt' -o -name '*.xml' \) \
  -exec brotli -q 11 -f -k {} \; \
  -exec gzip -9 -f -k {} \;
stamp="$(date -u +%Y%m%dT%H%M%SZ)"

ssh -i ~/.ssh/climb_ed25519 climb@<server-address> "mkdir -p /srv/climb/releases/$stamp"
rsync -a --delete -e "ssh -i ~/.ssh/climb_ed25519" \
  dist/ climb@<server-address>:/srv/climb/releases/$stamp/
ssh -t -i ~/.ssh/climb_ed25519 climb@<server-address> \
  "sudo ln -sfnT /srv/climb/releases/$stamp /srv/climb/current && sudo systemctl reload caddy"
```

`ln -sfnT` replaces the directory the bootstrap created with the symlink
rather than nesting inside it. The previous release stays in
`/srv/climb/releases/`; roll back by pointing `current` at it again and
reloading Caddy.

The Caddyfile serves precompressed `.br` and `.gz` siblings, falls back to
zstd or gzip on the fly, marks `/_astro/*` immutable for a year, revalidates
everything else, keeps `/tui/manifest.json` out of every cache, sets
`application/wasm`, and answers a missing path with `404.html`.

### 8. Enable HSTS

HSTS is deliberately absent from the first deployment: sending it before
HTTPS is known to work can lock a browser out of the site for a year. After
step 6 has reported a passing certificate and a `200` from
`https://climb.duckdns.org/`, enable it once:

```sh
sudo bash /root/deploy/bootstrap.sh --domain climb --hsts
```

The flag uncomments the single `Strict-Transport-Security` line in the
installed Caddyfile and reloads Caddy. `deploy/Caddyfile` keeps it commented,
so a fresh deployment always starts without it. The scripts are no longer
needed after this step:

```sh
sudo rm -rf /root/deploy
```

### Cost log

Record each invoice as it arrives. Amounts are entered when the provider
invoices; no price is assumed here.

| Date | Item | Amount | Currency | Notes |
|---|---|---|---|---|
| | VPS plan, India, 1 vCPU / 2 GB / 20 GB, monthly | | INR | first invoice <!-- docs-check:allow-number --> |
| | renewal, monthly | | INR | |
| | DuckDNS | 0 | — | free tier, no card |

### Testing the deployment scripts

The scripts are tested against a temporary root with stand-in commands, not
against the server:

```sh
bash deploy/tests/run.sh
```

The suite drives both the virtual-machine and container branches with a fake
`systemd-detect-virt`, checks that a container plan creates no swap file, no
time-sync unit and no sysctl file, asserts the token never appears in
arguments, output or the warning file, and runs `verify-server.sh` against a
healthy and a failing host. `shellcheck` is part of the suite when installed.

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
