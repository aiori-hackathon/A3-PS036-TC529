//! `cargo xtask workflows`: lint the GitHub Actions workflow files.
//!
//! Three lexical rules keep the workflows reviewable and the supply chain
//! pinned:
//!
//! 1. every `uses:` reference names a third-party action at a full
//!    40-character commit SHA, so moving a tag cannot change what runs;
//! 2. no workflow uses the `pull_request_target` event, which would run with a
//!    write token against code from a fork;
//! 3. no workflow reads a repository secret.
//!
//! Every workflow must also declare a top-level `permissions:` block, so the
//! token the run receives is named in the file rather than defaulted.
//!
//! The scan is line-based on purpose: the rules are lexical, so a small reader
//! reports the offending line without pulling a YAML parser into the tool and
//! without accepting a document the parser would silently normalise.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Args)]
pub(crate) struct WorkflowsArgs {
    /// Workspace root holding `.github/workflows`.
    #[arg(long, default_value = ".")]
    pub(crate) root: PathBuf,
}

/// One rule violation, with the file and line that carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Violation {
    /// Path of the workflow file, relative to the workspace root.
    pub(crate) file: String,
    /// One-based line number.
    pub(crate) line: usize,
    /// What is wrong with that line.
    pub(crate) message: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.file, self.line, self.message)
    }
}

/// The length of a full git commit SHA.
const FULL_SHA_LEN: usize = 40;

/// True when an action reference names a full commit SHA.
///
/// `owner/repo@<40 hex>` passes; `owner/repo@v4`, `owner/repo@main` and a local
/// `./path` reference do not.
pub(crate) fn is_pinned_action(value: &str) -> bool {
    let Some((name, reference)) = value.rsplit_once('@') else {
        return false;
    };
    !name.trim().is_empty()
        && reference.len() == FULL_SHA_LEN
        && reference.chars().all(|c| c.is_ascii_hexdigit())
}

/// Extract the `uses:` value of a line, without its trailing comment.
fn uses_value(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let content = trimmed.strip_prefix("- ").unwrap_or(trimmed);
    let rest = content.strip_prefix("uses:")?;
    let value = rest.split(" #").next().unwrap_or(rest).trim();
    Some(value.trim_matches(|c| c == '"' || c == '\''))
}

/// Lint one workflow document.
pub(crate) fn lint_workflow(file: &str, text: &str) -> Vec<Violation> {
    let mut violations = Vec::new();

    if !text.lines().any(|line| line.starts_with("permissions:")) {
        violations.push(Violation {
            file: file.to_string(),
            line: 1,
            message: "no top-level `permissions:` block, so the token scope is left to the default"
                .to_string(),
        });
    }

    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        if let Some(value) = uses_value(line) {
            if !is_pinned_action(value) {
                violations.push(Violation {
                    file: file.to_string(),
                    line: number,
                    message: format!(
                        "`uses: {value}` is not an action pinned to a {FULL_SHA_LEN}-character commit SHA"
                    ),
                });
            }
        }
        if line.contains("pull_request_target") {
            violations.push(Violation {
                file: file.to_string(),
                line: number,
                message: "the workflow uses the pull_request_target event".to_string(),
            });
        }
        if line.contains("secrets.") {
            violations.push(Violation {
                file: file.to_string(),
                line: number,
                message: "the workflow reads a repository secret".to_string(),
            });
        }
    }

    violations
}

/// Lint every workflow under `<root>/.github/workflows`, sorted by file name.
///
/// Returns the number of workflow files read and every violation found.
pub(crate) fn check_workflows(root: &Path) -> Result<(usize, Vec<Violation>)> {
    let dir = root.join(".github").join("workflows");
    let entries = fs::read_dir(&dir)
        .with_context(|| format!("failed to read the workflow directory {}", dir.display()))?;
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry =
            entry.with_context(|| format!("failed to read an entry of {}", dir.display()))?;
        let path = entry.path();
        let extension = path.extension().and_then(|value| value.to_str());
        if path.is_file() && matches!(extension, Some("yml") | Some("yaml")) {
            files.push(path);
        }
    }
    files.sort();

    let mut violations = Vec::new();
    for path in &files {
        let label = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string();
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        violations.extend(lint_workflow(&label, &text));
    }
    Ok((files.len(), violations))
}

/// The `workflows` subcommand.
pub(crate) fn cmd_workflows(args: WorkflowsArgs) -> Result<()> {
    let (files, violations) = check_workflows(&args.root)?;
    if violations.is_empty() {
        println!("workflows: {files} file(s) clean");
        return Ok(());
    }
    for violation in &violations {
        eprintln!("{violation}");
    }
    bail!(
        "workflows: {} violation(s) across {files} file(s)",
        violations.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLEAN: &str = "\
name: CI
on:
  push:
permissions:
  contents: read
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262
";

    #[test]
    fn a_tag_reference_is_a_violation() {
        let text = CLEAN.replace(
            "actions/checkout@11d5960a326750d5838078e36cf38b85af677262",
            "actions/checkout@v7",
        );
        let violations = lint_workflow(".github/workflows/ci.yml", &text);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].message.contains("commit SHA"),
            "{violations:?}"
        );
    }

    #[test]
    fn a_full_sha_reference_passes() {
        assert!(lint_workflow(".github/workflows/ci.yml", CLEAN).is_empty());
    }

    #[test]
    fn a_short_sha_reference_is_a_violation() {
        let text = CLEAN.replace("11d5960a326750d5838078e36cf38b85af677262", "11d5960a");
        let violations = lint_workflow(".github/workflows/ci.yml", &text);
        assert_eq!(violations.len(), 1, "{violations:?}");
    }

    #[test]
    fn a_local_action_reference_is_a_violation() {
        let text = CLEAN.replace(
            "actions/checkout@11d5960a326750d5838078e36cf38b85af677262",
            "./.github/actions/build",
        );
        let violations = lint_workflow(".github/workflows/ci.yml", &text);
        assert_eq!(violations.len(), 1, "{violations:?}");
    }

    #[test]
    fn a_commented_reference_is_not_scanned() {
        let text = format!("{CLEAN}# uses: actions/checkout@v7\n");
        assert!(
            lint_workflow(".github/workflows/ci.yml", &text).is_empty(),
            "a comment is not an action reference"
        );
    }

    #[test]
    fn pull_request_target_is_a_violation() {
        let text = CLEAN.replace("on:\n  push:", "on:\n  pull_request_target:");
        let violations = lint_workflow(".github/workflows/ci.yml", &text);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].message.contains("pull_request_target"),
            "{violations:?}"
        );
    }

    #[test]
    fn a_secret_reference_is_a_violation() {
        let text = format!("{CLEAN}      - run: echo \"${{ secrets.TOKEN }}\"\n");
        let violations = lint_workflow(".github/workflows/ci.yml", &text);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].message.contains("secret"), "{violations:?}");
    }

    #[test]
    fn a_workflow_without_top_level_permissions_is_a_violation() {
        let text = CLEAN.replace("permissions:\n  contents: read\n", "");
        let violations = lint_workflow(".github/workflows/ci.yml", &text);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].message.contains("permissions"),
            "{violations:?}"
        );
    }

    #[test]
    fn an_indented_permissions_block_is_not_a_top_level_declaration() {
        let text = CLEAN.replace("permissions:\n", "  permissions:\n");
        let violations = lint_workflow(".github/workflows/ci.yml", &text);
        assert!(
            violations.iter().any(|v| v.message.contains("permissions")),
            "{violations:?}"
        );
    }

    #[test]
    fn a_workflow_directory_with_no_files_reports_zero() {
        let dir = std::env::temp_dir().join(format!(
            "climb-workflows-empty-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".github/workflows")).expect("create the workflow directory");
        let (files, violations) = check_workflows(&dir).expect("the directory exists");
        assert_eq!(files, 0);
        assert!(violations.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_workflow_directory_is_an_error() {
        let dir = std::env::temp_dir().join(format!(
            "climb-workflows-missing-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let error = check_workflows(&dir).expect_err("a missing directory must fail");
        assert!(
            format!("{error:#}").contains("workflow directory"),
            "unexpected message: {error:#}"
        );
    }

    #[test]
    fn the_committed_workflows_are_clean() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let (files, violations) = check_workflows(root).expect("the workflow directory exists");
        assert!(files >= 2, "expected more than one workflow, found {files}");
        assert!(
            violations.is_empty(),
            "the committed workflows carry violations: {violations:?}"
        );
    }

    #[test]
    fn the_committed_workflow_set_matches_the_repository() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let (files, _) = check_workflows(root).expect("the workflow directory exists");
        for name in ["ci.yml", "security.yml"] {
            assert!(
                root.join(".github/workflows").join(name).is_file(),
                "{name} is missing from the workflow directory ({files} files)"
            );
        }
        assert!(
            root.join(".github/dependabot.yml").is_file(),
            "the Dependabot configuration is missing"
        );
    }
}
