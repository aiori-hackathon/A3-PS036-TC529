//! `cargo xtask feature-matrix`: compile the viewer-scope crates in every
//! feature configuration the CI replica has to survive.
//!
//! The viewer CI job has no native crypto toolchain, so it can only build the
//! `--no-default-features` configuration. A local checkout has the extracted
//! prefix, and a feature-gated lint that only appears when the default or
//! `--all-features` configuration is compiled used to escape that job. This
//! command makes the matrix explicit: every viewer-scope crate is checked,
//! linted and test-compiled under `none`, `default` and `all`, always with
//! `RUSTFLAGS=-D warnings`.
//!
//! A feature set that enables the `native` feature needs the libMTL prefix. In
//! a public export there is none, so those cells are reported as skipped, with
//! the reason, instead of being silently omitted. Crates without a `native`
//! feature still run all three sets: their sets are identical, and that is
//! cheap to prove.

use anyhow::Result;
use clap::Args;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::proc;

/// One crate in the viewer scope and whether it owns a `native` feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MatrixCrate {
    pub(crate) name: &'static str,
    pub(crate) has_native_feature: bool,
}

/// The crates the viewer job and its dependency tree compile. `climb-bench` is
/// included because `climb-report --no-default-features` depends on it, even
/// though the job never names it directly.
pub(crate) const VIEWER_CRATES: &[MatrixCrate] = &[
    MatrixCrate {
        name: "climb-dns",
        has_native_feature: false,
    },
    MatrixCrate {
        name: "climb-analytical",
        has_native_feature: false,
    },
    MatrixCrate {
        name: "climb-bench",
        has_native_feature: true,
    },
    MatrixCrate {
        name: "climb-report",
        has_native_feature: true,
    },
    MatrixCrate {
        name: "climb-tui",
        has_native_feature: true,
    },
    MatrixCrate {
        name: "xtask",
        has_native_feature: true,
    },
];

/// The cargo feature selection for one cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FeatureSet {
    None,
    Default,
    All,
}

impl FeatureSet {
    pub(crate) const ALL: [FeatureSet; 3] =
        [FeatureSet::None, FeatureSet::Default, FeatureSet::All];

    pub(crate) fn label(self) -> &'static str {
        match self {
            FeatureSet::None => "none",
            FeatureSet::Default => "default",
            FeatureSet::All => "all",
        }
    }

    /// Cargo arguments for this feature selection.
    pub(crate) fn cargo_args(self) -> Vec<&'static str> {
        match self {
            FeatureSet::None => vec!["--no-default-features"],
            FeatureSet::Default => Vec::new(),
            FeatureSet::All => vec!["--all-features"],
        }
    }

    /// Whether this selection activates a crate's `native` feature.
    pub(crate) fn enables_native(self, krate: MatrixCrate) -> bool {
        krate.has_native_feature && !matches!(self, FeatureSet::None)
    }
}

/// One command the matrix runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MatrixAction {
    Check,
    Clippy,
    TestNoRun,
}

impl MatrixAction {
    pub(crate) const ALL: [MatrixAction; 3] = [
        MatrixAction::Check,
        MatrixAction::Clippy,
        MatrixAction::TestNoRun,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            MatrixAction::Check => "check",
            MatrixAction::Clippy => "clippy",
            MatrixAction::TestNoRun => "test --no-run",
        }
    }

    /// The cargo subcommand name.
    pub(crate) fn subcommand(self) -> &'static str {
        match self {
            MatrixAction::Check => "check",
            MatrixAction::Clippy => "clippy",
            MatrixAction::TestNoRun => "test",
        }
    }

    /// Arguments that follow the crate and feature selection. Anything after
    /// `--` belongs to rustc, so it must come last.
    pub(crate) fn tail_args(self) -> Vec<&'static str> {
        match self {
            MatrixAction::Check => vec!["--all-targets"],
            MatrixAction::Clippy => vec!["--all-targets", "--", "-D", "warnings"],
            MatrixAction::TestNoRun => vec!["--no-run"],
        }
    }
}

/// One planned cell: a crate, a feature set and an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cell {
    pub(crate) krate: MatrixCrate,
    pub(crate) features: FeatureSet,
    pub(crate) action: MatrixAction,
}

impl Cell {
    /// The full cargo command line, for the printed table and the error text.
    pub(crate) fn command_line(self) -> String {
        let mut parts = vec!["cargo".to_string(), self.action.subcommand().to_string()];
        parts.push("-p".to_string());
        parts.push(self.krate.name.to_string());
        parts.extend(self.features.cargo_args().iter().map(|s| s.to_string()));
        parts.extend(self.action.tail_args().iter().map(|s| s.to_string()));
        parts.join(" ")
    }
}

/// Build the matrix. A cell that needs the native stack is omitted when the
/// prefix is absent; every omission is reported by the caller.
pub(crate) fn plan(native_available: bool, check_only: bool) -> Vec<Cell> {
    let actions: &[MatrixAction] = if check_only {
        &[MatrixAction::Check, MatrixAction::Clippy]
    } else {
        &MatrixAction::ALL
    };
    let mut cells = Vec::new();
    for krate in VIEWER_CRATES {
        for features in FeatureSet::ALL {
            if features.enables_native(*krate) && !native_available {
                continue;
            }
            for action in actions {
                cells.push(Cell {
                    krate: *krate,
                    features,
                    action: *action,
                });
            }
        }
    }
    cells
}

/// Cells omitted because they need a native prefix.
pub(crate) fn skipped(check_only: bool) -> Vec<Cell> {
    let actions: &[MatrixAction] = if check_only {
        &[MatrixAction::Check, MatrixAction::Clippy]
    } else {
        &MatrixAction::ALL
    };
    let mut cells = Vec::new();
    for krate in VIEWER_CRATES {
        for features in FeatureSet::ALL {
            if features.enables_native(*krate) {
                for action in actions {
                    cells.push(Cell {
                        krate: *krate,
                        features,
                        action: *action,
                    });
                }
            }
        }
    }
    cells
}

/// True when `prefix` holds the public libMTL header. This mirrors the build
/// script's lookup so the matrix agrees with what a build would resolve.
pub(crate) fn prefix_has_libmtl(prefix: &Path) -> bool {
    let include = prefix.join("include");
    include.join("mtllib").join("mtllib.h").is_file() || include.join("mtllib.h").is_file()
}

/// The prefix a native build would use, if any. Environment override first,
/// then the workspace's extracted toolchain, then the documented global paths.
pub(crate) fn native_prefix(root: &Path) -> Option<PathBuf> {
    if let Ok(prefix) = std::env::var("CLIMB_MTL_PREFIX") {
        let path = PathBuf::from(prefix);
        return prefix_has_libmtl(&path).then_some(path);
    }
    [
        root.join(".mtl-install"),
        PathBuf::from("/out"),
        PathBuf::from("/usr/local"),
    ]
    .into_iter()
    .find(|candidate| prefix_has_libmtl(candidate))
}

/// CLI arguments for `cargo xtask feature-matrix`.
#[derive(Args)]
pub(crate) struct FeatureMatrixArgs {
    /// Workspace root.
    #[arg(long, default_value = ".")]
    pub(crate) root: PathBuf,

    /// Run only `check` and `clippy` (the viewer job's subset).
    #[arg(long)]
    pub(crate) check_only: bool,
}

/// One cell's outcome, kept for the final summary.
struct Outcome {
    cell: Cell,
    passed: bool,
    elapsed: Duration,
    output: Option<String>,
}

/// `cargo xtask feature-matrix`.
pub(crate) fn cmd_feature_matrix(args: FeatureMatrixArgs) -> Result<()> {
    let root = &args.root;
    let prefix = native_prefix(root);
    let native_available = prefix.is_some();
    if let Some(prefix) = &prefix {
        println!(
            "feature-matrix: native prefix {} (default and all-features cells run)",
            prefix.display()
        );
    } else {
        println!(
            "feature-matrix: no libMTL prefix in this checkout; default and all-features cells \
             for crates with a native feature are skipped (the public viewer runner has none)"
        );
    }

    let cells = plan(native_available, args.check_only);
    println!(
        "feature-matrix: {} cell(s), actions: {}",
        cells.len(),
        if args.check_only {
            "check, clippy"
        } else {
            "check, clippy, test --no-run"
        }
    );

    let mut outcomes = Vec::with_capacity(cells.len());
    for cell in cells {
        let started = Instant::now();
        let mut command = Command::new("cargo");
        command
            .current_dir(root)
            .arg(cell.action.subcommand())
            .arg("-p")
            .arg(cell.krate.name)
            .args(cell.features.cargo_args())
            .args(cell.action.tail_args())
            .env("RUSTFLAGS", "-D warnings");
        let label = cell.command_line();
        let output = proc::run_raw(&mut command, Duration::from_secs(1800), &label);
        let elapsed = started.elapsed();
        match output {
            Ok(output) if output.status.success() => {
                println!(
                    "  PASS  {:<16} {:<8} {:<14} {:>6.1}s",
                    cell.krate.name,
                    cell.features.label(),
                    cell.action.label(),
                    elapsed.as_secs_f64()
                );
                outcomes.push(Outcome {
                    cell,
                    passed: true,
                    elapsed,
                    output: None,
                });
            }
            Ok(output) => {
                let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
                text.push_str(&String::from_utf8_lossy(&output.stderr));
                println!(
                    "  FAIL  {:<16} {:<8} {:<14} {:>6.1}s",
                    cell.krate.name,
                    cell.features.label(),
                    cell.action.label(),
                    elapsed.as_secs_f64()
                );
                outcomes.push(Outcome {
                    cell,
                    passed: false,
                    elapsed,
                    output: Some(text),
                });
            }
            Err(error) => {
                println!(
                    "  FAIL  {:<16} {:<8} {:<14} {:>6.1}s",
                    cell.krate.name,
                    cell.features.label(),
                    cell.action.label(),
                    elapsed.as_secs_f64()
                );
                outcomes.push(Outcome {
                    cell,
                    passed: false,
                    elapsed,
                    output: Some(format!("{error:#}")),
                });
            }
        }
    }

    let skipped = if native_available {
        Vec::new()
    } else {
        skipped(args.check_only)
    };
    for cell in &skipped {
        println!(
            "  SKIP  {:<16} {:<8} {:<14} needs the native crypto stack; no libMTL prefix",
            cell.krate.name,
            cell.features.label(),
            cell.action.label()
        );
    }

    let failed: Vec<&Outcome> = outcomes.iter().filter(|row| !row.passed).collect();
    if !failed.is_empty() {
        for row in &failed {
            eprintln!("\n--- {} failed ---", row.cell.command_line());
            if let Some(output) = &row.output {
                eprintln!("{output}");
            }
        }
        anyhow::bail!(
            "feature-matrix: {} cell(s) failed, {} passed, {} skipped",
            failed.len(),
            outcomes.len() - failed.len(),
            skipped.len()
        );
    }

    let total: Duration = outcomes.iter().map(|row| row.elapsed).sum();
    println!(
        "feature-matrix: {} passed, 0 failed, {} skipped in {:.1}s",
        outcomes.len(),
        skipped.len(),
        total.as_secs_f64()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_viewer_scope_names_every_crate_the_job_compiles() {
        let names: Vec<&str> = VIEWER_CRATES.iter().map(|k| k.name).collect();
        assert_eq!(
            names,
            vec![
                "climb-dns",
                "climb-analytical",
                "climb-bench",
                "climb-report",
                "climb-tui",
                "xtask"
            ]
        );
    }

    #[test]
    fn a_full_matrix_has_three_actions_per_crate_and_feature_set() {
        let cells = plan(true, false);
        assert_eq!(cells.len(), VIEWER_CRATES.len() * 3 * 3);
        assert_eq!(
            cells
                .iter()
                .filter(|c| c.features == FeatureSet::None)
                .count(),
            VIEWER_CRATES.len() * 3
        );
    }

    #[test]
    fn the_check_only_matrix_omits_test_compilation() {
        let cells = plan(true, true);
        assert_eq!(cells.len(), VIEWER_CRATES.len() * 3 * 2);
        assert!(!cells.iter().any(|c| c.action == MatrixAction::TestNoRun));
    }

    #[test]
    fn without_a_native_prefix_only_no_default_cells_for_native_crates() {
        let cells = plan(false, false);
        let native_names = ["climb-bench", "climb-report", "climb-tui", "xtask"];
        for cell in &cells {
            if native_names.contains(&cell.krate.name) {
                assert_eq!(
                    cell.features,
                    FeatureSet::None,
                    "{} must only run none",
                    cell.krate.name
                );
            }
        }
        // The two featureless crates still prove all three selections.
        for name in ["climb-dns", "climb-analytical"] {
            let sets: Vec<FeatureSet> = cells
                .iter()
                .filter(|c| c.krate.name == name && c.action == MatrixAction::Check)
                .map(|c| c.features)
                .collect();
            assert_eq!(sets, FeatureSet::ALL.to_vec());
        }
    }

    #[test]
    fn skipped_cells_name_only_native_feature_sets() {
        let skipped = skipped(false);
        assert!(!skipped.is_empty());
        assert!(skipped.iter().all(|c| c.features.enables_native(c.krate)));
        assert!(skipped.iter().any(|c| c.krate.name == "climb-report"));
        assert!(!skipped.iter().any(|c| c.krate.name == "climb-dns"));
    }

    #[test]
    fn a_feature_set_never_claims_native_for_a_featureless_crate() {
        let dns = VIEWER_CRATES[0];
        assert!(!FeatureSet::All.enables_native(dns));
        let report = VIEWER_CRATES[3];
        assert!(FeatureSet::All.enables_native(report));
        assert!(!FeatureSet::None.enables_native(report));
    }

    #[test]
    fn a_prefix_with_the_installed_header_is_recognised() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(!prefix_has_libmtl(dir.path()));
        let installed = dir.path().join("include").join("mtllib");
        std::fs::create_dir_all(&installed).expect("mkdir");
        std::fs::write(installed.join("mtllib.h"), "/* header */").expect("write");
        assert!(prefix_has_libmtl(dir.path()));
    }

    #[test]
    fn a_prefix_with_the_raw_header_is_recognised() {
        let dir = tempfile::tempdir().expect("tempdir");
        let include = dir.path().join("include");
        std::fs::create_dir_all(&include).expect("mkdir");
        std::fs::write(include.join("mtllib.h"), "/* header */").expect("write");
        assert!(prefix_has_libmtl(dir.path()));
    }

    #[test]
    fn every_action_uses_the_feature_flags_of_its_set() {
        let cell = Cell {
            krate: VIEWER_CRATES[0],
            features: FeatureSet::None,
            action: MatrixAction::Clippy,
        };
        assert_eq!(
            cell.command_line(),
            "cargo clippy -p climb-dns --no-default-features --all-targets -- -D warnings"
        );
    }
}
