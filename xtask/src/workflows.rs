//! `cargo xtask workflows`: lint the GitHub Actions workflow files.
//!
//! Three lexical rules keep the workflows reviewable and the supply chain
//! pinned:
//!
//! 1. every `uses:` reference names a third-party action at a full
//!    40-character commit SHA, so moving a tag cannot change what runs;
//! 2. no workflow uses the `pull_request_target` event, which would run with a
//!    write token against code from a fork;
//! 3. no workflow reads a repository secret, except the deploy workflow, which
//!    may read the names on its own allowlist (the deploy key and the optional
//!    Cloudflare token) and nothing else.
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

/// The only workflow allowed to read repository secrets, and the only names it
/// may read. Publishing needs a deploy key; the optional second target needs a
/// Cloudflare token. Every other workflow, and every other name in this one, is
/// a violation, so a new secret cannot be wired in without being written here
/// first.
const DEPLOY_WORKFLOW: &str = ".github/workflows/deploy.yml";
const DEPLOY_SECRETS: &[&str] = &[
    "DEPLOY_SSH_KEY",
    "DEPLOY_KNOWN_HOSTS",
    "CLOUDFLARE_API_TOKEN",
    "CLOUDFLARE_ACCOUNT_ID",
];

/// True when the file is the deploy workflow.
fn is_deploy_workflow(file: &str) -> bool {
    file == DEPLOY_WORKFLOW || file.ends_with("/deploy.yml")
}

/// Every `secrets.<NAME>` reference on a line, in order of appearance.
fn secret_references(line: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = line;
    while let Some(index) = rest.find("secrets.") {
        rest = &rest[index + "secrets.".len()..];
        let name: String = rest
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect();
        if !name.is_empty() {
            names.push(name);
        }
    }
    names
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
        for name in secret_references(line) {
            if is_deploy_workflow(file) && DEPLOY_SECRETS.contains(&name.as_str()) {
                continue;
            }
            let message = if is_deploy_workflow(file) {
                format!(
                    "`secrets.{name}` is not on the deploy allowlist, so the token scope has to be widened deliberately"
                )
            } else {
                format!(
                    "the workflow reads `secrets.{name}`, which only the deploy workflow may do"
                )
            };
            violations.push(Violation {
                file: file.to_string(),
                line: number,
                message,
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
    fn the_deploy_workflow_may_read_its_allowlisted_secrets() {
        for name in DEPLOY_SECRETS {
            let text = format!("{CLEAN}      - run: echo \"${{{{ secrets.{name} }}}}\"\n");
            let violations = lint_workflow(DEPLOY_WORKFLOW, &text);
            assert!(violations.is_empty(), "{name}: {violations:?}");
        }
    }

    #[test]
    fn the_deploy_workflow_may_not_read_another_secret() {
        let text = format!("{CLEAN}      - run: echo \"${{{{ secrets.AWS_KEY }}}}\"\n");
        let violations = lint_workflow(DEPLOY_WORKFLOW, &text);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].message.contains("AWS_KEY"), "{violations:?}");
        assert!(
            violations[0].message.contains("allowlist"),
            "{violations:?}"
        );
    }

    #[test]
    fn an_allowlisted_name_is_still_a_violation_outside_the_deploy_workflow() {
        let text = format!("{CLEAN}      - run: echo \"${{{{ secrets.DEPLOY_SSH_KEY }}}}\"\n");
        let violations = lint_workflow(".github/workflows/site.yml", &text);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].message.contains("only the deploy workflow"),
            "{violations:?}"
        );
    }

    #[test]
    fn every_secret_on_a_line_is_checked() {
        let text = format!(
            "{CLEAN}      - run: echo \"${{{{ secrets.DEPLOY_SSH_KEY }}}} ${{{{ secrets.DEPLOY_KNOWN_HOSTS }}}} ${{{{ secrets.OTHER }}}}\"\n"
        );
        let violations = lint_workflow(DEPLOY_WORKFLOW, &text);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].message.contains("OTHER"), "{violations:?}");
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

    /// The local CI replica must read the workflow, not a copy of it: a step
    /// added to `ci.yml` has to appear in `scripts/ci-local.sh --list`.
    #[test]
    fn ci_local_lists_the_steps_parsed_out_of_the_workflow() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let output = std::process::Command::new("bash")
            .arg(root.join("scripts/ci-local.sh"))
            .arg("--list")
            .current_dir(root)
            .output()
            .expect("run scripts/ci-local.sh --list");
        assert!(
            output.status.success(),
            "ci-local --list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        for step in [
            "Check committed sample pair",
            "Test viewer-scope crates",
            "Lint workflow files",
        ] {
            assert!(
                stdout.contains(step),
                "ci-local --list misses `{step}`:\n{stdout}"
            );
        }
        assert!(
            !stdout.contains("Detect docs-only changes"),
            "the viewer job no longer skips the native job, so the docs-only step must be gone:\n{stdout}"
        );
    }

    #[test]
    fn the_committed_workflow_set_matches_the_repository() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let (files, _) = check_workflows(root).expect("the workflow directory exists");
        for name in ["ci.yml", "security.yml", "deploy.yml"] {
            assert!(
                root.join(".github/workflows").join(name).is_file(),
                "{name} is missing from the workflow directory ({files} files)"
            );
        }
        // The Dependabot configuration is development-only: the publishing
        // rewrite withholds it from the published mirror, so a checkout without
        // it is the published one and has nothing to verify. When it is there,
        // the freshness policy is checked, so a broken edit still fails.
        match dependabot_config(root) {
            Some(text) => {
                assert!(
                    text.contains("version: 2"),
                    "the Dependabot configuration is not schema version 2"
                );
                for ecosystem in ["cargo", "github-actions", "npm"] {
                    assert!(
                        text.contains(&format!("package-ecosystem: \"{ecosystem}\"")),
                        "the Dependabot configuration does not update {ecosystem}"
                    );
                }
                assert_eq!(
                    text.matches("interval: \"weekly\"").count(),
                    3,
                    "every Dependabot update must run weekly"
                );
                assert_eq!(
                    text.matches("default-days: 14").count(),
                    3,
                    "every Dependabot update must wait out the 14-day cooldown"
                );
            }
            None => eprintln!(
                "workflows: .github/dependabot.yml is not part of this checkout, so there is \
                 nothing to verify"
            ),
        }
    }

    /// The local replica overlays the working tree onto the public export, so
    /// that a dirty tree is tested rather than only HEAD. The overlay has to
    /// carry the publishing rewrite too, or a dirty tree is tested with text
    /// the published mirror never carries.
    #[test]
    fn the_rehearsal_rewrites_the_files_it_overlays() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let dir = tempfile::tempdir().expect("create a temporary directory");
        let tree = dir.path().join("tree");
        std::fs::create_dir_all(&tree).expect("create the tree");
        let name = format!("AGENTS{}", ".md");
        std::fs::write(tree.join("notes.md"), format!("see {name} for the rules\n"))
            .expect("write the overlaid file");
        let rules = dir.path().join("redactions.txt");
        std::fs::write(
            &rules,
            format!("# comment\n{name}==>the project conventions\n"),
        )
        .expect("write the rules");
        let output = std::process::Command::new("bash")
            .arg(root.join("scripts/ci-local.sh"))
            .env("CLIMB_REDACT_TREE", &tree)
            .env("CLIMB_REDACTION_FILE", &rules)
            .current_dir(root)
            .output()
            .expect("run the replica's rewrite pass");
        assert!(
            output.status.success(),
            "the rewrite pass failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            std::fs::read_to_string(tree.join("notes.md")).expect("read the rewritten file"),
            "see the project conventions for the rules\n"
        );
    }

    /// A rule form the replica cannot apply is a refusal, never a silent skip:
    /// otherwise the rehearsal would test unrewritten files and pass where the
    /// mirror fails.
    #[test]
    fn the_rehearsal_refuses_a_rule_form_it_cannot_apply() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let dir = tempfile::tempdir().expect("create a temporary directory");
        let tree = dir.path().join("tree");
        std::fs::create_dir_all(&tree).expect("create the tree");
        let rules = dir.path().join("redactions.txt");
        std::fs::write(&rules, "regex:^foo==>bar\n").expect("write the rules");
        let output = std::process::Command::new("bash")
            .arg(root.join("scripts/ci-local.sh"))
            .env("CLIMB_REDACT_TREE", &tree)
            .env("CLIMB_REDACTION_FILE", &rules)
            .current_dir(root)
            .output()
            .expect("run the replica's rewrite pass");
        assert!(!output.status.success(), "an unsupported rule must fail");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("literal rules only"),
            "the refusal must name the limitation: {stderr}"
        );
    }

    /// The Dependabot configuration, or `None` when this checkout does not
    /// carry it (the published mirror does not).
    fn dependabot_config(root: &Path) -> Option<String> {
        std::fs::read_to_string(root.join(".github/dependabot.yml")).ok()
    }

    /// An absent Dependabot configuration must be an absence, not a failure:
    /// the presence check above runs in both checkouts.
    #[test]
    fn a_withheld_dependabot_config_is_an_absence_not_a_failure() {
        let dir = tempfile::tempdir().expect("create a temporary directory");
        assert!(dependabot_config(dir.path()).is_none());
        std::fs::create_dir_all(dir.path().join(".github")).expect("create .github");
        std::fs::write(dir.path().join(".github/dependabot.yml"), "version: 2\n")
            .expect("write the configuration");
        assert_eq!(
            dependabot_config(dir.path()).as_deref(),
            Some("version: 2\n")
        );
    }

    /// One job's lines of `ci.yml`, from its `  name:` header to the next job.
    fn job_body(text: &str, job: &str) -> String {
        let mut body = Vec::new();
        let mut inside = false;
        for line in text.lines() {
            if is_job_header(line) {
                if inside {
                    break;
                }
                inside = line.trim() == format!("{job}:");
                continue;
            }
            if inside {
                body.push(line);
            }
        }
        assert!(inside || !body.is_empty(), "job `{job}` not found");
        body.join("\n")
    }

    /// True for a `  name:` line at the job level of the workflow document.
    fn is_job_header(line: &str) -> bool {
        let Some(rest) = line.strip_prefix("  ") else {
            return false;
        };
        rest.strip_suffix(':').is_some_and(|name| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
        })
    }

    /// Each job installs the toolchain the same way, because the local replica
    /// parses one job at a time: a job that omitted an input would run locally
    /// with a different environment than the runner gives it.
    #[test]
    fn every_ci_job_pins_the_toolchain_environment() {
        let text = committed_workflow("ci.yml");
        for job in ["viewer", "native"] {
            let body = job_body(&text, job);
            for (needle, what) in [
                (
                    "actions-rust-lang/setup-rust-toolchain@",
                    "the pinned toolchain action",
                ),
                ("rustflags: \"-D warnings\"", "the explicit lint flags"),
                ("cache: false", "the explicit cache input"),
                (
                    "./scripts/ci/env.sh --github-env",
                    "the shared environment export",
                ),
            ] {
                assert!(
                    body.contains(needle),
                    "job `{job}` does not carry {what} (`{needle}`)"
                );
            }
        }
    }

    /// A stuck step must not hold a runner for the default six hours, and a
    /// failing step must still save the cache it paid for.
    #[test]
    fn every_ci_run_step_is_bounded_and_caches_survive_failure() {
        let text = committed_workflow("ci.yml");
        for job in ["viewer", "native"] {
            let body = job_body(&text, job);
            let run_steps = body.matches("run: ./scripts/ci/").count();
            let timeouts = body.matches("timeout-minutes:").count();
            assert!(
                run_steps > 0,
                "job `{job}` has no run steps, so this test would be vacuous"
            );
            assert_eq!(
                timeouts,
                run_steps + 1,
                "job `{job}` must give the job and each of its {run_steps} run steps a timeout"
            );
            assert!(
                body.contains("cache-on-failure: true"),
                "job `{job}` discards its Cargo caches when a later step fails"
            );
        }
    }

    /// The deploy workflow is the only door into the host, so the properties
    /// that keep it narrow are asserted here rather than trusted to review:
    /// it cannot run from a fork, cannot be triggered by a pull request, and
    /// declares the one permission it needs.
    #[test]
    fn the_deploy_workflow_is_scoped_to_the_public_repository() {
        let text = committed_workflow("deploy.yml");
        assert!(
            text.contains("if: github.repository == 'aiori-hackathon/A3-PS036-TC529'"),
            "the deploy job does not refuse to run outside the public repository"
        );
        assert!(
            text.contains("permissions:\n  contents: read\n"),
            "the workflow does not declare contents: read at the top level"
        );
        assert!(
            !text.contains("pull_request"),
            "a pull request must never reach the deploy workflow"
        );
        assert!(
            text.contains("concurrency:"),
            "a deploy has no concurrency group"
        );
    }

    #[test]
    fn the_deploy_workflow_runs_on_the_builds_that_change_the_site() {
        let text = committed_workflow("deploy.yml");
        for trigger in [
            "\"site/**\"",
            "\"crates/climb-web/**\"",
            "\"docs/sample-output/**\"",
            "workflow_dispatch:",
        ] {
            assert!(
                text.contains(trigger),
                "the deploy workflow does not trigger on {trigger}"
            );
        }
    }

    /// Order is the point: the gates and the smoke test run around the publish
    /// step, not after it.
    #[test]
    fn the_deploy_workflow_gates_before_it_publishes() {
        let text = committed_workflow("deploy.yml");
        let position = |needle: &str| {
            text.find(needle)
                .unwrap_or_else(|| panic!("the deploy workflow never mentions `{needle}`"))
        };
        let check = position("npm run check");
        let lighthouse = position("npm run lighthouse");
        let scripts = position("deploy/tests/release.sh");
        let publish = position("scripts/deploy-local.sh --no-build");
        let smoke = position("scripts/smoke-live.sh");
        assert!(check < publish, "the site checks run after the publish");
        assert!(
            lighthouse < publish,
            "the Lighthouse gate runs after the publish"
        );
        assert!(
            scripts < publish,
            "the release-script tests run after the publish"
        );
        assert!(
            publish < smoke,
            "the live smoke test runs before the publish"
        );
        assert!(
            text.contains("CLIMB_DEPLOY_KNOWN_HOSTS"),
            "the host key is not pinned from a secret"
        );
        assert!(
            text.contains("vars.CLOUDFLARE_PAGES_PROJECT != ''"),
            "the second target is not behind a repository variable"
        );
    }

    /// The allowlist is the contract: a workflow that reads a secret the list
    /// does not name fails the lint, so this test keeps the list honest.
    #[test]
    fn the_deploy_workflow_reads_only_allowlisted_secrets() {
        let text = committed_workflow("deploy.yml");
        let mut used: Vec<String> = Vec::new();
        for line in text.lines() {
            for name in secret_references(line) {
                if !used.contains(&name) {
                    used.push(name);
                }
            }
        }
        used.sort();
        let mut allowed: Vec<String> = DEPLOY_SECRETS.iter().map(|name| name.to_string()).collect();
        allowed.sort();
        assert_eq!(
            used, allowed,
            "the deploy workflow and the secret allowlist disagree"
        );
    }

    fn committed_workflow(name: &str) -> String {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        std::fs::read_to_string(root.join(".github/workflows").join(name))
            .unwrap_or_else(|error| panic!("failed to read {name}: {error}"))
    }

    fn committed_file(relative: &str) -> String {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        std::fs::read_to_string(root.join(relative))
            .unwrap_or_else(|error| panic!("failed to read {relative}: {error}"))
    }

    /// The environment variables the pinned setup action writes into
    /// `GITHUB_ENV`, from its `action.yml` at the SHA named in `ci.yml`.
    const SETUP_ACTION_ENV: &[&str] = &[
        "CARGO_INCREMENTAL",
        "CARGO_PROFILE_DEV_DEBUG",
        "CARGO_TERM_COLOR",
        "RUST_BACKTRACE",
        "RUSTFLAGS",
        "CARGO_UNSTABLE_SPARSE_REGISTRY",
        "CARGO_REGISTRIES_CRATES_IO_PROTOCOL",
    ];

    #[test]
    fn the_ci_workflow_writes_out_the_toolchain_rustflags() {
        let text = committed_workflow("ci.yml");
        let count = text
            .lines()
            .filter(|line| line.trim() == "rustflags: \"-D warnings\"")
            .count();
        assert_eq!(
            count, 2,
            "both setup-rust-toolchain steps must name their rustflags explicitly"
        );
    }

    #[test]
    fn ci_scripts_export_every_variable_the_setup_action_sets() {
        let script = committed_file("scripts/ci/env.sh");
        let marker = script
            .lines()
            .find(|line| line.contains("ci-setup-rust-toolchain-env:"))
            .expect("scripts/ci/env.sh must carry the setup-action environment contract marker");
        let names: Vec<String> = marker
            .split_once("ci-setup-rust-toolchain-env:")
            .expect("marker prefix")
            .1
            .split_whitespace()
            .map(|name| name.to_string())
            .collect();
        let mut expected: Vec<String> = SETUP_ACTION_ENV
            .iter()
            .map(|name| name.to_string())
            .collect();
        let mut actual = names.clone();
        expected.sort();
        actual.sort();
        assert_eq!(
            actual, expected,
            "scripts/ci/env.sh does not export the same variables as the setup action; \
             update the marker and the printf block together"
        );
        for name in SETUP_ACTION_ENV {
            let assignment = format!("\"{name}=${{{name}:-");
            assert!(
                script.contains(&assignment),
                "scripts/ci/env.sh names {name} but never sets it (`{assignment}`)"
            );
        }
    }

    /// The environment must live in one file that both the workflow and the
    /// step scripts read, so a local run cannot differ from the runner.
    #[test]
    fn the_ci_environment_comes_from_one_file() {
        let workflow = committed_workflow("ci.yml");
        assert!(
            workflow.contains("./scripts/ci/env.sh --github-env"),
            "the workflow must load the shared environment file"
        );
        for script in [
            "format",
            "clippy-viewer",
            "test-workspace",
            "docs-check",
            "native-toolchain",
            "system-deps",
        ] {
            let text = committed_file(&format!("scripts/ci/{script}.sh"));
            assert!(
                text.contains("scripts/ci/env.sh"),
                "scripts/ci/{script}.sh must source the shared environment file"
            );
        }
        let local = committed_file("scripts/ci-local.sh");
        assert!(
            local.contains("scripts/ci/env.sh"),
            "the local replica must source the same environment file"
        );
    }

    /// Every `run:` step of both CI jobs must be a call into `scripts/ci/`,
    /// so the workflow holds no logic of its own that a local run cannot
    /// reproduce.
    #[test]
    fn the_ci_jobs_call_scripts_instead_of_running_commands_inline() {
        let text = committed_workflow("ci.yml");
        for line in text.lines() {
            let trimmed = line.trim();
            let Some(command) = trimmed.strip_prefix("run: ") else {
                continue;
            };
            assert!(
                command.starts_with("./scripts/ci/"),
                "the workflow runs a command inline instead of calling scripts/ci/: {command}"
            );
        }
    }

    /// The commit-graph history checks need the full public history. A shallow
    /// checkout silently skips them, so both jobs must ask for `fetch-depth: 0`.
    #[test]
    fn both_ci_jobs_check_out_full_history() {
        let text = committed_workflow("ci.yml");
        let steps: Vec<&str> = text.split("      - name:").collect();
        let checkouts: Vec<&&str> = steps
            .iter()
            .filter(|step| step.contains("actions/checkout@"))
            .collect();
        assert_eq!(
            checkouts.len(),
            2,
            "expected one checkout step per CI job, found {}",
            checkouts.len()
        );
        for step in checkouts {
            assert!(
                step.contains("fetch-depth: 0"),
                "a CI checkout does not ask for the full history: {step}"
            );
        }
    }

    #[test]
    fn ci_local_reads_the_rustflags_input_from_the_workflow() {
        let script = committed_file("scripts/ci-local.sh");
        assert!(
            script.contains("key == \"rustflags\""),
            "ci-local must parse the workflow's rustflags input"
        );
        assert!(
            script.contains("SETUP_RUSTFLAGS"),
            "ci-local must carry the parsed rustflags into the step environment"
        );
        assert!(
            script.contains("[[ ! -v RUSTFLAGS"),
            "the replica must keep an already-set RUSTFLAGS, as the action does"
        );
    }

    #[test]
    fn the_viewer_job_lints_every_target_it_tests() {
        let text = committed_workflow("ci.yml");
        assert!(
            text.contains("./scripts/ci/clippy-viewer.sh")
                && text.contains("./scripts/ci/test-viewer.sh"),
            "the viewer job must call the clippy and test scripts"
        );
        let clippy = committed_file("scripts/ci/clippy-viewer.sh");
        let report_clippy = clippy
            .lines()
            .find(|line| line.contains("cargo clippy -p climb-report"))
            .expect("the report lint line");
        assert!(
            report_clippy.contains("--all-targets"),
            "clippy for climb-report must lint test targets: {report_clippy}"
        );
        assert!(
            report_clippy.contains("--no-default-features"),
            "the report lint and test feature sets must agree: {report_clippy}"
        );
        let tests = committed_file("scripts/ci/test-viewer.sh");
        let report_test = tests
            .lines()
            .find(|line| line.contains("cargo test -p climb-report"))
            .expect("the report test line");
        assert!(
            report_test.contains("--no-default-features"),
            "the report test feature set must agree with clippy: {report_test}"
        );
        assert!(
            !report_test.contains("--lib"),
            "the report test must compile every target clippy lints: {report_test}"
        );
    }

    #[test]
    fn the_workflows_pin_a_supported_runner_image() {
        for name in ["ci.yml", "site.yml"] {
            let text = committed_workflow(name);
            assert!(
                text.contains("runs-on: ubuntu-24.04"),
                "{name} must pin a concrete runner image"
            );
            assert!(
                !text.contains("runs-on: ubuntu-latest"),
                "{name} still uses the moving `ubuntu-latest` label"
            );
        }
    }

    #[test]
    fn the_deploy_job_is_skipped_until_a_host_is_configured() {
        let text = committed_workflow("deploy.yml");
        let condition = text
            .lines()
            .find(|line| line.trim_start().starts_with("if:"))
            .expect("the deploy job condition");
        assert!(
            condition.contains("github.repository =="),
            "the deploy job must stay scoped to the public repository: {condition}"
        );
        assert!(
            condition.contains("vars.DEPLOY_HOST != ''"),
            "the deploy job must be skipped, not failed, without DEPLOY_HOST: {condition}"
        );
    }
}
