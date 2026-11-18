//! `cargo xtask site-data` and `cargo xtask site-check`.
//!
//! `site-data` regenerates the seven committed build-time documents the
//! website reads. `site-check` re-renders them in memory and fails when the
//! committed copy is stale, runs the numeric-literal guard over `site/src` so a
//! typed benchmark number cannot reach the page, and then runs the brand, copy
//! and design checks in `scripts/site-check.py`.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::proc;

/// The rule script the gate runs after the Rust-side checks.
const SITE_CHECK_SCRIPT: &str = "scripts/site-check.py";

/// Arguments shared by the two site commands.
#[derive(Args)]
pub(crate) struct SiteArgs {
    /// Repository root the artifact paths are resolved against.
    #[arg(long, default_value = ".")]
    pub(crate) root: PathBuf,

    /// Generated-data directory, relative to the repository root.
    #[arg(long, default_value = climb_report::site_data::DEFAULT_OUT_DIR)]
    pub(crate) out: PathBuf,
}

/// Regenerate the site's build-time data documents.
pub(crate) fn cmd_site_data(args: SiteArgs) -> Result<()> {
    let out = args.root.join(&args.out);
    let written = climb_report::site_data::write(&args.root, &out)
        .context("failed to generate the site data documents")?;
    println!(
        "site-data: wrote {} document(s) to {}",
        written.len(),
        out.display()
    );
    Ok(())
}

/// Fail when the generated documents are stale or a page types a number.
pub(crate) fn cmd_site_check(args: SiteArgs) -> Result<()> {
    let out = args.root.join(&args.out);
    let stale = climb_report::site_data::stale_files(&args.root, &out)
        .context("failed to re-render the site data documents")?;
    if !stale.is_empty() {
        bail!(
            "site data is stale: {}. Run `cargo xtask site-data`.",
            stale.join(", ")
        );
    }

    let guard = args.root.join("site/src/lib/guard.ts");
    let target = args.root.join("site/src");
    let mut command = Command::new("node");
    command.arg(&guard).arg(&target);
    let output = proc::run_raw(
        &mut command,
        Duration::from_secs(120),
        "node site/src/lib/guard.ts",
    )
    .with_context(|| format!("failed to run the guard at {}", guard.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        print!("{stdout}");
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            eprint!("{stderr}");
        }
        bail!("site/src contains a typed benchmark number (see above)");
    }

    let self_checks = run_rule_script(&args.root)?;

    println!(
        "site-check: {} document(s) fresh; site/src has no typed benchmark number; \
         brand, copy and design rules pass ({self_checks} self-check(s))",
        climb_report::site_data::OUTPUT_FILES.len()
    );
    Ok(())
}

/// Run the brand, copy and design rules, and return their self-check count.
///
/// The script prints the contrast table, the copy scan, the document checks and
/// its own named self-checks; a non-zero exit is a gate failure. The count is
/// read from the script's own summary so the Rust side does not restate a
/// number the script owns.
fn run_rule_script(root: &Path) -> Result<usize> {
    let script = root.join(SITE_CHECK_SCRIPT);
    if !script.is_file() {
        bail!(
            "site-check: the rule script is missing at {}",
            script.display()
        );
    }
    let mut command = Command::new("python3");
    command.arg(&script).arg("all");
    let output = proc::run_raw(
        &mut command,
        Duration::from_secs(120),
        "python3 scripts/site-check.py all",
    )
    .with_context(|| format!("failed to run the rule script at {}", script.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        print!("{stdout}");
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            eprint!("{stderr}");
        }
        bail!("site-check: the brand, copy or design rules failed (see above)");
    }
    Ok(self_check_count(&stdout))
}

/// The number of named self-checks in the script's summary line, or 0 when the
/// line is absent (the exit status has already passed).
fn self_check_count(stdout: &str) -> usize {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("selftest: "))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|count| count.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .canonicalize()
            .expect("workspace root")
    }

    #[test]
    fn the_gate_runs_a_rule_script_that_exists() {
        let script = root().join(SITE_CHECK_SCRIPT);
        assert!(
            script.is_file(),
            "the site gate expects {}",
            script.display()
        );
    }

    #[test]
    fn the_rule_script_self_checks_pass_through_the_gate() {
        let checks = run_rule_script(&root()).expect("the rule script exits clean");
        assert!(
            checks >= 30,
            "expected at least 30 named self-checks, the script reported {checks}"
        );
    }

    #[test]
    fn the_self_check_count_comes_from_the_script_summary() {
        assert_eq!(self_check_count("selftest: 30 check(s), 0 failure(s)"), 30);
        assert_eq!(self_check_count("no summary here"), 0);
    }
}
