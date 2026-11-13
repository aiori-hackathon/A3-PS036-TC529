//! `cargo xtask site-data` and `cargo xtask site-check`.
//!
//! `site-data` regenerates the six committed build-time documents the website
//! reads. `site-check` re-renders them in memory and fails when the committed
//! copy is stale, then runs the numeric-literal guard over `site/src` so a
//! typed benchmark number cannot reach the page.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::proc;

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

    println!(
        "site-check: {} document(s) fresh; site/src has no typed benchmark number",
        climb_report::site_data::OUTPUT_FILES.len()
    );
    Ok(())
}
