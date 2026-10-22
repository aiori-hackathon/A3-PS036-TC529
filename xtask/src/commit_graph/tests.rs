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
        repo.git(&["init", "-q"]);
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
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.dir.root)
            .args(["commit", "-q", "--allow-empty", "-m", "fixture"])
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
        since: None,
        exclude_authors: Vec::new(),
        display_names: BTreeMap::new(),
        ignore_commits_whose_files_all_match: Vec::new(),
        milestones: Vec::new(),
        budget_kb: 50,
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
fn the_excluded_bot_never_enters_the_graph() {
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
    assert_eq!(graph.totals.commits, 1);
    assert_eq!(graph.per_author.len(), 1);
    assert!(!json.contains("github-actions"));
    assert!(!json.contains("bot"));
}

#[test]
fn a_commit_is_ignored_only_when_every_path_matches() {
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
    config.ignore_commits_whose_files_all_match = vec!["docs/media/commit-graph*".to_string()];
    let (graph, _) = generate(repo.path(), &config, Some("HEAD")).expect("generate");
    assert_eq!(graph.totals.commits, 2);
    // The ignored day precedes the first counted commit, so it is not part of
    // the range at all.
    assert_eq!(graph.range.start, "2026-10-11");
    assert_eq!(day(&graph, "2026-10-11").commits, 1);
    assert_eq!(graph.totals.files_touched, 3);
}

#[test]
fn an_empty_commit_is_not_swept_up_by_the_path_rule() {
    let repo = TempRepo::new("empty");
    repo.commit("2026-10-10T09:00:00+05:30", "Pratyay Mukherjee", &[]);
    let mut config = config();
    config.ignore_commits_whose_files_all_match = vec!["docs/media/commit-graph*".to_string()];
    let (graph, _) = generate(repo.path(), &config, Some("HEAD")).expect("generate");
    assert_eq!(graph.totals.commits, 1);
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
    std::fs::write(
        clone.join(CONFIG_PATH),
        "[commit_graph]\ntimezone = \"Asia/Kolkata\"\nbudget_kb = 50\n",
    )
    .expect("write the configuration");
    let (graph, json) = generate(&clone, &config(), Some("HEAD")).expect("generate");
    let palette = svg::Palette::default();
    let full = svg::render_full(&graph, &palette).expect("full card");
    let compact = svg::render_compact(&graph, &palette).expect("compact card");
    let json_path = clone.join(JSON_PATH);
    let full_path = clone.join(SVG_PATH);
    let compact_path = clone.join(COMPACT_PATH);
    std::fs::write(&json_path, json).expect("write the JSON");
    std::fs::write(&full_path, full).expect("write the full card");
    std::fs::write(&compact_path, compact).expect("write the compact card");

    let outcome = check::check_with_paths(&clone, &json_path, &full_path, &compact_path)
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

    let palette = svg::Palette::default();
    let first_full = svg::render_full(&first_graph, &palette).expect("first full card");
    let second_full = svg::render_full(&second_graph, &palette).expect("second full card");
    let first_compact = svg::render_compact(&first_graph, &palette).expect("first compact card");
    let second_compact = svg::render_compact(&second_graph, &palette).expect("second compact card");
    assert_eq!(first_full, second_full);
    assert_eq!(first_compact, second_compact);
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
    let palette = svg::Palette::default();
    for (label, card) in [
        (
            "full",
            svg::render_full(&graph, &palette).expect("full card"),
        ),
        (
            "compact",
            svg::render_compact(&graph, &palette).expect("compact card"),
        ),
    ] {
        assert!(
            card.len() <= 50 * 1024,
            "{label} card is {} bytes",
            card.len()
        );
        let document = roxmltree::Document::parse(&card)
            .unwrap_or_else(|error| panic!("{label} card is not XML: {error}"));
        assert_eq!(document.root_element().tag_name().name(), "svg");
        assert_eq!(document.root_element().attribute("role"), Some("img"));
        assert!(document
            .descendants()
            .any(|node| node.has_tag_name("title")));
        assert!(document.descendants().any(|node| node.has_tag_name("desc")));
        assert!(!document.descendants().any(|node| node.has_tag_name("text")));
        assert!(card.contains("<path id=\"g"));
        assert!(card.contains("<use href=\"#g"));
        let remaining = card.replace("http://www.w3.org/2000/svg", "");
        assert!(
            !remaining.contains("http"),
            "{label} card has an http reference"
        );
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
    let card = svg::render_full(&graph, &palette).expect("card");
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
    assert_eq!(tokens.get("lines_added"), Some(&"2"));
    assert_eq!(tokens.get("date"), Some(&"2026-10-10"));
    assert_eq!(tokens.get("as_of"), Some(&&graph.as_of_sha[..7]));
}

#[test]
fn the_committed_document_reproduces_from_its_own_as_of_commit() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives inside the workspace");
    let committed = std::fs::read_to_string(root.join(JSON_PATH)).expect("committed graph");
    let graph: CommitGraph = serde_json::from_str(&committed).expect("valid graph");
    let config = load_config(&root.join(CONFIG_PATH)).expect("config");
    let (_, regenerated) = generate(root, &config, Some(&graph.as_of_sha)).expect("regenerate");
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
fn since_bounds_the_range_and_rejects_a_future_start() {
    let repo = TempRepo::new("since");
    repo.commit(
        "2026-10-10T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("a.txt", "a\n")],
    );
    repo.commit(
        "2026-10-15T09:00:00+05:30",
        "Pratyay Mukherjee",
        &[("b.txt", "b\n")],
    );
    let mut config = config();
    config.since = Some("2026-10-12".to_string());
    let (graph, _) = generate(repo.path(), &config, Some("HEAD")).expect("generate");
    assert_eq!(graph.range.start, "2026-10-12");
    assert_eq!(graph.totals.commits, 1);
    assert_eq!(graph.per_day.len(), 4);

    config.since = Some("2027-01-01".to_string());
    assert!(generate(repo.path(), &config, Some("HEAD")).is_err());
}
