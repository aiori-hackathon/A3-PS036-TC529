//! `docs-check` integration for the committed commit graph.
//!
//! The document must describe the history its own `as_of_sha` names, so it is
//! regenerated in memory from that commit and compared byte for byte. History
//! is read from this checkout when it contains the commit (a public clone),
//! otherwise from the public bare cache, and skipped with a printed reason when
//! neither has it: an offline checkout must always pass. The check never
//! fetches, so it cannot fail on a missing network. The card must carry the
//! same headline numbers in its `<desc>`, stay inside the size budget, and
//! request nothing from the network. Staleness relative to `HEAD` is a warning,
//! never a failure.

use anyhow::{Context, Result};
use std::path::Path;

use super::{
    cache_path, commit_exists, config_hash, generate, is_ancestor, is_shallow, load_config,
    run_git, tz_offset_minutes, CommitGraph, CommitGraphConfig, CONFIG_PATH, JSON_PATH,
    NARROW_PATH, SCHEMA_VERSION, SVG_PATH,
};

/// The outcome of the commit-graph part of the documentation check.
#[derive(Debug)]
pub(crate) struct CheckOutcome {
    pub(crate) violations: Vec<String>,
    pub(crate) warnings: Vec<String>,
    pub(crate) note: Option<String>,
}

pub(crate) fn check_docs(root: &Path) -> Result<CheckOutcome> {
    check_with_paths(
        root,
        &root.join(JSON_PATH),
        &root.join(SVG_PATH),
        &root.join(NARROW_PATH),
    )
}

pub(crate) fn check_with_paths(
    root: &Path,
    json_path: &Path,
    full_path: &Path,
    narrow_path: &Path,
) -> Result<CheckOutcome> {
    let config = load_config(&root.join(CONFIG_PATH))?;
    let mut violations = Vec::new();
    let mut warnings = Vec::new();

    let json_text = std::fs::read_to_string(json_path)
        .with_context(|| format!("failed to read {}", json_path.display()))?;
    let graph: CommitGraph = serde_json::from_str(&json_text)
        .with_context(|| format!("{} is not a valid commit graph", json_path.display()))?;
    let label = json_path
        .strip_prefix(root)
        .unwrap_or(json_path)
        .display()
        .to_string();

    if graph.schema_version != SCHEMA_VERSION {
        violations.push(format!(
            "{label}: schema_version {} is not {SCHEMA_VERSION}",
            graph.schema_version
        ));
    }
    if graph.config_hash != config_hash(&config)? {
        violations.push(format!(
            "{label}: config_hash does not match the current [commit_graph] configuration"
        ));
    }
    if graph.per_day.is_empty() {
        violations.push(format!("{label}: per_day is empty"));
    }
    let per_day_commits: u64 = graph.per_day.iter().map(|day| day.commits).sum();
    if per_day_commits != graph.totals.commits {
        violations.push(format!(
            "{label}: per_day sums to {per_day_commits} commits but totals say {}",
            graph.totals.commits
        ));
    }
    if graph.as_of_sha.len() != 40 {
        violations.push(format!("{label}: as_of_sha is not a full commit id"));
    }

    let note = if graph.as_of_sha.len() != 40 {
        Some(format!(
            "commit graph: history checks skipped because as_of_sha `{}` is not a full commit id",
            graph.as_of_sha
        ))
    } else if is_shallow(root)? {
        Some(
            "commit graph: history-dependent checks skipped because this is a shallow clone; \
             run `git fetch --unshallow` to check reproduction, ancestry and staleness"
                .to_string(),
        )
    } else {
        check_history(
            root,
            &config,
            &graph,
            &json_text,
            &label,
            &mut violations,
            &mut warnings,
        )?
    };

    check_card(
        root,
        full_path,
        "commit-graph.svg",
        &graph,
        config.budget_kb,
        &mut violations,
    )?;
    check_card(
        root,
        narrow_path,
        "commit-graph-narrow.svg",
        &graph,
        config.budget_kb,
        &mut violations,
    )?;

    Ok(CheckOutcome {
        violations,
        warnings,
        note,
    })
}

/// Reproduce the document from the history that contains its as-of commit.
/// Returns a note when the history checks had to be skipped or read from the
/// cache, and `None` when they ran against this checkout.
fn check_history(
    root: &Path,
    config: &CommitGraphConfig,
    graph: &CommitGraph,
    json_text: &str,
    label: &str,
    violations: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<Option<String>> {
    let sha = &graph.as_of_sha;
    if is_ancestor(root, sha)? {
        let (_, regenerated) = generate(root, config, Some(sha))
            .with_context(|| format!("{label}: failed to regenerate the graph from as_of_sha"))?;
        if regenerated != json_text {
            violations.push(format!(
                "{label}: the document does not reproduce byte for byte from its as_of_sha; \
                 regenerate it with `cargo xtask commit-graph`"
            ));
        }
        let count = run_git(root, &["rev-list", "--count", &format!("{}..HEAD", sha)])?;
        let ahead: u64 = String::from_utf8_lossy(&count).trim().parse().unwrap_or(0);
        let head_stamp: i64 =
            String::from_utf8_lossy(&run_git(root, &["show", "-s", "--format=%at", "HEAD"])?)
                .trim()
                .parse()
                .unwrap_or(0);
        let offset = tz_offset_minutes(&config.timezone)?;
        let head_day = super::local_day(head_stamp, offset);
        let as_of_day = super::parse_iso_date(&graph.as_of_date)?;
        let days_behind = (head_day - as_of_day).max(0);
        warnings.extend(staleness_warnings(ahead, days_behind, config));
        return Ok(None);
    }

    let cache = cache_path(root, config);
    if cache.exists() && commit_exists(&cache, sha)? {
        let (_, regenerated) = generate(&cache, config, Some(sha)).with_context(|| {
            format!("{label}: failed to regenerate the graph from the public-history cache")
        })?;
        if regenerated != json_text {
            violations.push(format!(
                "{label}: the document does not reproduce byte for byte from the public-history \
                 cache at {}; regenerate it with `cargo xtask commit-graph`",
                cache.display()
            ));
        }
        return Ok(Some(format!(
            "commit graph: reproduction checked against the public-history cache at {}; \
             staleness is not checked because this checkout does not contain the public as-of \
             commit",
            cache.display()
        )));
    }

    Ok(Some(format!(
        "commit graph: history checks skipped because as_of_sha {} is not in this checkout and \
         the cache {} does not contain it; run `cargo xtask commit-graph --source public` to \
         populate the cache. An offline checkout must still pass, so this is not a failure.",
        &sha[..7.min(sha.len())],
        cache.display()
    )))
}

fn staleness_warnings(
    commits_behind: u64,
    days_behind: i64,
    config: &CommitGraphConfig,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if commits_behind > config.staleness_commits {
        warnings.push(format!(
            "commit graph is {commits_behind} commits behind HEAD (threshold {}); \
             run `cargo xtask commit-graph`",
            config.staleness_commits
        ));
    }
    if days_behind > config.staleness_days {
        warnings.push(format!(
            "commit graph is {days_behind} days behind HEAD (threshold {}); \
             run `cargo xtask commit-graph`",
            config.staleness_days
        ));
    }
    warnings
}

fn check_card(
    root: &Path,
    path: &Path,
    expected_name: &str,
    graph: &CommitGraph,
    budget_kb: u32,
    violations: &mut Vec<String>,
) -> Result<()> {
    let label = path
        .strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string();
    if path.file_name().and_then(|name| name.to_str()) != Some(expected_name) {
        violations.push(format!("{label}: unexpected card filename"));
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let budget = budget_kb as usize * 1024;
    if text.len() > budget {
        violations.push(format!(
            "{label}: {} bytes exceed the {budget_kb} kB budget",
            text.len()
        ));
    }
    let document = match roxmltree::Document::parse(&text) {
        Ok(document) => document,
        Err(error) => {
            violations.push(format!("{label}: not well-formed XML ({error})"));
            return Ok(());
        }
    };
    let root_element = document.root_element();
    if root_element.tag_name().name() != "svg" {
        violations.push(format!("{label}: root element is not <svg>"));
    }
    if root_element.attribute("role") != Some("img") {
        violations.push(format!("{label}: <svg> must carry role=\"img\""));
    }
    let title = root_element
        .children()
        .find(|node| node.has_tag_name("title"))
        .and_then(|node| node.text())
        .unwrap_or_default();
    if title.trim().is_empty() {
        violations.push(format!("{label}: <title> is missing or empty"));
    }
    let desc = root_element
        .children()
        .find(|node| node.has_tag_name("desc"))
        .and_then(|node| node.text())
        .unwrap_or_default();
    if desc.trim().is_empty() {
        violations.push(format!("{label}: <desc> is missing or empty"));
    } else {
        check_desc(&label, desc, graph, violations);
    }

    // Text must be outlines, never live text: a font substitution would change
    // the picture, and a <text> element is the one way to depend on a font.
    if document.descendants().any(|node| node.has_tag_name("text")) {
        violations.push(format!(
            "{label}: contains a <text> element; text must be paths"
        ));
    }
    for tag in ["script", "image", "foreignObject"] {
        if document.descendants().any(|node| node.has_tag_name(tag)) {
            violations.push(format!("{label}: contains a <{tag}> element"));
        }
    }
    if text.contains("xlink:href") || text.contains("@import") {
        violations.push(format!(
            "{label}: contains an external reference or stylesheet import"
        ));
    }
    let remaining = text.replace("http://www.w3.org/2000/svg", "");
    if remaining.contains("http") {
        violations.push(format!(
            "{label}: contains an http reference other than the SVG namespace"
        ));
    }
    Ok(())
}

fn check_desc(label: &str, desc: &str, graph: &CommitGraph, violations: &mut Vec<String>) {
    let tokens: std::collections::BTreeMap<&str, &str> = desc
        .split_whitespace()
        .filter_map(|token| token.split_once('='))
        .collect();
    let expected = [
        ("commits", graph.totals.commits.to_string()),
        ("refresh_commits", graph.totals.refresh_commits.to_string()),
        ("active_days", graph.totals.active_days.to_string()),
        ("longest_streak", graph.totals.longest_streak.to_string()),
        ("current_streak", graph.totals.current_streak.to_string()),
        ("lines_added", graph.totals.lines_added.to_string()),
        ("lines_removed", graph.totals.lines_removed.to_string()),
        ("files_touched", graph.totals.files_touched.to_string()),
        (
            "as_of",
            graph.as_of_sha[..7.min(graph.as_of_sha.len())].to_string(),
        ),
        ("date", graph.as_of_date.clone()),
    ];
    for (key, value) in expected {
        match tokens.get(key) {
            Some(actual) if *actual == value => {}
            Some(actual) => violations.push(format!(
                "{label}: <desc> says {key}={actual} but the JSON says {value}"
            )),
            None => violations.push(format!("{label}: <desc> does not carry {key}=<value>")),
        }
    }
}

/// Paths used by the tests below when they need the real workspace.
#[cfg(test)]
fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives inside the workspace")
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staleness_is_a_warning_and_respects_both_thresholds() {
        let config = CommitGraphConfig {
            timezone: "Asia/Kolkata".to_string(),
            source_url: "https://example.invalid/public.git".to_string(),
            source_ref: "main".to_string(),
            source_cache: "target/public.git".to_string(),
            repo_web_url: "https://example.invalid/public".to_string(),
            exclude_authors: Vec::new(),
            display_names: Default::default(),
            refresh_commits_whose_files_all_match: Vec::new(),
            milestones: Vec::new(),
            budget_kb: 50,
            staleness_commits: 3,
            staleness_days: 2,
        };
        assert!(staleness_warnings(3, 2, &config).is_empty());
        let warnings = staleness_warnings(4, 9, &config);
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("4 commits behind"));
        assert!(warnings[1].contains("9 days behind"));
    }

    #[test]
    fn a_card_whose_desc_disagrees_with_the_json_is_a_violation() {
        let root = workspace_root();
        let committed =
            std::fs::read_to_string(root.join(JSON_PATH)).expect("committed commit graph");
        let graph: CommitGraph = serde_json::from_str(&committed).expect("valid graph");
        let card = std::fs::read_to_string(root.join(SVG_PATH)).expect("committed card");
        let needle = format!("commits={}", graph.totals.commits);
        assert!(card.contains(&needle), "the card carries commits=");
        let edited = card.replacen(&needle, &format!("commits={}", graph.totals.commits + 1), 1);
        let path = root.join("target/commit-graph-edited-card.svg");
        std::fs::write(&path, edited).expect("write fixture");

        let outcome =
            check_with_paths(&root, &root.join(JSON_PATH), &path, &root.join(NARROW_PATH))
                .expect("check runs");
        assert!(
            outcome
                .violations
                .iter()
                .any(|violation| violation.contains("<desc> says commits")),
            "{:?}",
            outcome.violations
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_missing_json_is_an_error_that_names_the_path() {
        let root = workspace_root();
        let missing = root.join("target/commit-graph-absent.json");
        let _ = std::fs::remove_file(&missing);
        let error = check_with_paths(
            &root,
            &missing,
            &root.join(SVG_PATH),
            &root.join(NARROW_PATH),
        )
        .expect_err("a missing document cannot be checked");
        assert!(
            error.to_string().contains("commit-graph-absent.json"),
            "{error}"
        );
    }

    #[test]
    fn a_truncated_json_is_a_violation() {
        let root = workspace_root();
        let path = root.join("target/commit-graph-truncated.json");
        std::fs::write(&path, "{\"schema_version\": 1").expect("write fixture");
        let error = check_with_paths(&root, &path, &root.join(SVG_PATH), &root.join(NARROW_PATH))
            .expect_err("a truncated document cannot parse");
        assert!(error.to_string().contains("not a valid commit graph"));
        let _ = std::fs::remove_file(path);
    }
}
