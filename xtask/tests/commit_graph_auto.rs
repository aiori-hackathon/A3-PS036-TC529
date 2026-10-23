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

fn assert_clean_generated_surface(work: &Path) {
    let status = git_ok(work, &["status", "--porcelain"]);
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
