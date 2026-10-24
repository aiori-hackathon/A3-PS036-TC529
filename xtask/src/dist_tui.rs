//! The `dist-tui` subcommand: build a viewer-only release binary, strip it,
//! package it, and smoke-test the packaged binary in a minimal container.
//!
//! ```text
//! cargo xtask dist-tui
//! cargo xtask dist-tui --out dist --skip-smoke
//! ```
//!
//! The viewer links no native crypto stack, so the build runs with
//! `--no-default-features` and needs no `CLIMB_MTL_PREFIX`. A static musl
//! binary is preferred because it runs on any Linux distribution; when that
//! target is not installed the command falls back to the host gnu target and
//! says so. The tarball carries the binary, a `SHA256SUMS` manifest and a
//! `PROVENANCE.txt` derived from the checkout, using the same rendering as the
//! artifact bundle.
//!
//! The smoke test mounts the staged directory read-only into
//! `debian:stable-slim`, runs the binary under a pseudo-terminal with
//! `script -qec`, opens the guided tour, sends Esc to leave the tour and Esc
//! again to quit, and requires exit status 0.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::package::{
    capture, full_sha, render_provenance, render_sums, sha256_hex, submodule_shas, Provenance,
};

/// Workspace package that produces the viewer binary.
const CRATE: &str = "climb-tui";
/// The binary's filename inside the bundle and on `PATH` in the container.
const BINARY: &str = "climb-tui";
/// Preferred target: a static binary that runs on any Linux distribution.
pub(crate) const MUSL: &str = "x86_64-unknown-linux-musl";
/// Fallback target: the host's gnu toolchain.
pub(crate) const GNU: &str = "x86_64-unknown-linux-gnu";
/// Minimal image the smoke test runs in.
const SMOKE_IMAGE: &str = "debian:stable-slim";
/// Upper bound on the container smoke test, so a stuck terminal cannot hang
/// the packaging command forever.
const SMOKE_TIMEOUT_SECS: u32 = 60;

/// Arguments for `cargo xtask dist-tui`.
#[derive(Args)]
pub(crate) struct DistTuiArgs {
    /// Directory the tarball is written into (created when absent).
    #[arg(long, default_value = "dist")]
    pub(crate) out: PathBuf,

    /// Target triple. Defaults to musl when installed, otherwise gnu.
    #[arg(long)]
    pub(crate) target: Option<String>,

    /// Skip the container smoke test (the build and packaging still run).
    #[arg(long)]
    pub(crate) skip_smoke: bool,
}

/// Pick the target triple. An explicit request wins; otherwise musl is
/// preferred, and gnu is the documented fallback. The boolean reports whether
/// the choice is the portable musl target.
pub(crate) fn choose_target(explicit: Option<&str>, installed: &[String]) -> (String, bool) {
    if let Some(target) = explicit {
        return (target.to_string(), target.ends_with("-musl"));
    }
    if installed.iter().any(|target| target == MUSL) {
        (MUSL.to_string(), true)
    } else {
        (GNU.to_string(), false)
    }
}

/// The bundle's top-level directory name.
pub(crate) fn bundle_name(short_sha: &str, target: &str) -> String {
    format!("climb-tui-{short_sha}-{target}")
}

/// The tarball filename for a given short SHA and target.
pub(crate) fn tarball_name(short_sha: &str, target: &str) -> String {
    format!("{}.tar.gz", bundle_name(short_sha, target))
}

/// Every installed rustup target, or an empty list when rustup is unavailable.
fn installed_targets() -> Vec<String> {
    capture("rustup", &["target", "list", "--installed"])
        .map(|out| {
            out.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Build, strip, package and optionally smoke-test the viewer.
pub(crate) fn cmd_dist_tui(args: DistTuiArgs) -> Result<()> {
    let installed = installed_targets();
    let (target, musl) = choose_target(args.target.as_deref(), &installed);
    if !musl {
        eprintln!(
            "note: {MUSL} is not installed; packaging for {GNU} instead. \
             Run `rustup target add {MUSL}` for a portable static binary."
        );
    }
    if !installed.iter().any(|candidate| candidate == &target) {
        bail!("target {target} is not installed — run `rustup target add {target}`");
    }

    let status = Command::new("cargo")
        .args([
            "build",
            "-p",
            CRATE,
            "--release",
            "--no-default-features",
            "--target",
            &target,
        ])
        .status()
        .context("failed to run cargo build")?;
    if !status.success() {
        bail!("cargo build exited with {status}");
    }

    let built = Path::new("target")
        .join(&target)
        .join("release")
        .join(BINARY);
    if !built.is_file() {
        bail!("{} was not produced by the release build", built.display());
    }

    let short =
        capture("git", &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let bundle = bundle_name(&short, &target);
    let tarball = args.out.join(tarball_name(&short, &target));
    let staging = args.out.join(format!(".{bundle}.staging"));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .with_context(|| format!("failed to clear {}", staging.display()))?;
    }
    let root = staging.join(&bundle);
    std::fs::create_dir_all(&root)
        .with_context(|| format!("failed to create {}", root.display()))?;

    let staged = root.join(BINARY);
    std::fs::copy(&built, &staged)
        .with_context(|| format!("failed to copy {} to {}", built.display(), staged.display()))?;
    let stripped = Command::new("strip")
        .arg(&staged)
        .status()
        .context("failed to run strip")?;
    if !stripped.success() {
        bail!("strip exited with {stripped}");
    }

    let bytes =
        std::fs::read(&staged).with_context(|| format!("failed to read {}", staged.display()))?;
    let mut sums = BTreeMap::new();
    sums.insert(BINARY.to_string(), sha256_hex(&bytes));
    std::fs::write(root.join("SHA256SUMS"), render_sums(&sums))
        .with_context(|| format!("failed to write {}/SHA256SUMS", root.display()))?;

    let provenance = Provenance {
        commit: full_sha(),
        submodules: submodule_shas(),
        rustc: capture("rustc", &["--version"]).unwrap_or_else(|| "unknown".to_string()),
        host_fingerprint: "not recorded (viewer-only build)".to_string(),
        date: capture("date", &["--iso-8601=seconds"]).unwrap_or_else(|| "unknown".to_string()),
    };
    std::fs::write(root.join("PROVENANCE.txt"), render_provenance(&provenance))
        .with_context(|| format!("failed to write {}/PROVENANCE.txt", root.display()))?;

    if tarball.exists() {
        std::fs::remove_file(&tarball)
            .with_context(|| format!("failed to replace {}", tarball.display()))?;
    }
    let tar = Command::new("tar")
        .args(["-czf"])
        .arg(&tarball)
        .arg("-C")
        .arg(&staging)
        .arg(&bundle)
        .status()
        .context("failed to run tar")?;
    if !tar.success() {
        bail!("tar exited with {tar}");
    }

    let size = std::fs::metadata(&tarball)
        .map(|meta| meta.len())
        .unwrap_or_default();
    println!(
        "wrote {} ({BINARY} + SHA256SUMS + PROVENANCE.txt, {target}, {size} B, {:.2} MiB)",
        tarball.display(),
        size as f64 / (1024.0 * 1024.0),
    );

    if !args.skip_smoke {
        smoke(&staged)?;
    }

    std::fs::remove_dir_all(&staging)
        .with_context(|| format!("failed to clear {}", staging.display()))?;
    Ok(())
}

/// Run the packaged binary in a minimal container under a pseudo-terminal,
/// open the guided tour, and quit with two Esc presses.
fn smoke(binary: &Path) -> Result<()> {
    if capture("docker", &["--version"]).is_none() {
        bail!("docker is required for the smoke test — pass --skip-smoke to skip it");
    }
    let directory = binary
        .parent()
        .context("the staged binary has no parent directory")?;
    // Docker only accepts an absolute host path as a bind-mount source.
    let directory = std::fs::canonicalize(directory).with_context(|| {
        format!(
            "failed to resolve the absolute path of {}",
            directory.display()
        )
    })?;

    let mut child = Command::new("timeout")
        .arg(SMOKE_TIMEOUT_SECS.to_string())
        .args(["docker", "run", "--rm", "-i", "-v"])
        .arg(format!("{}:/bundle:ro", directory.display()))
        .args([
            SMOKE_IMAGE,
            "script",
            "-qec",
            "/bundle/climb-tui --tour",
            "/dev/null",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to start the container smoke test")?;

    let mut stdin = child
        .stdin
        .take()
        .context("smoke test stdin is not piped")?;
    std::thread::spawn(move || {
        // Leave the first frame on screen before the tour is interrupted, then
        // give the tour a moment to unwind before quitting the app.
        std::thread::sleep(Duration::from_secs(2));
        let _ = stdin.write_all(b"\x1b");
        let _ = stdin.flush();
        std::thread::sleep(Duration::from_secs(1));
        let _ = stdin.write_all(b"\x1b");
        let _ = stdin.flush();
    });

    let output = child
        .wait_with_output()
        .context("failed to wait for the container smoke test")?;
    if !output.status.success() {
        bail!(
            "container smoke test failed ({}) — the packaged binary did not open the \
             guided tour and exit cleanly\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
    println!("smoke: the packaged binary opened the guided tour and exited 0 in {SMOKE_IMAGE}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explicit_target_wins_over_the_installed_list() {
        let installed = vec![GNU.to_string(), MUSL.to_string()];
        assert_eq!(
            choose_target(Some("aarch64-unknown-linux-musl"), &installed),
            ("aarch64-unknown-linux-musl".to_string(), true)
        );
    }

    #[test]
    fn musl_is_preferred_when_installed() {
        let installed = vec![GNU.to_string(), MUSL.to_string()];
        assert_eq!(choose_target(None, &installed), (MUSL.to_string(), true));
    }

    #[test]
    fn gnu_is_the_fallback_when_musl_is_absent() {
        let installed = vec![GNU.to_string()];
        assert_eq!(choose_target(None, &installed), (GNU.to_string(), false));
    }

    #[test]
    fn an_explicit_gnu_target_is_not_reported_as_musl() {
        assert_eq!(
            choose_target(Some(GNU), &[MUSL.to_string()]),
            (GNU.to_string(), false)
        );
    }

    #[test]
    fn bundle_and_tarball_names_carry_the_sha_and_target() {
        assert_eq!(
            bundle_name("47a4a14", MUSL),
            "climb-tui-47a4a14-x86_64-unknown-linux-musl"
        );
        assert_eq!(
            tarball_name("47a4a14", GNU),
            "climb-tui-47a4a14-x86_64-unknown-linux-gnu.tar.gz"
        );
    }
}
