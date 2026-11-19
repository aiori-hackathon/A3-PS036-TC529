//! End-to-end tests for `cargo xtask commit-graph --auto` over a synthetic
//! public repository. The generator, the cache fetch and the process exit
//! status all run for real; only the remotes are local directories.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

const BIN: &str = env!("CARGO_BIN_EXE_xtask");

struct TempDir {
    root: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "climb-xtask-auto-{}-{serial}-{label}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create the temporary directory");
        Self { root }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn git(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Test Harness")
        .env("GIT_AUTHOR_EMAIL", "harness@example.invalid")
        .env("GIT_COMMITTER_NAME", "Test Harness")
        .env("GIT_COMMITTER_EMAIL", "harness@example.invalid")
        .env("GIT_AUTHOR_DATE", "2026-10-22T12:00:00+0530")
        .env("GIT_COMMITTER_DATE", "2026-10-22T12:00:00+0530")
        .output()
        .expect("run git")
}

fn git_ok(dir: &Path, args: &[&str]) -> String {
    let output = git(dir, args);
    assert!(
        output.status.success(),
        "git {args:?} in {} failed: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn init_repo(dir: &Path) {
    std::fs::create_dir_all(dir).expect("create the repository directory");
    git_ok(dir, &["init", "-q", "-b", "main"]);
    git_ok(dir, &["config", "user.name", "Test Harness"]);
    git_ok(dir, &["config", "user.email", "harness@example.invalid"]);
    git_ok(dir, &["config", "commit.gpgsign", "false"]);
}

fn write(dir: &Path, path: &str, text: &str) {
    let full = dir.join(path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).expect("create the parent directory");
    }
    std::fs::write(full, text).expect("write the fixture file");
}

fn commit_all(dir: &Path, message: &str) -> String {
    git_ok(dir, &["add", "-A"]);
    git_ok(dir, &["commit", "-q", "-m", message]);
    git_ok(dir, &["rev-parse", "HEAD"])
}

fn config_text(public_url: &str) -> String {
    format!(
        r#"[commit_graph]
timezone = "Asia/Kolkata"
source_url = "{public_url}"
source_ref = "main"
source_cache = "target/public.git"
repo_web_url = "https://example.invalid/public"
refresh_commits_whose_files_all_match = ["docs/media/commit-graph*"]
self_count = true
budget_kb = 50
strip_budget_kb = 25
"#
    )
}

fn readme_text() -> &'static str {
    "<!-- commit-graph-strip:begin -->\n\
     <a href=\"https://example.invalid/commit/old\"><img src=\"docs/media/commit-graph-compact.svg\" alt=\"commits\"></a>\n\
     <!-- commit-graph-strip:end -->\n\
     \n\
     <!-- commit-graph:begin -->\n\
     <a href=\"https://example.invalid/commit/old\"><img src=\"docs/media/commit-graph.svg\" alt=\"commits\"></a>\n\
     <!-- commit-graph:end -->\n"
}

fn run_auto(work: &Path) -> Output {
    Command::new(BIN)
        .arg("commit-graph")
        .arg("--auto")
        .arg("--source")
        .arg("public")
        .arg("--today")
        .arg("2026-11-18")
        .arg("--root")
        .arg(work)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("run xtask")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn json(work: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(work.join("docs/media/commit-graph.json"))
        .expect("the generated document");
    serde_json::from_str(&text).expect("valid JSON")
}

fn copy_generated(work: &Path, public: &Path) {
    for name in [
        "commit-graph.json",
        "commit-graph.svg",
        "commit-graph-narrow.svg",
        "commit-graph-compact.svg",
        "commit-graph-compact-narrow.svg",
    ] {
        let from = work.join("docs/media").join(name);
        let to = public.join("docs/media").join(name);
        std::fs::create_dir_all(to.parent().expect("artifact parent")).expect("create media dir");
        std::fs::copy(from, to).expect("copy the generated artifact");
    }
    std::fs::copy(work.join("README.md"), public.join("README.md"))
        .expect("copy the README permalink rewrite");
}

fn git_status(dir: &Path) -> String {
    String::from_utf8_lossy(
        &Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["status", "--porcelain", "-uall"])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .expect("run git status")
            .stdout,
    )
    .to_string()
}

fn assert_clean_generated_surface(work: &Path) {
    let status = git_status(work);
    let mut seen = 0usize;
    for line in status.lines() {
        let path = line.get(3..).unwrap_or(line);
        assert!(
            path == "README.md"
                || (path.starts_with("docs/media/commit-graph") && path.ends_with(".svg"))
                || path == "docs/media/commit-graph.json",
            "the refresh staged an unexpected path: {line}"
        );
        seen += 1;
    }
    assert!(seen > 0, "the refresh should stage at least one file");
}

#[test]
fn auto_reports_up_to_date_then_changed_then_converges() {
    let temp = TempDir::new("cycle");
    let seed = temp.root.join("seed");
    init_repo(&seed);
    write(&seed, "notes/a.txt", "one\n");
    commit_all(&seed, "first");
    write(&seed, "notes/b.txt", "two\n");
    commit_all(&seed, "second");
    let public = temp.root.join("public.git");
    assert!(
        Command::new("git")
            .args(["clone", "--bare", "--quiet"])
            .arg(&seed)
            .arg(&public)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .status()
            .expect("clone the public repository")
            .success(),
        "the bare clone succeeds"
    );

    let work = temp.root.join("work");
    init_repo(&work);
    write(
        &work,
        "docs/readme-config.toml",
        &config_text(&public.display().to_string()),
    );
    write(&work, "README.md", readme_text());
    write(&work, ".gitignore", "target/\n");
    commit_all(&work, "work");

    let generated = Command::new(BIN)
        .args(["commit-graph", "--source", "public", "--root"])
        .arg(&work)
        .args(["--today", "2026-11-18"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("run the generator");
    assert!(
        generated.status.success(),
        "the generator succeeds: {}",
        stderr(&generated)
    );
    commit_all(&work, "docs: refresh commit graph");

    // Publish the first card as a refresh commit whose parent is the public tip
    // the generator stamped, exactly as a sync would.
    let public_work = temp.root.join("public-work");
    assert!(Command::new("git")
        .args(["clone", "--quiet"])
        .arg(&public)
        .arg(&public_work)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .status()
        .expect("clone the public working copy")
        .success());
    git_ok(&public_work, &["config", "user.name", "Test Harness"]);
    git_ok(
        &public_work,
        &["config", "user.email", "harness@example.invalid"],
    );
    copy_generated(&work, &public_work);
    let first_refresh = commit_all(&public_work, "refresh the card");
    git_ok(&public_work, &["push", "--quiet", "origin", "main"]);
    let stamped = json(&work)["as_of_sha"]
        .as_str()
        .expect("as_of_sha")
        .to_string();
    assert_eq!(
        git_ok(&public_work, &["rev-parse", &format!("{first_refresh}^")]),
        stamped,
        "the refresh commit's first parent is the stamped as-of commit"
    );

    // 1. The card now describes exactly the public tip: no work, exit 0.
    let before = std::fs::read(work.join("docs/media/commit-graph.json")).expect("card bytes");
    let up_to_date = run_auto(&work);
    assert_eq!(
        up_to_date.status.code(),
        Some(0),
        "up to date exits zero: {}",
        stderr(&up_to_date)
    );
    assert!(
        stdout(&up_to_date).contains("up to date"),
        "{}",
        stdout(&up_to_date)
    );
    let after = std::fs::read(work.join("docs/media/commit-graph.json")).expect("card bytes");
    assert_eq!(before, after, "up to date must not rewrite the card");
    assert!(
        git_ok(&work, &["status", "--porcelain"]).is_empty(),
        "up to date must not stage anything"
    );

    // 2. A new public commit makes the card stale: regenerate, stage, exit 10.
    write(&public_work, "notes/c.txt", "three\n");
    let third = commit_all(&public_work, "third");
    git_ok(&public_work, &["push", "--quiet", "origin", "main"]);

    let changed = run_auto(&work);
    assert_eq!(
        changed.status.code(),
        Some(10),
        "a stale card exits ten: {}",
        stderr(&changed)
    );
    assert!(stdout(&changed).contains("changed"), "{}", stdout(&changed));
    let document = json(&work);
    assert_eq!(document["as_of_sha"], third);
    assert_eq!(document["pending_refresh"], true);
    let pending = document["pending_date"]
        .as_str()
        .expect("the pending day is recorded");
    assert_eq!(pending.len(), 10, "the pending day is an ISO date");
    let tip_count: u64 = git_ok(&public_work, &["rev-list", "--count", &third])
        .parse()
        .expect("a commit count");
    assert_eq!(
        document["totals"]["commits"].as_u64(),
        Some(tip_count + 1),
        "a self-counted card is one ahead of the stamped tip"
    );
    assert_clean_generated_surface(&work);

    // 3. Publish that refresh and require convergence: exit 0, up to date.
    copy_generated(&work, &public_work);
    commit_all(&public_work, "refresh the card");
    git_ok(&public_work, &["push", "--quiet", "origin", "main"]);
    // The local checkout commits the same generated files, as publish.sh does.
    commit_all(&work, "docs: refresh commit graph");

    let converged = run_auto(&work);
    assert_eq!(
        converged.status.code(),
        Some(0),
        "the second pass converges: {}",
        stderr(&converged)
    );
    assert!(
        stdout(&converged).contains("up to date"),
        "{}",
        stdout(&converged)
    );
    assert!(
        git_ok(&work, &["status", "--porcelain"]).is_empty(),
        "convergence leaves nothing staged"
    );
}

#[test]
fn auto_refuses_an_explicit_as_of() {
    let temp = TempDir::new("as-of");
    let work = temp.root.join("work");
    init_repo(&work);
    write(
        &work,
        "docs/readme-config.toml",
        &config_text("https://example.invalid/public.git"),
    );
    write(&work, "README.md", readme_text());
    commit_all(&work, "work");

    let output = Command::new(BIN)
        .args(["commit-graph", "--auto", "--as-of", "HEAD", "--root"])
        .arg(&work)
        .output()
        .expect("run xtask");
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("omit --as-of"),
        "{}",
        stderr(&output)
    );
}

/// `config_text` plus the two live milestones the committed card draws.
fn config_text_with_milestones(public_url: &str) -> String {
    format!(
        "{}\n[[commit_graph.milestones]]\ndate = \"2026-11-19\"\nlabel = \"Regional, IIEST Shibpur\"\nshort = \"Reg\"\n\n\
         [[commit_graph.milestones]]\ndate = \"2026-12-17\"\nlabel = \"Finals, IIT Jammu\"\nshort = \"Finals\"\n",
        config_text(public_url)
    )
}

fn commit_all_at(dir: &Path, message: &str, date: &str) -> String {
    let add = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["add", "-A"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("run git add");
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let commit = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["commit", "-q", "--allow-empty", "-m", message])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Test Harness")
        .env("GIT_AUTHOR_EMAIL", "harness@example.invalid")
        .env("GIT_COMMITTER_NAME", "Test Harness")
        .env("GIT_COMMITTER_EMAIL", "harness@example.invalid")
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .output()
        .expect("run git commit");
    assert!(
        commit.status.success(),
        "{}",
        String::from_utf8_lossy(&commit.stderr)
    );
    git_ok(dir, &["rev-parse", "HEAD"])
}

fn bare_public(seed: &Path, public: &Path) {
    assert!(Command::new("git")
        .args(["clone", "--bare", "--quiet"])
        .arg(seed)
        .arg(public)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .status()
        .expect("clone the public repository")
        .success());
}

fn clone_from(url: &str, destination: &Path, depth: Option<&str>) {
    let mut command = Command::new("git");
    command.args(["clone", "--quiet", "--no-local"]);
    if let Some(depth) = depth {
        command.args(["--depth", depth]);
    }
    command
        .arg(url)
        .arg(destination)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null");
    assert!(
        command.status().expect("run git clone").success(),
        "clone from {url} failed"
    );
}

/// A public-shaped checkout: full depth, no local objects borrowed from the
/// source repository, and an `origin` that is the public URL.
#[test]
fn a_full_depth_clone_regenerates_the_card_from_its_own_history() {
    let temp = TempDir::new("clean-clone");
    let seed = temp.root.join("seed");
    let public = temp.root.join("public.git");
    let url = format!("file://{}", public.display());
    init_repo(&seed);
    write(&seed, "docs/readme-config.toml", &config_text(&url));
    write(&seed, "README.md", readme_text());
    write(&seed, ".gitignore", "target/\n");
    commit_all_at(&seed, "first", "2026-11-16T09:00:00+0530");
    write(&seed, "notes/a.txt", "one\n");
    commit_all_at(&seed, "second", "2026-11-17T09:00:00+0530");
    bare_public(&seed, &public);

    let clone = temp.root.join("clone");
    clone_from(&url, &clone, None);
    assert_eq!(
        git_ok(&clone, &["rev-parse", "--is-shallow-repository"]),
        "false"
    );

    let generated = Command::new(BIN)
        .args(["commit-graph", "--source", "auto", "--root"])
        .arg(&clone)
        .args(["--today", "2026-11-18"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("run the generator");
    assert!(
        generated.status.success(),
        "the clean clone regenerates the card: {}",
        stderr(&generated)
    );
    assert_eq!(
        json(&clone)["as_of_sha"],
        git_ok(&clone, &["rev-parse", "HEAD"])
    );
    assert_clean_generated_surface(&clone);
}

#[test]
fn a_shallow_clone_is_refused_with_the_fetch_depth_fix() {
    let temp = TempDir::new("shallow");
    let seed = temp.root.join("seed");
    let public = temp.root.join("public.git");
    let url = format!("file://{}", public.display());
    init_repo(&seed);
    write(&seed, "docs/readme-config.toml", &config_text(&url));
    write(&seed, "README.md", readme_text());
    commit_all_at(&seed, "first", "2026-11-16T09:00:00+0530");
    write(&seed, "notes/a.txt", "one\n");
    commit_all_at(&seed, "second", "2026-11-17T09:00:00+0530");
    bare_public(&seed, &public);
    let shallow = temp.root.join("shallow");
    clone_from(&url, &shallow, Some("1"));

    let refused = Command::new(BIN)
        .args(["commit-graph", "--source", "auto", "--root"])
        .arg(&shallow)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("run the generator");
    assert!(!refused.status.success(), "a shallow clone is refused");
    let message = stderr(&refused);
    assert!(message.contains("git fetch --unshallow"), "{message}");
    assert!(message.contains("fetch-depth 0"), "{message}");
    assert!(
        !shallow.join("docs/media/commit-graph.json").exists(),
        "the refusal must write nothing"
    );
}

/// A user's global configuration must not be able to change the URL the card
/// is read from, or to run a credential helper. The fixture rewrites the
/// configured URL to a repository that *would* work, so the test fails if the
/// rewrite is honoured.
#[test]
fn a_user_gitconfig_cannot_rewrite_the_url_or_run_a_credential_helper() {
    let temp = TempDir::new("rewrite");
    let real = temp.root.join("real");
    init_repo(&real);
    write(&real, "notes/a.txt", "one\n");
    commit_all(&real, "first");
    let real_public = temp.root.join("real/public.git");
    bare_public(&real, &real_public);

    let home = temp.root.join("home");
    std::fs::create_dir_all(&home).expect("create the temporary home");
    let marker = temp.root.join("helper-ran");
    let absent = temp.root.join("absent");
    write(
        &home,
        ".gitconfig",
        &format!(
            "[url \"ssh://git@127.0.0.1:1/\"]\n\tinsteadOf = https://example.invalid/\n\
             [url \"file://{real}/\"]\n\tinsteadOf = file://{absent}/\n\
             [credential]\n\thelper = \"!f() {{ echo ran > {marker}; }}; f\"\n",
            real = real.display(),
            absent = absent.display(),
            marker = marker.display(),
        ),
    );

    let work = temp.root.join("work");
    init_repo(&work);
    let url = format!("file://{}/public.git", absent.display());
    write(&work, "docs/readme-config.toml", &config_text(&url));
    write(&work, "README.md", readme_text());
    write(&work, ".gitignore", "target/\n");
    commit_all(&work, "work");

    let output = Command::new(BIN)
        .args(["commit-graph", "--source", "public", "--root"])
        .arg(&work)
        .args(["--today", "2026-11-18"])
        .env("HOME", &home)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("GIT_CONFIG_SYSTEM")
        .output()
        .expect("run the generator");
    assert!(
        !output.status.success(),
        "the rewrite must not make the absent URL readable: {}",
        stdout(&output)
    );
    let message = stderr(&output);
    assert!(
        message.contains(&format!("cannot reach {url}:")),
        "the failure must name the configured URL: {message}"
    );
    assert!(
        !marker.exists(),
        "a credential helper from the user's configuration ran"
    );
    assert!(
        !work.join("target/public.git").exists(),
        "the cache must not be populated from a rewritten URL"
    );
}

/// The class the root-cause report proved: the plotted range ends a couple of
/// days before an upcoming milestone whose label is wider than the runway lane.
#[test]
fn a_public_tip_near_an_upcoming_milestone_still_refreshes() {
    let temp = TempDir::new("near-milestone");
    let seed = temp.root.join("seed");
    let public = temp.root.join("public.git");
    let url = format!("file://{}", public.display());
    init_repo(&seed);
    write(
        &seed,
        "docs/readme-config.toml",
        &config_text_with_milestones(&url),
    );
    write(&seed, "README.md", readme_text());
    write(&seed, ".gitignore", "target/\n");
    commit_all_at(&seed, "first", "2026-09-26T09:00:00+0530");
    write(&seed, "notes/a.txt", "one\n");
    commit_all_at(&seed, "second", "2026-11-17T09:00:00+0530");
    bare_public(&seed, &public);

    let work = temp.root.join("work");
    init_repo(&work);
    write(
        &work,
        "docs/readme-config.toml",
        &config_text_with_milestones(&url),
    );
    write(&work, "README.md", readme_text());
    write(&work, ".gitignore", "target/\n");
    write(
        &work,
        "docs/media/commit-graph.json",
        "{\n  \"totals\": {\n    \"commits\": 1,\n    \"other\": 0\n  }\n}\n",
    );
    commit_all(&work, "work");

    // A first writing-form run produces a valid card; the human's commit makes
    // it the committed document.
    let first = Command::new(BIN)
        .args(["commit-graph", "--source", "public", "--root"])
        .arg(&work)
        .args(["--today", "2026-11-18"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("run the generator");
    assert!(
        first.status.success(),
        "the generator renders the card near the milestone: {}",
        stderr(&first)
    );
    commit_all(&work, "docs: refresh commit graph");

    // New public work makes the card stale, exactly as in the publishing cycle.
    write(&seed, "notes/b.txt", "two\n");
    commit_all_at(&seed, "third", "2026-11-17T10:00:00+0530");
    assert!(Command::new("git")
        .arg("-C")
        .arg(&seed)
        .args(["push", "--quiet", &url, "main:main"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .status()
        .expect("push the new public commit")
        .success());
    let changed = run_auto(&work);
    assert_eq!(
        changed.status.code(),
        Some(10),
        "a stale card is regenerated and staged: {}",
        stderr(&changed)
    );
    let card = std::fs::read_to_string(work.join("docs/media/commit-graph.svg"))
        .expect("the wide card was written");
    let document = roxmltree::Document::parse(&card).expect("the card is well-formed XML");
    let edge: Vec<roxmltree::Node<'_, '_>> = document
        .descendants()
        .filter(|node| {
            node.attribute("class")
                .is_some_and(|class| class.split_whitespace().any(|token| token == "cg-fill"))
        })
        .collect();
    assert!(!edge.is_empty(), "the card carries the filled staircase");
    for node in document.descendants() {
        let name = node.attribute("data-name").unwrap_or_default();
        if node.attribute("class") != Some("cg-text-group") || !name.starts_with("milestone-") {
            continue;
        }
        let box_: Vec<f64> = node
            .attribute("data-bbox")
            .expect("a bbox")
            .split_whitespace()
            .map(|part| part.parse().expect("a number"))
            .collect();
        let (x, y, width, height) = (box_[0], box_[1], box_[2], box_[3]);
        for mark in &edge {
            for segment in path_segments(mark.attribute("d").expect("a path")) {
                assert!(
                    !segment_intersects_rect(
                        segment.0,
                        segment.1,
                        segment.2,
                        segment.3,
                        (x, y, width, height)
                    ),
                    "{name} crosses the staircase edge at {:?}",
                    segment
                );
            }
        }
    }
}

fn path_segments(d: &str) -> Vec<(f64, f64, f64, f64)> {
    let mut numbers: Vec<(f64, f64)> = Vec::new();
    let mut current = String::new();
    let mut xs = Vec::new();
    for ch in d.chars() {
        if ch.is_ascii_alphabetic() || ch.is_whitespace() || ch == ',' {
            if !current.is_empty() {
                xs.push(current.parse::<f64>().expect("a numeric path component"));
                current.clear();
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        xs.push(current.parse::<f64>().expect("a numeric path component"));
    }
    let mut index = 0;
    while index + 1 < xs.len() {
        numbers.push((xs[index], xs[index + 1]));
        index += 2;
    }
    numbers
        .windows(2)
        .map(|pair| (pair[0].0, pair[0].1, pair[1].0, pair[1].1))
        .collect()
}

fn segment_intersects_rect(x1: f64, y1: f64, x2: f64, y2: f64, rect: (f64, f64, f64, f64)) -> bool {
    let (bx, by, bw, bh) = rect;
    let inside = |px: f64, py: f64| px >= bx && px <= bx + bw && py >= by && py <= by + bh;
    if inside(x1, y1) || inside(x2, y2) {
        return true;
    }
    // Only axis-aligned segments occur in the staircase; a vertical edge is
    // crossed when it lies inside the box's horizontal span and the box's
    // vertical span overlaps the segment.
    if (x1 - x2).abs() < 1e-9 {
        return x1 >= bx && x1 <= bx + bw && y1.min(y2) <= by + bh && by <= y1.max(y2);
    }
    if (y1 - y2).abs() < 1e-9 {
        return y1 >= by && y1 <= by + bh && x1.min(x2) <= bx + bw && bx <= x1.max(x2);
    }
    false
}

/// The document is a function of the history and the configuration, never of
/// the developer's git configuration. The fixture's `.gitconfig` marks every
/// path as binary, which would zero the line counts and change the JSON if the
/// generator read it.
#[test]
fn a_user_gitconfig_cannot_change_the_generated_document() {
    let temp = TempDir::new("config-proof");
    let seed = temp.root.join("seed");
    let public = temp.root.join("public.git");
    let url = format!("file://{}", public.display());
    init_repo(&seed);
    write(&seed, "notes/a.txt", "one\ntwo\n");
    commit_all_at(&seed, "first", "2026-11-16T09:00:00+0530");
    write(&seed, "notes/a.txt", "one\ntwo\nthree\nfour\n");
    commit_all_at(&seed, "second", "2026-11-17T09:00:00+0530");
    bare_public(&seed, &public);

    let work = temp.root.join("work");
    init_repo(&work);
    write(&work, "docs/readme-config.toml", &config_text(&url));
    write(&work, "README.md", readme_text());
    write(&work, ".gitignore", "target/\n");
    commit_all(&work, "work");

    let clean_home = temp.root.join("home-clean");
    std::fs::create_dir_all(&clean_home).expect("create the clean home");
    let biased_home = temp.root.join("home-biased");
    std::fs::create_dir_all(&biased_home).expect("create the biased home");
    let attributes = temp.root.join("attributes");
    std::fs::write(&attributes, "* -diff\n").expect("write the attributes file");
    write(
        &biased_home,
        ".gitconfig",
        &format!("[core]\n\tattributesFile = {}\n", attributes.display()),
    );

    let generate = |home: &Path| {
        Command::new(BIN)
            .args(["commit-graph", "--source", "public", "--root"])
            .arg(&work)
            .args(["--today", "2026-11-18"])
            .env("HOME", home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("GIT_CONFIG_GLOBAL")
            .output()
            .expect("run the generator")
    };
    let clean = generate(&clean_home);
    assert!(
        clean.status.success(),
        "the clean home generates the card: {}",
        stderr(&clean)
    );
    let clean_json =
        std::fs::read(work.join("docs/media/commit-graph.json")).expect("the document");
    let biased = generate(&biased_home);
    assert!(
        biased.status.success(),
        "the biased home still generates the card: {}",
        stderr(&biased)
    );
    let biased_json =
        std::fs::read(work.join("docs/media/commit-graph.json")).expect("the document");
    assert_eq!(
        clean_json, biased_json,
        "the generated document changed with the user's git configuration"
    );
}

/// The generator's date is an input: `--today` and `CLIMB_TODAY` set the
/// pending day, the flag wins over the environment, and a malformed date is
/// refused before anything is written.
#[test]
fn the_clock_can_be_pinned_by_the_flag_or_the_environment() {
    let temp = TempDir::new("pinned-clock");
    let work = temp.root.join("work");
    init_repo(&work);
    write(
        &work,
        "docs/readme-config.toml",
        &config_text_with_milestones("https://example.invalid/public.git"),
    );
    write(&work, "README.md", readme_text());
    commit_all_at(&work, "first", "2026-11-17T09:00:00+0530");

    let run = |flag: Option<&str>, env_today: Option<&str>| {
        let mut command = Command::new(BIN);
        command
            .args(["commit-graph", "--source", "local", "--root"])
            .arg(&work)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null");
        if let Some(day) = flag {
            command.args(["--today", day]);
        }
        match env_today {
            Some(day) => {
                command.env("CLIMB_TODAY", day);
            }
            None => {
                command.env_remove("CLIMB_TODAY");
            }
        }
        command.output().expect("run the generator")
    };

    let flagged = run(Some("2026-11-18"), None);
    assert!(
        flagged.status.success(),
        "--today renders the card: {}",
        stderr(&flagged)
    );
    assert_eq!(json(&work)["pending_date"], "2026-11-18");

    let from_env = run(None, Some("2026-11-18"));
    assert!(
        from_env.status.success(),
        "CLIMB_TODAY renders the card: {}",
        stderr(&from_env)
    );
    assert_eq!(json(&work)["pending_date"], "2026-11-18");

    let flag_wins = run(Some("2026-11-19"), Some("2026-11-18"));
    assert!(
        flag_wins.status.success(),
        "the flag wins over the environment: {}",
        stderr(&flag_wins)
    );
    assert_eq!(json(&work)["pending_date"], "2026-11-19");

    let malformed = run(Some("2026-02-31"), None);
    assert!(
        !malformed.status.success(),
        "a date that does not exist is refused"
    );
    assert!(
        stderr(&malformed).contains("is not an ISO date"),
        "the refusal names the input: {}",
        stderr(&malformed)
    );
}
