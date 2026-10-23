//! Tests over synthetic repositories. Every fixture is created in a temporary
//! directory with pinned author and committer dates, so the expectations do not
//! depend on the host clock or on the repository this test binary runs in.

use super::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempDir {
    root: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "climb-commit-graph-{}-{serial}-{label}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create the temporary repository");
        Self { root }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct TempRepo {
    dir: TempDir,
}

impl TempRepo {
    fn new(label: &str) -> Self {
        let repo = Self {
            dir: TempDir::new(label),
        };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.name", "Test Harness"]);
        repo.git(&["config", "user.email", "harness@example.invalid"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo
    }

    fn path(&self) -> &Path {
        &self.dir.root
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.dir.root)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn commit(&self, date: &str, author: &str, files: &[(&str, &str)]) -> String {
        for (path, content) in files {
            let full = self.dir.root.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("create a fixture directory");
            }
            std::fs::write(&full, content).expect("write a fixture file");
        }
        self.git(&["add", "-A"]);
        self.commit_staged(date, author, "fixture")
    }

    /// Stage exactly `paths` and commit them with `message`.
    fn commit_paths(&self, date: &str, author: &str, message: &str, paths: &[&str]) -> String {
        let mut args = vec!["add", "--"];
        args.extend(paths);
        self.git(&args);
        self.commit_staged(date, author, message)
    }

    fn commit_staged(&self, date: &str, author: &str, message: &str) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.dir.root)
            .args(["commit", "-q", "--allow-empty", "-m", message])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .env("GIT_AUTHOR_NAME", author)
            .env("GIT_AUTHOR_EMAIL", "harness@example.invalid")
            .env("GIT_COMMITTER_NAME", author)
            .env("GIT_COMMITTER_EMAIL", "harness@example.invalid")
            .output()
            .expect("run git commit");
        assert!(
            output.status.success(),
            "git commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        self.git(&["rev-parse", "HEAD"]).trim().to_string()
    }
}

fn config() -> CommitGraphConfig {
    CommitGraphConfig {
        timezone: "Asia/Kolkata".to_string(),
        source_url: "https://example.invalid/public.git".to_string(),
        source_ref: "main".to_string(),
        source_cache: "target/public.git".to_string(),
        repo_web_url: "https://example.invalid/public".to_string(),
        exclude_authors: Vec::new(),
        display_names: BTreeMap::new(),
        refresh_commits_whose_files_all_match: Vec::new(),
        refresh_commits_allow_readme: true,
        self_count: false,
        milestones: Vec::new(),
        style: StyleConfig::default(),
        budget_kb: 50,
        strip_budget_kb: 25,
        staleness_commits: 20,
        staleness_days: 7,
    }
}

fn day<'a>(graph: &'a CommitGraph, date: &str) -> &'a DayEntry {
    graph
        .per_day
        .iter()
        .find(|entry| entry.date == date)
        .unwrap_or_else(|| panic!("{date} is not in per_day"))
}

/// Every SVG variant rendered with `style`, labelled by variant.
fn render_all(graph: &CommitGraph, style: &StyleConfig) -> Vec<(&'static str, String)> {
    let palette = svg::Palette::default();
    vec![
        (
            "full",
            svg::render_full(graph, &palette, style).expect("full card"),
        ),
        (
            "narrow",
            svg::render_narrow(graph, &palette, style).expect("narrow card"),
        ),
        (
            "compact",
            svg::render_compact(graph, &palette, style).expect("compact strip"),
        ),
        (
            "compact-narrow",
            svg::render_compact_narrow(graph, &palette, style).expect("narrow compact strip"),
        ),
    ]
}

/// Write the four SVG variants into `root/docs/media`.
fn write_cards(root: &Path, graph: &CommitGraph, style: &StyleConfig) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for (label, card) in render_all(graph, style) {
        let name = match label {
            "full" => SVG_PATH,
            "narrow" => NARROW_PATH,
            "compact" => COMPACT_PATH,
            _ => COMPACT_NARROW_PATH,
        };
        let path = root.join(name);
        std::fs::write(&path, card).expect("write card");
        paths.push(path);
    }
    paths
}

#[test]
fn per_day_counts_and_totals_reconcile() {
    let repo = TempRepo::new("per-day");
    repo.commit(
        "2026-10-10T10:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "one\ntwo\n")],
    );
    repo.commit(
        "2026-10-10T18:00:00+05:30",
        "Pratyay Mukherjee",
        &[("b.txt", "x\n")],
    );
    repo.commit(
        "2026-10-12T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "one\nthree\n")],
    );

    let (graph, _) = generate(repo.path(), &config(), Some("HEAD")).expect("generate");
    assert_eq!(graph.totals.commits, 3);
    assert_eq!(graph.totals.active_days, 2);
    assert_eq!(day(&graph, "2026-10-10").commits, 2);
    assert_eq!(day(&graph, "2026-10-11").commits, 0);
    assert_eq!(day(&graph, "2026-10-12").commits, 1);
    assert_eq!(graph.per_day.len(), 3);
    assert_eq!(graph.range.start, "2026-10-10");
    assert_eq!(graph.range.end, "2026-10-12");
}

#[test]
fn per_day_sums_match_the_totals() {
    let repo = TempRepo::new("sums");
    repo.commit(
        "2026-10-01T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("one.txt", "a\nb\nc\n")],
    );
    repo.commit(
        "2026-10-02T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("two.txt", "d\n")],
    );
    repo.commit(
        "2026-10-02T19:00:00+05:30",
        "Pratyay Mukherjee",
        &[("one.txt", "a\n")],
    );

    let (graph, _) = generate(repo.path(), &config(), Some("HEAD")).expect("generate");
    let commits: u64 = graph.per_day.iter().map(|entry| entry.commits).sum();
    let added: u64 = graph.per_day.iter().map(|entry| entry.added).sum();
    let removed: u64 = graph.per_day.iter().map(|entry| entry.removed).sum();
    assert_eq!(commits, graph.totals.commits);
    assert_eq!(added, graph.totals.lines_added);
    assert_eq!(removed, graph.totals.lines_removed);
    assert_eq!(graph.totals.lines_added, 4);
    assert_eq!(graph.totals.lines_removed, 2);
    assert_eq!(graph.totals.files_touched, 3);
}

#[test]
fn the_istanbul_day_boundary_wins_over_the_utc_date() {
    let repo = TempRepo::new("istanbul");
    // 23:30 UTC is 05:00 the next day in Asia/Kolkata.
    repo.commit(
        "2026-10-13T23:30:00Z",
        "Pratyay Mukherjee",
        &[("a.txt", "a\n")],
    );
    // 23:30 IST is still the same calendar day there, and 18:00 UTC.
    repo.commit(
        "2026-10-14T23:30:00+05:30",
        "Pratyay Mukherjee",
        &[("b.txt", "b\n")],
    );

    let (graph, _) = generate(repo.path(), &config(), Some("HEAD")).expect("generate");
    assert_eq!(graph.as_of_date, "2026-10-14");
    // Both commits fall on 14 October in Kolkata, so the first day of the
    // range is that day and no earlier, empty day is emitted.
    assert_eq!(graph.per_day.len(), 1);
    assert_eq!(day(&graph, "2026-10-14").commits, 2);
    assert_eq!(graph.range.start, "2026-10-14");
}

#[test]
fn a_utc_graph_buckets_the_same_commit_on_the_previous_day() {
    let repo = TempRepo::new("utc");
    repo.commit(
        "2026-10-13T23:30:00Z",
        "Pratyay Mukherjee",
        &[("a.txt", "a\n")],
    );
    let mut utc = config();
    utc.timezone = "UTC".to_string();
    let (graph, _) = generate(repo.path(), &utc, Some("HEAD")).expect("generate");
    assert_eq!(graph.as_of_date, "2026-10-13");
    assert_eq!(day(&graph, "2026-10-13").commits, 1);
}

#[test]
fn streak_logic_counts_consecutive_active_days() {
    let repo = TempRepo::new("streak");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "a\n")],
    );
    repo.commit(
        "2026-10-11T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("b.txt", "b\n")],
    );
    let middle = repo.commit(
        "2026-10-12T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("c.txt", "c\n")],
    );
    repo.commit(
        "2026-10-15T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("d.txt", "d\n")],
    );

    let (head, _) = generate(repo.path(), &config(), Some("HEAD")).expect("generate");
    assert_eq!(head.totals.longest_streak, 3);
    assert_eq!(head.totals.current_streak, 1);

    let (past, _) = generate(repo.path(), &config(), Some(&middle)).expect("generate");
    assert_eq!(past.totals.longest_streak, 3);
    assert_eq!(past.totals.current_streak, 3);
    assert_eq!(past.range.end, "2026-10-12");
}

#[test]
fn an_excluded_author_is_hidden_but_its_commits_are_counted() {
    let repo = TempRepo::new("bot");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "github-actions[bot]",
        &[("generated.txt", "bot\n")],
    );
    repo.commit(
        "2026-10-11T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("human.txt", "human\n")],
    );
    let mut config = config();
    config.exclude_authors = vec!["github-actions[bot]".to_string()];
    let (graph, json) = generate(repo.path(), &config, Some("HEAD")).expect("generate");
    // The public repository's commit count includes the bot commit, so the
    // totals must too; only the contributor breakdown hides the name.
    assert_eq!(graph.totals.commits, 2);
    assert_eq!(graph.per_author.len(), 1);
    assert!(!json.contains("github-actions"));
    assert!(!json.contains("bot"));
    let counted: u64 = graph.per_day.iter().map(|day| day.commits).sum();
    assert_eq!(counted, 2);
}

#[test]
fn media_only_commits_are_counted_and_reported_as_refreshes() {
    let repo = TempRepo::new("paths");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[
            ("docs/media/commit-graph.json", "{}"),
            ("docs/media/commit-graph.svg", "<svg/>"),
        ],
    );
    repo.commit(
        "2026-10-11T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[
            ("docs/media/commit-graph.json", "{\"v\":2}"),
            ("src/lib.rs", "pub fn f() {}\n"),
        ],
    );
    repo.commit(
        "2026-10-12T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("README.md", "hello\n")],
    );
    let mut config = config();
    config.refresh_commits_whose_files_all_match = vec!["docs/media/commit-graph*".to_string()];
    let (graph, _) = generate(repo.path(), &config, Some("HEAD")).expect("generate");
    // Nothing is hidden: the media-only commit is part of the total and the
    // per-day counts, and is reported separately as a graph refresh.
    assert_eq!(graph.totals.commits, 3);
    assert_eq!(graph.totals.refresh_commits, 1);
    assert_eq!(graph.range.start, "2026-10-10");
    assert_eq!(day(&graph, "2026-10-10").commits, 1);
    assert_eq!(graph.totals.files_touched, 5);
}

#[test]
fn a_commit_that_touches_a_graph_file_and_code_is_not_a_refresh() {
    let repo = TempRepo::new("mixed-paths");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[
            ("docs/media/commit-graph.json", "{}"),
            ("src/lib.rs", "pub fn f() {}\n"),
        ],
    );
    let mut config = config();
    config.refresh_commits_whose_files_all_match = vec!["docs/media/commit-graph*".to_string()];
    let (graph, _) = generate(repo.path(), &config, Some("HEAD")).expect("generate");
    assert_eq!(graph.totals.commits, 1);
    assert_eq!(graph.totals.refresh_commits, 0);
}

#[test]
fn an_empty_commit_is_not_a_graph_refresh() {
    let repo = TempRepo::new("empty");
    repo.commit("2026-10-10T09:00:00+05:30", "Pratyay Mukherjee", &[]);
    let mut config = config();
    config.refresh_commits_whose_files_all_match = vec!["docs/media/commit-graph*".to_string()];
    let (graph, _) = generate(repo.path(), &config, Some("HEAD")).expect("generate");
    assert_eq!(graph.totals.commits, 1);
    assert_eq!(graph.totals.refresh_commits, 0);
    assert_eq!(graph.totals.files_touched, 0);
}

#[test]
fn display_names_are_keyed_by_git_author_name() {
    let repo = TempRepo::new("display");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "a\n")],
    );
    let mut config = config();
    config
        .display_names
        .insert("Pratyay Mukherjee".to_string(), "Pratyay".to_string());
    let (graph, json) = generate(repo.path(), &config, Some("HEAD")).expect("generate");
    assert_eq!(graph.per_author[0].display, "Pratyay");
    assert!(json.contains("\"display\": \"Pratyay\""));
    assert!(!json.contains("Mukherjee"));
}

#[test]
fn an_unmapped_author_keeps_the_git_name_and_no_email_appears() {
    let repo = TempRepo::new("unmapped");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Someone Else",
        &[("a.txt", "a\n")],
    );
    let (graph, json) = generate(repo.path(), &config(), Some("HEAD")).expect("generate");
    assert_eq!(graph.per_author[0].display, "Someone Else");
    assert!(!json.contains('@'));
    assert!(!json.contains("harness@example.invalid"));
}

#[test]
fn a_shallow_clone_is_rejected_with_an_actionable_message() {
    let origin = TempRepo::new("shallow-origin");
    origin.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "a\n")],
    );
    origin.commit(
        "2026-10-11T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("b.txt", "b\n")],
    );
    let destination = TempDir::new("shallow-clone");
    let clone = destination.root.join("clone");
    let url = format!("file://{}", origin.path().display());
    let status = Command::new("git")
        .args([
            "clone",
            "-q",
            "--depth",
            "1",
            &url,
            clone.to_str().expect("utf-8 path"),
        ])
        .status()
        .expect("run git clone");
    assert!(status.success());
    let error = ensure_full_history(&clone).expect_err("a shallow clone has no full history");
    let message = error.to_string();
    assert!(message.contains("shallow"), "{message}");
    assert!(message.contains("unshallow"), "{message}");
    assert!(generate(&clone, &config(), Some("HEAD")).is_ok());
    assert!(is_shallow(&clone).expect("shallow probe"));
}

#[test]
fn a_shallow_clone_skips_history_checks_and_reports_why() {
    let origin = TempRepo::new("shallow-check-origin");
    origin.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "a\n")],
    );
    origin.commit(
        "2026-10-13T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("b.txt", "b\n")],
    );
    let destination = TempDir::new("shallow-check-clone");
    let clone = destination.root.join("clone");
    let url = format!("file://{}", origin.path().display());
    let status = Command::new("git")
        .args([
            "clone",
            "-q",
            "--depth",
            "1",
            &url,
            clone.to_str().expect("utf-8 path"),
        ])
        .status()
        .expect("run git clone");
    assert!(status.success());

    std::fs::create_dir_all(clone.join("docs/media")).expect("create the card directory");
    write_config(&clone, &config());
    let (graph, json) = generate(&clone, &config(), Some("HEAD")).expect("generate");
    let json_path = clone.join(JSON_PATH);
    std::fs::write(&json_path, json).expect("write the JSON");
    let cards = write_cards(&clone, &graph, &StyleConfig::default());

    let outcome = check::check_with_paths(
        &clone, &json_path, &cards[0], &cards[1], &cards[2], &cards[3],
    )
    .expect("the check runs on a shallow clone");
    let note = outcome.note.unwrap_or_default();
    assert!(note.contains("shallow"), "{note}");
    assert!(note.contains("unshallow"), "{note}");
    assert!(outcome.violations.is_empty(), "{:?}", outcome.violations);
}

#[test]
fn generation_is_byte_identical_twice() {
    let repo = TempRepo::new("determinism");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "one\n")],
    );
    repo.commit(
        "2026-10-11T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("b.txt", "two\n")],
    );
    let (first_graph, first_json) =
        generate(repo.path(), &config(), Some("HEAD")).expect("first generation");
    let (second_graph, second_json) =
        generate(repo.path(), &config(), Some("HEAD")).expect("second generation");
    assert_eq!(first_json, second_json);
    assert_eq!(first_graph, second_graph);

    let first = render_all(&first_graph, &StyleConfig::default());
    let second = render_all(&second_graph, &StyleConfig::default());
    for ((label, first), (_, second)) in first.iter().zip(second.iter()) {
        assert_eq!(first, second, "{label} is not deterministic");
    }
}

#[test]
fn the_cards_are_well_formed_self_contained_and_within_budget() {
    let repo = TempRepo::new("cards");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "one\ntwo\n")],
    );
    repo.commit(
        "2026-10-12T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("b.txt", "three\n")],
    );
    let (graph, _) = generate(repo.path(), &config(), Some("HEAD")).expect("generate");
    for (label, card) in render_all(&graph, &StyleConfig::default()) {
        let budget = if label.starts_with("compact") {
            STRIP_BUDGET_BYTES
        } else {
            CARD_BUDGET_BYTES
        };
        assert!(
            card.len() <= budget,
            "{label} is {} bytes, over {budget}",
            card.len()
        );
        let document = roxmltree::Document::parse(&card)
            .unwrap_or_else(|error| panic!("{label} is not XML: {error}"));
        assert_eq!(document.root_element().tag_name().name(), "svg");
        assert_eq!(document.root_element().attribute("role"), Some("img"));
        assert!(document
            .descendants()
            .any(|node| node.has_tag_name("title")));
        assert!(document.descendants().any(|node| node.has_tag_name("desc")));
        assert!(!document.descendants().any(|node| node.has_tag_name("text")));
        assert!(card.contains("<path id=\"m") || card.contains("<path id=\"d"));
        assert!(card.contains("<use href=\"#"));
        let remaining = card.replace("http://www.w3.org/2000/svg", "");
        assert!(!remaining.contains("http"), "{label} has an http reference");
        assert!(!card.contains("<script"));
    }
}

#[test]
fn the_desc_numbers_equal_the_json_totals() {
    let repo = TempRepo::new("desc");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "one\ntwo\n")],
    );
    let (graph, _) = generate(repo.path(), &config(), Some("HEAD")).expect("generate");
    let palette = svg::Palette::default();
    let card = svg::render_full(&graph, &palette, &StyleConfig::default()).expect("card");
    let document = roxmltree::Document::parse(&card).expect("XML");
    let desc = document
        .root_element()
        .children()
        .find(|node| node.has_tag_name("desc"))
        .and_then(|node| node.text())
        .expect("desc");
    let tokens: BTreeMap<&str, &str> = desc
        .split_whitespace()
        .filter_map(|token| token.split_once('='))
        .collect();
    assert_eq!(tokens.get("commits"), Some(&"1"));
    assert_eq!(tokens.get("refresh_commits"), Some(&"0"));
    assert_eq!(tokens.get("lines_added"), Some(&"2"));
    assert_eq!(tokens.get("date"), Some(&"2026-10-10"));
    assert_eq!(tokens.get("as_of"), Some(&&graph.as_of_sha[..7]));
    assert_eq!(tokens.get("authors"), Some(&"1"));
    assert_eq!(tokens.get("first_commit"), Some(&"2026-10-10"));
    assert_eq!(tokens.get("last_commit"), Some(&"2026-10-10"));
    assert_eq!(tokens.get("busiest_day"), Some(&"2026-10-10"));
    assert_eq!(tokens.get("busiest_commits"), Some(&"1"));
    assert_eq!(tokens.get("streak_start"), Some(&"2026-10-10"));
    assert_eq!(tokens.get("streak_end"), Some(&"2026-10-10"));
}

#[test]
fn the_committed_document_reproduces_from_the_history_that_contains_it() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives inside the workspace");
    let committed = std::fs::read_to_string(root.join(JSON_PATH)).expect("committed graph");
    let graph: CommitGraph = serde_json::from_str(&committed).expect("valid graph");
    let config = load_config(&root.join(CONFIG_PATH)).expect("config");
    let cache = cache_path(root, &config);
    let history = if is_ancestor(root, &graph.as_of_sha).expect("ancestry probe") {
        root.to_path_buf()
    } else if cache.exists() && commit_exists(&cache, &graph.as_of_sha).expect("cache probe") {
        cache
    } else {
        // A private checkout without the public cache cannot reproduce the
        // committed card; the documentation check reports that as a skip rather
        // than a failure, and there is nothing to compare here.
        eprintln!(
            "commit graph: {} is in neither this checkout nor the cache; reproduction skipped",
            &graph.as_of_sha[..7]
        );
        return;
    };
    let (_, regenerated) = generate(&history, &config, Some(&graph.as_of_sha))
        .expect("regenerate from the containing history");
    assert_eq!(regenerated, committed);
}

#[test]
fn the_committed_cards_pass_the_documentation_check() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives inside the workspace");
    let outcome = check::check_docs(root).expect("check runs");
    assert!(outcome.violations.is_empty(), "{:#?}", outcome.violations);
}

#[test]
fn config_hash_changes_when_a_rule_changes() {
    let base = config();
    let mut changed = base.clone();
    changed
        .display_names
        .insert("A".to_string(), "B".to_string());
    assert_ne!(
        config_hash(&base).expect("hash"),
        config_hash(&changed).expect("hash")
    );
    assert_eq!(
        config_hash(&base).expect("hash"),
        config_hash(&base).expect("hash")
    );
}

#[test]
fn glob_matching_handles_stars_questions_and_separators() {
    assert!(glob_match(
        "docs/media/commit-graph*",
        "docs/media/commit-graph.json"
    ));
    assert!(glob_match(
        "docs/media/commit-graph*",
        "docs/media/commit-graph.svg"
    ));
    assert!(!glob_match(
        "docs/media/commit-graph*",
        "docs/media/commit-graph/deep.json"
    ));
    assert!(glob_match("src/*.rs", "src/lib.rs"));
    assert!(!glob_match("src/*.rs", "src/nested/lib.rs"));
    assert!(glob_match("src/**/lib.rs", "src/nested/lib.rs"));
    assert!(glob_match("a?c", "abc"));
    assert!(!glob_match("a?c", "ac"));
}

#[test]
fn timezone_offsets_are_fixed_and_validated() {
    assert_eq!(tz_offset_minutes("Asia/Kolkata").expect("ist"), 330);
    assert_eq!(tz_offset_minutes("UTC").expect("utc"), 0);
    assert_eq!(tz_offset_minutes("UTC+02:00").expect("plus"), 120);
    assert_eq!(tz_offset_minutes("UTC-03:30").expect("minus"), -210);
    assert!(tz_offset_minutes("Europe/London").is_err());
    assert!(tz_offset_minutes("UTC+25:00").is_err());
}

#[test]
fn iso_dates_round_trip_across_leap_years_and_epochs() {
    for date in [
        "1970-01-01",
        "2000-02-29",
        "2024-02-29",
        "2026-10-22",
        "1969-12-31",
    ] {
        let days = parse_iso_date(date).expect("valid date");
        assert_eq!(format_date(days), date);
    }
    assert!(parse_iso_date("2026-02-30").is_err());
    assert!(parse_iso_date("2026-2-3").is_err());
}

#[test]
fn totals_commits_equals_git_rev_list_count() {
    let repo = TempRepo::new("rev-list");
    for day in 10..16 {
        repo.commit(
            &format!("2026-10-{day}T09:00:00+05:30"),
            "Pratyay Mukherjee",
            &[(format!("f{day}.txt").as_str(), "a\n")],
        );
    }
    let expected: u64 = repo
        .git(&["rev-list", "--count", "HEAD"])
        .trim()
        .parse()
        .expect("a count");
    let (graph, _) = generate(repo.path(), &config(), Some("HEAD")).expect("generate");
    assert_eq!(graph.totals.commits, expected);
    assert_eq!(
        graph.per_day.iter().map(|entry| entry.commits).sum::<u64>(),
        expected
    );
    assert_eq!(graph.totals.refresh_commits, 0);
}

/// Write a configuration file that `load_config` parses into `config`.
fn write_config(root: &Path, config: &CommitGraphConfig) {
    let patterns = config
        .refresh_commits_whose_files_all_match
        .iter()
        .map(|pattern| format!("\"{pattern}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let text = format!(
        "[commit_graph]\n\
         timezone = \"{}\"\n\
         source_url = \"{}\"\n\
         source_ref = \"{}\"\n\
         source_cache = \"{}\"\n\
         repo_web_url = \"{}\"\n\
         refresh_commits_whose_files_all_match = [{patterns}]\n\
         refresh_commits_allow_readme = {}\n\
         self_count = {}\n\
         budget_kb = {}\n\
         staleness_commits = {}\n\
         staleness_days = {}\n",
        config.timezone,
        config.source_url,
        config.source_ref,
        config.source_cache,
        config.repo_web_url,
        config.refresh_commits_allow_readme,
        config.self_count,
        config.budget_kb,
        config.staleness_commits,
        config.staleness_days,
    );
    std::fs::write(root.join(CONFIG_PATH), text).expect("write the configuration");
}

/// The README blocks with a permalink, a compact strip and the provenance
/// caption, matching the shape the documentation check enforces.
fn readme_text(permalink: &str) -> String {
    format!(
        "# CLIMB\n\n\
         <!-- badges:begin -->\n\
         <!-- badges:end -->\n\n\
         <!-- commit-graph-strip:begin -->\n\
         <a href=\"{permalink}\" target=\"_blank\" rel=\"noopener\">\n\
         <picture>\n\
         <source media=\"(max-width: 600px)\" srcset=\"docs/media/commit-graph-compact-narrow.svg\" width=\"390\">\n\
         <img src=\"docs/media/commit-graph-compact.svg\" alt=\"the climb so far\" width=\"840\">\n\
         </picture>\n\
         </a>\n\
         <!-- commit-graph-strip:end -->\n\n\
         <!-- commit-graph:begin -->\n\
         <a href=\"{permalink}\" target=\"_blank\" rel=\"noopener\">\n\
         <picture>\n\
         <source media=\"(max-width: 600px)\" srcset=\"docs/media/commit-graph-narrow.svg\" width=\"390\">\n\
         <img src=\"docs/media/commit-graph.svg\" alt=\"a graph\" width=\"840\">\n\
         </picture>\n\
         </a>\n\n\
         Read from the public commit history of this repository.\n\n\
         Generated by `cargo xtask commit-graph` from the public history of\n\
         aiori-hackathon/A3-PS036-TC529, as of the commit shown in the image; later\n\
         commits appear after the next refresh.\n\
         <!-- commit-graph:end -->\n"
    )
}

/// Write a complete, consistent set of artifacts for `repo` at `as_of`, read
/// from `history`, so the `--check` command has something to read.
fn write_artifacts_from(
    history: &Path,
    repo: &TempRepo,
    config: &CommitGraphConfig,
    as_of: &str,
) -> CommitGraph {
    let pending = if config.self_count {
        Pending::Today
    } else {
        Pending::Disabled
    };
    write_artifacts_at(history, repo, config, as_of, pending)
}

/// [`write_artifacts_from`] with an explicit pending-refresh mode.
fn write_artifacts_at(
    history: &Path,
    repo: &TempRepo,
    config: &CommitGraphConfig,
    as_of: &str,
    pending: Pending,
) -> CommitGraph {
    std::fs::create_dir_all(repo.path().join("docs/media")).expect("create the media directory");
    write_config(repo.path(), config);
    let (graph, json) = generate_with(history, config, Some(as_of), pending).expect("generate");
    std::fs::write(repo.path().join(JSON_PATH), json).expect("write the JSON");
    write_cards(repo.path(), &graph, &config.style);
    let permalink = commit_permalink(config, &graph.as_of_sha);
    std::fs::write(repo.path().join("README.md"), readme_text(&permalink))
        .expect("write the README");
    graph
}

/// The day (days since the epoch) of an ISO date, for pinned pending dates.
fn day_of(date: &str) -> i64 {
    parse_iso_date(date).expect("a fixture date")
}

fn write_artifacts(repo: &TempRepo, as_of: &str) -> CommitGraph {
    write_artifacts_from(repo.path(), repo, &config(), as_of)
}

/// A `commit-graph` invocation rooted at `repo` whose `--check` flag is set.
fn check_args(repo: &TempRepo) -> CommitGraphArgs {
    CommitGraphArgs {
        root: repo.path().to_path_buf(),
        as_of: None,
        config: PathBuf::from(CONFIG_PATH),
        out_dir: PathBuf::from("docs/media"),
        source: HistorySource::Auto,
        check: true,
        auto: false,
    }
}

#[test]
fn the_check_command_accepts_a_consistent_graph_at_head() {
    let repo = TempRepo::new("check-consistent");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "a\n")],
    );
    write_artifacts(&repo, "HEAD");
    cmd_commit_graph_check(&check_args(&repo)).expect("a consistent graph passes --check");
}

#[test]
fn the_check_command_warns_but_passes_when_the_graph_is_behind_head() {
    let repo = TempRepo::new("check-stale");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "a\n")],
    );
    // The graph is written at the first commit, then one more commit lands.
    write_artifacts(&repo, "HEAD");
    for index in 0..25 {
        repo.commit(
            "2026-10-11T09:00:00+05:30",
            "Pratyay Mukherjee",
            &[(format!("b{index}.txt").as_str(), "b\n")],
        );
    }
    // Staleness is a warning, never a failure: a push must not be blocked.
    cmd_commit_graph_check(&check_args(&repo)).expect("staleness does not fail --check");
}

#[test]
fn the_check_command_rejects_an_inconsistent_graph() {
    let repo = TempRepo::new("check-inconsistent");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "a\n")],
    );
    write_artifacts(&repo, "HEAD");
    // Corrupt one card's <desc> so it disagrees with the document.
    let card = std::fs::read_to_string(repo.path().join(SVG_PATH)).expect("the full card");
    let graph: CommitGraph =
        serde_json::from_str(&std::fs::read_to_string(repo.path().join(JSON_PATH)).expect("json"))
            .expect("valid graph");
    let edited = card.replacen(
        &format!("commits={}", graph.totals.commits),
        &format!("commits={}", graph.totals.commits + 1),
        1,
    );
    std::fs::write(repo.path().join(SVG_PATH), edited).expect("rewrite the full card");
    assert!(cmd_commit_graph_check(&check_args(&repo)).is_err());
}

#[test]
fn a_hand_edited_json_is_a_violation() {
    let repo = TempRepo::new("hand-edited");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "a\n")],
    );
    write_artifacts(&repo, "HEAD");
    let path = repo.path().join(JSON_PATH);
    let committed = std::fs::read_to_string(&path).expect("the document");
    let mut value: serde_json::Value = serde_json::from_str(&committed).expect("valid JSON");
    let commits = value["totals"]["commits"].as_u64().expect("a count");
    *value
        .pointer_mut("/totals/commits")
        .expect("the totals object has a commits count") = serde_json::json!(commits + 1);
    std::fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&value).expect("serialise")
        ),
    )
    .expect("rewrite the document");

    let outcome = check::check_docs(repo.path()).expect("the check runs");
    assert!(
        outcome
            .violations
            .iter()
            .any(|violation| violation.contains("does not reproduce")),
        "{:?}",
        outcome.violations
    );
}

/// Clone `repo` into a bare repository at `destination`.
fn bare_clone(repo: &TempRepo, destination: &Path) {
    let url = format!("file://{}", repo.path().display());
    let status = Command::new("git")
        .args(["clone", "--bare", "--quiet", &url])
        .arg(destination)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .status()
        .expect("run git clone --bare");
    assert!(status.success());
}

/// Three public commits, in a bare repository a cache can clone from.
fn public_bare(label: &str) -> (TempRepo, TempDir, PathBuf) {
    let work = TempRepo::new(label);
    for (index, day) in [10, 11, 12].into_iter().enumerate() {
        work.commit(
            &format!("2026-10-{day}T09:00:00+05:30"),
            "Pratyay Mukherjee",
            &[(format!("public{index}.txt").as_str(), "public\n")],
        );
    }
    let sandbox = TempDir::new(&format!("{label}-bare"));
    let bare = sandbox.root.join("public.git");
    bare_clone(&work, &bare);
    (work, sandbox, bare)
}

/// A checkout whose history is not the public one: five commits, two of them
/// private-only (the public repository shows three).
fn private_checkout(label: &str) -> TempRepo {
    let repo = TempRepo::new(label);
    for (index, day) in [10, 11, 12].into_iter().enumerate() {
        repo.commit(
            &format!("2026-10-{day}T10:00:00+05:30"),
            "Pratyay Mukherjee",
            &[(format!("public{index}.txt").as_str(), "public\n")],
        );
    }
    repo.commit(
        "2026-10-13T10:00:00+05:30",
        "Pratyay Mukherjee",
        &[("private-one.txt", "private only\n")],
    );
    repo.commit(
        "2026-10-14T10:00:00+05:30",
        "Pratyay Mukherjee",
        &[("private-two.txt", "private only\n")],
    );
    repo
}

fn config_for_public(bare: &Path) -> CommitGraphConfig {
    let mut config = config();
    config.source_url = format!("file://{}", bare.display());
    config.source_cache = "target/public-cache.git".to_string();
    config
}

#[test]
fn the_public_graph_excludes_private_only_commits_and_matches_rev_list() {
    let (_public_work, _public_dir, bare) = public_bare("cache-public");
    let private = private_checkout("cache-private");
    assert_eq!(
        private.git(&["rev-list", "--count", "HEAD"]).trim(),
        "5",
        "the private checkout has two extra commits"
    );
    let config = config_for_public(&bare);

    let history = acquire_history(private.path(), &config, HistorySource::Public)
        .expect("the public cache is populated");
    assert!(history.public);
    let (graph, json) = generate(&history.path, &config, Some(&config.source_ref))
        .expect("generate from the public cache");
    let expected: u64 = String::from_utf8_lossy(
        &run_git(&bare, &["rev-list", "--count", "refs/heads/main"]).expect("rev-list count"),
    )
    .trim()
    .parse()
    .expect("a count");
    assert_eq!(expected, 3);
    assert_eq!(graph.totals.commits, expected);
    assert!(!json.contains("private-one"));
    assert!(!json.contains("private-two"));
}

#[test]
fn an_unreachable_public_source_names_the_offline_local_option() {
    let repo = TempRepo::new("offline");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "a\n")],
    );
    let mut config = config();
    config.source_url = "file:///definitely/not/a/public/repository.git".to_string();
    config.source_cache = "target/absent-cache.git".to_string();
    let error = acquire_history(repo.path(), &config, HistorySource::Public)
        .expect_err("an unreachable source cannot be read");
    let message = error.to_string();
    assert!(message.contains("--source local"), "{message}");
    assert!(
        message.contains("must not be committed"),
        "the message must say an offline graph is not committable: {message}"
    );
    assert!(message.contains("public repository"), "{message}");
}

#[test]
fn a_public_clone_passes_the_check_from_local_history_without_the_cache() {
    let repo = TempRepo::new("public-clone-check");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "a\n")],
    );
    repo.commit(
        "2026-10-11T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("b.txt", "b\n")],
    );
    let mut config = config();
    // Deliberately unreachable: the local history contains the as-of commit,
    // so the check must never need the cache or the network.
    config.source_url = "https://example.invalid/public.git".to_string();
    config.source_cache = "target/absent-cache.git".to_string();
    write_artifacts_from(repo.path(), &repo, &config, "HEAD");

    let outcome = check::check_docs(repo.path()).expect("the check runs offline");
    assert!(outcome.violations.is_empty(), "{:#?}", outcome.violations);
    assert!(outcome.note.is_none(), "{:?}", outcome.note);
}

#[test]
fn a_private_checkout_reproduces_from_the_public_cache() {
    let (_public_work, _public_dir, bare) = public_bare("cache-check-public");
    let private = private_checkout("cache-check-private");
    let config = config_for_public(&bare);
    let history = acquire_history(private.path(), &config, HistorySource::Public)
        .expect("populate the cache");
    let graph = write_artifacts_from(&history.path, &private, &config, &config.source_ref);
    assert_eq!(graph.totals.commits, 3);

    let outcome = check::check_docs(private.path()).expect("the check runs");
    assert!(outcome.violations.is_empty(), "{:#?}", outcome.violations);
    let note = outcome.note.unwrap_or_default();
    assert!(note.contains("public-history cache"), "{note}");
    assert!(note.contains("staleness is not checked"), "{note}");
}

#[test]
fn a_commit_missing_from_the_cache_skips_the_history_checks_with_a_reason() {
    let repo = TempRepo::new("cache-missing");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "a\n")],
    );
    let mut config = config();
    config.source_cache = "target/absent-cache.git".to_string();
    let graph = write_artifacts_from(repo.path(), &repo, &config, "HEAD");
    // Rewrite the only commit so the artifact's as-of commit is no longer
    // reachable from HEAD, and no cache exists to contain it.
    repo.git(&["checkout", "-q", "--detach", "HEAD"]);
    let status = Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["commit", "--amend", "-q", "-m", "rewritten"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_DATE", "2026-10-11T09:00:00+05:30")
        .env("GIT_COMMITTER_DATE", "2026-10-11T09:00:00+05:30")
        .env("GIT_AUTHOR_NAME", "Pratyay Mukherjee")
        .env("GIT_AUTHOR_EMAIL", "harness@example.invalid")
        .env("GIT_COMMITTER_NAME", "Pratyay Mukherjee")
        .env("GIT_COMMITTER_EMAIL", "harness@example.invalid")
        .status()
        .expect("run git commit --amend");
    assert!(status.success());

    let outcome = check::check_docs(repo.path()).expect("the check runs offline");
    assert!(outcome.violations.is_empty(), "{:#?}", outcome.violations);
    let note = outcome.note.unwrap_or_default();
    assert!(note.contains("skipped"), "{note}");
    assert!(note.contains("not in this checkout"), "{note}");
    assert!(
        note.contains(&graph.as_of_sha[..7]),
        "the note names the missing commit: {note}"
    );
    assert!(note.contains("--source public"), "{note}");
}

#[test]
fn the_readme_permalink_tracks_the_public_as_of_commit() {
    let text = "# title\n\n<!-- commit-graph-strip:begin -->\n\
                <a href=\"https://example.invalid/public/commits/main\"><img src=\"strip.svg\"></a>\n\
                <!-- commit-graph-strip:end -->\n\n\
                <!-- commit-graph:begin -->\n\
                <a href=\"https://example.invalid/public/commits/main\"><img src=\"card.svg\"></a>\n\
                Generated by `cargo xtask commit-graph`.\n\
                <!-- commit-graph:end -->\n\nfooter\n";
    let rewritten = rewrite_readme_permalink(
        text,
        "https://example.invalid/public/commit/0123456789abcdef",
    )
    .expect("the blocks carry anchors");
    assert_eq!(
        rewritten
            .matches("href=\"https://example.invalid/public/commit/0123456789abcdef\"")
            .count(),
        2,
        "{rewritten}"
    );
    assert!(!rewritten.contains("commits/main"));
    assert!(rewritten.starts_with("# title\n\n<!-- commit-graph-strip:begin -->"));
    assert!(rewritten.ends_with("<!-- commit-graph:end -->\n\nfooter\n"));
    assert!(rewrite_readme_permalink("no block here", "https://x").is_none());
}

#[test]
fn remote_urls_compare_across_clone_forms() {
    assert_eq!(
        normalize_remote("git@github.com:aiori-hackathon/A3-PS036-TC529.git"),
        normalize_remote("https://github.com/aiori-hackathon/A3-PS036-TC529")
    );
    assert_eq!(
        normalize_remote("https://github.com/aiori-hackathon/A3-PS036-TC529.git/"),
        "https://github.com/aiori-hackathon/A3-PS036-TC529"
    );
    assert_ne!(
        normalize_remote("https://github.com/someone/CLIMB.git"),
        normalize_remote("https://github.com/aiori-hackathon/A3-PS036-TC529.git")
    );
}

/// A public history whose tip is the self-counted refresh commit.
///
/// Returns the work repository (which is also the checkout `--auto` runs in),
/// the sandbox that owns the bare public repository, the bare path, the
/// configuration, the as-of commit and the refresh commit.
fn public_with_refresh(
    label: &str,
) -> (
    TempRepo,
    TempDir,
    PathBuf,
    CommitGraphConfig,
    String,
    String,
) {
    let work = TempRepo::new(label);
    work.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "a\n")],
    );
    let as_of = work.commit(
        "2026-10-11T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("b.txt", "b\n")],
    );
    let sandbox = TempDir::new(&format!("{label}-public"));
    let bare = sandbox.root.join("public.git");
    bare_clone(&work, &bare);

    let mut config = config();
    config.source_url = format!("file://{}", bare.display());
    config.source_cache = "target/public-cache.git".to_string();
    config.refresh_commits_whose_files_all_match = vec!["docs/media/commit-graph*".to_string()];
    config.refresh_commits_allow_readme = true;
    config.self_count = true;

    write_artifacts_at(
        work.path(),
        &work,
        &config,
        &as_of,
        Pending::On(day_of("2026-10-12")),
    );
    let refresh = work.commit_paths(
        "2026-10-12T10:00:00+05:30",
        "Pratyay Mukherjee",
        "docs: refresh commit graph",
        &[
            JSON_PATH,
            SVG_PATH,
            NARROW_PATH,
            COMPACT_PATH,
            COMPACT_NARROW_PATH,
            "README.md",
        ],
    );
    let url = format!("file://{}", bare.display());
    work.git(&["push", "--quiet", &url, "main:main"]);
    (work, sandbox, bare, config, as_of, refresh)
}

/// A `commit-graph --auto` invocation rooted at `repo` reading the public
/// history through the cache.
fn auto_args(repo: &TempRepo) -> CommitGraphArgs {
    CommitGraphArgs {
        root: repo.path().to_path_buf(),
        as_of: None,
        config: PathBuf::from(CONFIG_PATH),
        out_dir: PathBuf::from("docs/media"),
        source: HistorySource::Public,
        check: false,
        auto: true,
    }
}

#[test]
fn a_self_counted_card_matches_rev_list_once_its_refresh_commit_exists() {
    let (work, _sandbox, _bare, config, as_of, refresh) = public_with_refresh("self-count");
    let committed = std::fs::read_to_string(work.path().join(JSON_PATH)).expect("the document");
    let graph: CommitGraph = serde_json::from_str(&committed).expect("a valid document");
    assert!(graph.pending_refresh, "the card counted its own refresh");
    assert_eq!(graph.pending_date.as_deref(), Some("2026-10-12"));
    assert_eq!(graph.totals.refresh_commits, 1);
    assert_eq!(graph.totals.commits, 3, "two commits plus the refresh");
    assert_eq!(
        rev_list_count(work.path(), "HEAD").expect("a count"),
        graph.totals.commits,
        "the number GitHub shows at the refresh commit"
    );
    assert_eq!(refresh, work.git(&["rev-parse", "HEAD"]).trim());

    // Regenerating from the parent with the recorded pending day reproduces
    // the committed bytes, even though the refresh commit now exists.
    let (_, again) = generate_with(
        work.path(),
        &config,
        Some(&as_of),
        Pending::On(day_of("2026-10-12")),
    )
    .expect("regenerate");
    assert_eq!(again, committed);
}

#[test]
fn auto_is_a_no_op_on_the_refresh_tip() {
    let (work, _sandbox, _bare, _config, _as_of, _refresh) = public_with_refresh("auto-current");
    let before = std::fs::read(work.path().join(JSON_PATH)).expect("the document");
    let outcome = run_auto(&auto_args(&work)).expect("auto runs");
    match outcome {
        AutoOutcome::UpToDate { tip, as_of } => {
            assert_eq!(tip, _refresh);
            assert_eq!(as_of, _as_of);
        }
        other => panic!("expected up to date, got {other:?}"),
    }
    let after = std::fs::read(work.path().join(JSON_PATH)).expect("the document");
    assert_eq!(before, after, "the no-op must not rewrite the document");
}

#[test]
fn a_new_commit_makes_the_card_stale_and_one_refresh_cycle_converges() {
    let (work, _sandbox, _bare, _config, _as_of, _refresh) = public_with_refresh("auto-converge");
    let real = work.commit(
        "2026-10-13T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("c.txt", "c\n")],
    );
    let url = {
        let config = load_config(&work.path().join(CONFIG_PATH)).expect("the configuration");
        config.source_url
    };
    work.git(&["push", "--quiet", &url, "main:main"]);

    let outcome = run_auto(&auto_args(&work)).expect("auto runs");
    match outcome {
        AutoOutcome::Changed {
            old_commits,
            new_commits,
            tip,
        } => {
            assert_eq!(tip, real);
            assert_eq!(old_commits, 3);
            assert_eq!(new_commits, 5, "four existing commits plus the new refresh");
        }
        other => panic!("expected a change, got {other:?}"),
    }
    let staged = work.git(&["diff", "--cached", "--name-only"]);
    assert!(staged.contains("docs/media/commit-graph.json"), "{staged}");
    assert!(
        staged.contains("README.md"),
        "the permalink travels with it"
    );

    // The publishing script commits exactly the staged files and publishes the
    // refresh commit; the next decision must converge.
    let refresh = work.commit_staged(
        "2026-10-13T10:00:00+05:30",
        "Pratyay Mukherjee",
        "docs: refresh commit graph",
    );
    work.git(&["push", "--quiet", &url, "main:main"]);
    let outcome = run_auto(&auto_args(&work)).expect("auto runs again");
    match outcome {
        AutoOutcome::UpToDate { tip, .. } => assert_eq!(tip, refresh),
        other => panic!("expected convergence, got {other:?}"),
    }
    let graph: CommitGraph = serde_json::from_str(
        &std::fs::read_to_string(work.path().join(JSON_PATH)).expect("the document"),
    )
    .expect("a valid document");
    assert_eq!(
        graph.totals.commits,
        rev_list_count(work.path(), "HEAD").expect("a count")
    );
}

#[test]
fn a_refresh_tip_whose_parent_is_not_the_as_of_commit_is_stale() {
    let (work, _sandbox, _bare, _config, as_of, _refresh) =
        public_with_refresh("auto-wrong-parent");
    // Claim the card describes an older commit than the one the refresh tip was
    // built on. The tip is media-only, but it is not *this* card's refresh.
    let path = work.path().join(JSON_PATH);
    let text = std::fs::read_to_string(&path).expect("the document");
    let mut value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    *value.pointer_mut("/as_of_sha").expect("as_of_sha") =
        serde_json::json!("0000000000000000000000000000000000000000");
    std::fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&value).expect("serialise")
        ),
    )
    .expect("rewrite the document");
    assert_ne!(
        value.pointer("/as_of_sha").and_then(|v| v.as_str()),
        Some(as_of.as_str())
    );
    let outcome = run_auto(&auto_args(&work)).expect("auto runs");
    assert!(
        matches!(outcome, AutoOutcome::Changed { .. }),
        "a refresh with the wrong parent is stale"
    );
}

#[test]
fn the_exactness_check_rejects_a_card_that_omits_its_refresh_commit() {
    let (work, _sandbox, _bare, _config, _as_of, refresh) = public_with_refresh("exactness");
    // The public clone's tip is the refresh commit and its first parent is the
    // as-of commit, so docs-check requires the count GitHub would show.
    let outcome = check::check_docs(work.path()).expect("the check runs");
    assert!(outcome.violations.is_empty(), "{:#?}", outcome.violations);

    let path = work.path().join(JSON_PATH);
    let text = std::fs::read_to_string(&path).expect("the document");
    let mut value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    let count = value
        .pointer("/totals/commits")
        .and_then(|v| v.as_u64())
        .expect("a count");
    *value.pointer_mut("/totals/commits").expect("commits") = serde_json::json!(count - 1);
    std::fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&value).expect("serialise")
        ),
    )
    .expect("rewrite the document");
    let outcome = check::check_docs(work.path()).expect("the check runs");
    let message = outcome
        .violations
        .iter()
        .find(|violation| violation.contains("the card must count its own refresh commit"))
        .unwrap_or_else(|| panic!("no exactness violation in {:#?}", outcome.violations));
    assert!(message.contains("git rev-list --count HEAD"), "{message}");
    assert!(message.contains(&(count - 1).to_string()), "{message}");
    assert!(message.contains(&count.to_string()), "{message}");
    assert_eq!(
        rev_list_count(work.path(), "HEAD").expect("a count"),
        count,
        "the fixture's refresh commit is {}",
        &refresh[..7]
    );
}

#[test]
fn a_refresh_may_carry_the_readme_permalink_but_a_readme_only_commit_may_not() {
    let repo = TempRepo::new("refresh-readme");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[
            ("docs/media/commit-graph.json", "{}"),
            ("README.md", "permalink\n"),
        ],
    );
    repo.commit(
        "2026-10-11T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("README.md", "prose only\n")],
    );
    let mut config = config();
    config.refresh_commits_whose_files_all_match = vec!["docs/media/commit-graph*".to_string()];
    config.refresh_commits_allow_readme = true;
    let (graph, _) = generate(repo.path(), &config, Some("HEAD")).expect("generate");
    assert_eq!(
        graph.totals.refresh_commits, 1,
        "the graph artifact plus README is one refresh; README alone is not"
    );

    config.refresh_commits_allow_readme = false;
    let (strict, _) = generate(repo.path(), &config, Some("HEAD")).expect("generate");
    assert_eq!(
        strict.totals.refresh_commits, 0,
        "without the allowance a README rewrite makes the commit something else"
    );
}

#[test]
fn self_count_adds_one_commit_on_the_pending_day_and_records_it() {
    let repo = TempRepo::new("self-count-fields");
    for day in 10..=12 {
        repo.commit(
            &format!("2026-10-{day}T09:00:00+05:30"),
            "Pratyay Mukherjee",
            &[(format!("f{day}.txt").as_str(), "a\n")],
        );
    }
    let mut config = config();
    config.self_count = true;
    config.refresh_commits_whose_files_all_match = vec!["docs/media/commit-graph*".to_string()];
    let pending = day_of("2026-10-13");
    let (graph, json) =
        generate_with(repo.path(), &config, Some("HEAD"), Pending::On(pending)).expect("generate");
    assert_eq!(graph.totals.commits, 4);
    assert_eq!(graph.totals.refresh_commits, 1);
    assert_eq!(graph.totals.active_days, 4);
    assert_eq!(graph.totals.longest_streak, 4);
    assert_eq!(graph.totals.current_streak, 4);
    assert_eq!(graph.pending_date.as_deref(), Some("2026-10-13"));
    assert_eq!(graph.range.end, "2026-10-13");
    assert_eq!(day(&graph, "2026-10-13").commits, 1);
    assert!(day(&graph, "2026-10-13").authors.is_empty());
    assert!(json.contains("\"pending_refresh\": true"));
    assert!(json.contains("\"pending_date\": \"2026-10-13\""));

    // The pending commit lands on an existing day without extending the range.
    let (same_day, _) = generate_with(
        repo.path(),
        &config,
        Some("HEAD"),
        Pending::On(day_of("2026-10-12")),
    )
    .expect("generate");
    assert_eq!(same_day.totals.commits, 4);
    assert_eq!(same_day.range.end, "2026-10-12");
    assert_eq!(day(&same_day, "2026-10-12").commits, 2);
    assert_eq!(same_day.totals.active_days, 3);

    // Disabled keeps the historical shape.
    let (plain, plain_json) =
        generate_with(repo.path(), &config, Some("HEAD"), Pending::Disabled).expect("generate");
    assert_eq!(plain.totals.commits, 3);
    assert_eq!(plain.totals.refresh_commits, 0);
    assert!(!plain.pending_refresh);
    assert!(plain.pending_date.is_none());
    assert!(plain_json.contains("\"pending_refresh\": false"));
    assert!(!plain_json.contains("pending_date"));
}

#[test]
fn a_config_without_self_count_keeps_the_historical_default() {
    let repo = TempRepo::new("defaults");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "a\n")],
    );
    let text = "[commit_graph]\n\
                timezone = \"Asia/Kolkata\"\n\
                budget_kb = 50\n";
    std::fs::create_dir_all(repo.path().join("docs")).expect("create the docs directory");
    std::fs::write(repo.path().join(CONFIG_PATH), text).expect("write the configuration");
    let config = load_config(&repo.path().join(CONFIG_PATH)).expect("load the configuration");
    assert!(config.self_count);
    assert!(config.refresh_commits_allow_readme);
    let (graph, _) = generate(repo.path(), &config, Some("HEAD")).expect("generate");
    assert!(graph.pending_refresh);
    assert_eq!(graph.totals.commits, 2);
}

#[test]
fn a_card_without_self_count_is_not_checked_against_its_refresh_tip() {
    let (work, _sandbox, _bare, config, as_of, _refresh) = public_with_refresh("no-self-count");
    // Replace the self-counted card with one that counts only commits that
    // exist, then amend the refresh commit so HEAD still carries it. The card
    // is consistent, and the exactness rule must not demand a pending commit
    // the document never claimed.
    write_artifacts_at(work.path(), &work, &config, &as_of, Pending::Disabled);
    work.git(&["add", "-A"]);
    work.git(&["commit", "--quiet", "--amend", "--no-edit"]);

    let graph: CommitGraph =
        serde_json::from_str(&std::fs::read_to_string(work.path().join(JSON_PATH)).expect("doc"))
            .expect("valid JSON");
    assert!(!graph.pending_refresh);
    assert_eq!(
        rev_list_count(work.path(), "HEAD").expect("a count"),
        graph.totals.commits + 1,
        "the refresh commit is one ahead of a document that does not count it"
    );

    let outcome = check::check_docs(work.path()).expect("the check runs");
    assert!(outcome.violations.is_empty(), "{:#?}", outcome.violations);
}
