//! Tests for `scripts/publish.sh`. The sync script and the generator are
//! replaced with small fakes so the test can drive every branch without a
//! network or a real sync. Git, the lock and the published-count comparison run
//! for real against local bare repositories.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace")
        .join("scripts/publish.sh")
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

fn commit_all(dir: &Path, message: &str) {
    git_ok(dir, &["add", "-A"]);
    git_ok(dir, &["commit", "-q", "-m", message]);
}

fn bare_clone(source: &Path, destination: &Path) {
    let output = Command::new("git")
        .args(["clone", "--bare", "--quiet"])
        .arg(source)
        .arg(destination)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("clone a bare repository");
    assert!(
        output.status.success(),
        "clone failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    sync_log: PathBuf,
    auto_state: PathBuf,
    sync_script: PathBuf,
    xtask_script: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fixture(label: &str, public_commits: usize) -> Fixture {
    let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!(
        "climb-publish-{}-{serial}-{label}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create the fixture directory");

    // A public repository with a known commit count.
    let seed = root.join("public-seed");
    init_repo(&seed);
    for index in 0..public_commits {
        write(&seed, &format!("public/{index}.txt"), &format!("{index}\n"));
        commit_all(&seed, &format!("public {index}"));
    }
    let public = root.join("public.git");
    bare_clone(&seed, &public);

    // The private repository that publish.sh sees.
    let private = root.join("private.git");
    init_repo(&root.join("private-seed"));
    let private_seed = root.join("private-seed");
    bare_clone(&private_seed, &private);
    let repo = root.join("repo");
    init_repo(&repo);
    git_ok(
        &repo,
        &["remote", "add", "origin", &private.display().to_string()],
    );
    write(
        &repo,
        "docs/readme-config.toml",
        &format!(
            "[commit_graph]\nsource_ref = \"main\"\nsource_cache = \"{}\"\n",
            public.display()
        ),
    );
    write(
        &repo,
        "docs/media/commit-graph.json",
        "{\n  \"totals\": {\n    \"commits\": 2,\n    \"other\": 0\n  }\n}\n",
    );
    write(&repo, "README.md", "# fixture\n");
    write(&repo, ".gitignore", "target/\n");
    commit_all(&repo, "work");
    git_ok(&repo, &["push", "--quiet", "-u", "origin", "main"]);

    // Fakes: the sync records its calls, the generator changes once and then
    // reports convergence.
    let sync_log = root.join("sync.log");
    let sync_script = root.join("fake-sync.sh");
    write(
        &root,
        "fake-sync.sh",
        &format!(
            "#!/bin/bash\nset -euo pipefail\necho \"$PWD\" >> {log:?}\nexit \"${{SYNC_EXIT:-0}}\"\n",
            log = sync_log.display().to_string()
        ),
    );
    make_executable(&sync_script);

    let auto_state = root.join("auto-ran");
    let xtask_script = root.join("fake-xtask.sh");
    write(
        &root,
        "fake-xtask.sh",
        &format!(
            r#"#!/bin/bash
set -euo pipefail
if [ ! -f {state:?} ]; then
  : > {state:?}
  mkdir -p docs/media
  printf '{{\n  "totals": {{\n    "commits": 3,\n    "other": 0\n  }}\n}}\n' > docs/media/commit-graph.json
  for name in commit-graph.svg commit-graph-narrow.svg commit-graph-compact.svg commit-graph-compact-narrow.svg; do
    printf '<svg/>' > "docs/media/$name"
  done
  echo "commit-graph --auto: changed 2 -> 3 commits (public tip deadbee, parent cafe123)"
  exit 10
fi
echo "commit-graph --auto: up to date; public tip deadbee is the refresh of cafe123"
"#,
            state = auto_state.display().to_string()
        ),
    );
    make_executable(&xtask_script);

    Fixture {
        root,
        repo,
        sync_log,
        auto_state,
        sync_script,
        xtask_script,
    }
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)
        .expect("fixture script metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("make the fixture script executable");
}

fn publish(fixture: &Fixture, sync_exit: u8) -> Output {
    Command::new("bash")
        .arg(script())
        .current_dir(&fixture.repo)
        .env("CLIMB_SYNC_SCRIPT", &fixture.sync_script)
        .env("CLIMB_XTASK", &fixture.xtask_script)
        .env("SYNC_EXIT", sync_exit.to_string())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("run publish.sh")
}

fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn a_full_cycle_commits_the_refresh_and_checks_the_count() {
    let fixture = fixture("cycle", 3);
    let output = publish(&fixture, 0);
    let text = combined(&output);
    assert!(output.status.success(), "publish succeeded: {text}");

    let calls = std::fs::read_to_string(&fixture.sync_log).expect("the sync log");
    assert_eq!(calls.lines().count(), 2, "the sync runs twice: {calls}");
    assert_eq!(
        git_ok(&fixture.repo, &["log", "-1", "--format=%s"]),
        "docs: refresh commit graph"
    );
    assert!(
        text.contains("card says:   3 commits"),
        "the card count is printed: {text}"
    );
    assert!(
        text.contains("public shows: 3 commits"),
        "the published count is printed: {text}"
    );
    let log =
        std::fs::read_to_string(fixture.repo.join("target/publish.log")).expect("the publish log");
    assert!(log.contains("==> 1/5"), "the log records every step: {log}");
    assert!(log.contains("==> 5/5"), "the log records every step: {log}");
}

#[test]
fn a_sync_gate_failure_aborts_before_the_refresh_commit() {
    let fixture = fixture("gate", 3);
    let output = publish(&fixture, 1);
    let text = combined(&output);
    assert!(!output.status.success());
    assert!(text.contains("the sync failed"), "{text}");
    assert_eq!(
        git_ok(&fixture.repo, &["log", "-1", "--format=%s"]),
        "work",
        "no refresh commit is created"
    );
    assert!(
        !fixture.auto_state.exists(),
        "the generator never runs after a failed gate"
    );
    let calls = std::fs::read_to_string(&fixture.sync_log).expect("the sync log");
    assert_eq!(calls.lines().count(), 1, "the sync runs once: {calls}");
}

#[test]
fn a_dirty_tree_is_refused_before_anything_is_pushed() {
    let fixture = fixture("dirty", 3);
    write(&fixture.repo, "dirty.txt", "uncommitted\n");
    let output = publish(&fixture, 0);
    let text = combined(&output);
    assert!(!output.status.success());
    assert!(text.contains("not clean"), "{text}");
    assert!(
        !fixture.sync_log.exists(),
        "the sync never runs on a dirty tree"
    );
}

#[test]
fn a_second_publish_is_refused_by_the_lock() {
    let fixture = fixture("lock", 3);
    let lock = fixture.repo.join("target/publish.lock");
    std::fs::create_dir_all(lock.parent().expect("lock parent")).expect("create target");
    let mut holder = Command::new("flock")
        .arg(&lock)
        .args(["sleep", "20"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the lock holder");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let probe = Command::new("flock")
            .arg("--nonblock")
            .arg(&lock)
            .arg("true")
            .status()
            .expect("probe the lock");
        if !probe.success() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the background process did not take the lock"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    let output = publish(&fixture, 0);
    let _ = holder.kill();
    let _ = holder.wait();
    let text = combined(&output);
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("already running"), "{text}");
    assert!(
        !fixture.sync_log.exists(),
        "the sync never runs under a lock"
    );
}

#[test]
fn publish_refuses_to_reenter_when_the_hook_started_it() {
    let fixture = fixture("reentry", 3);
    let output = Command::new("bash")
        .arg(script())
        .current_dir(&fixture.repo)
        .env("CLIMB_SYNC_SCRIPT", &fixture.sync_script)
        .env("CLIMB_XTASK", &fixture.xtask_script)
        .env("CLIMB_AUTOPUBLISH_RUNNING", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("run publish.sh");
    let text = combined(&output);
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("already running"), "{text}");
    assert!(
        !fixture.sync_log.exists(),
        "a re-entrant publish never reaches the sync"
    );
}
