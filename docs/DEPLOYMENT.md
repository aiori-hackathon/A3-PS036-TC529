# Deployment and publishing

CLIMB has two repositories: a development checkout and the public submission
copy. This note explains why their commit hashes differ and which checks are
therefore based on content rather than on history.

## Why the two repositories disagree about hashes

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
