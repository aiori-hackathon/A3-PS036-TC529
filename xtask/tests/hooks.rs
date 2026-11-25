//! Tests for `scripts/install-hooks.sh`: what it installs, the recursion guard
//! in the autopublish hook, and the scope of `--remove`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn installer() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace")
        .join("scripts/install-hooks.sh")
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Test Harness")
        .env("GIT_AUTHOR_EMAIL", "harness@example.invalid")
        .env("GIT_COMMITTER_NAME", "Test Harness")
        .env("GIT_COMMITTER_EMAIL", "harness@example.invalid")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn install(repo: &Path, args: &[&str]) -> Output {
    Command::new("bash")
        .arg(installer())
        .args(args)
        .current_dir(repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("run install-hooks.sh")
}

fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn new(label: &str) -> Self {
        let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "climb-hooks-{}-{serial}-{label}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create the repository directory");
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["config", "user.name", "Test Harness"]);
        git(&root, &["config", "user.email", "harness@example.invalid"]);
        std::fs::write(root.join("README.md"), "# fixture\n").expect("write a tracked file");
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-q", "-m", "fixture"]);
        Self { root }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn hook(repo: &Path, name: &str) -> PathBuf {
    repo.join(".git/hooks").join(name)
}

#[test]
fn the_pre_push_hook_treats_ten_as_a_failure_not_a_staged_refresh() {
    let repo = TempRepo::new("pre-push-exit");
    let installed = install(&repo.root, &[]);
    assert!(installed.status.success(), "{}", combined(&installed));

    let bin = repo.root.join("fake-bin");
    std::fs::create_dir_all(&bin).expect("create the fake bin directory");
    let calls = repo.root.join("generator-calls.log");
    let fake_cargo = bin.join("cargo");
    std::fs::write(
        &fake_cargo,
        format!(
            "#!/bin/bash\nprintf '%s\\n' \"$*\" >> {log}\nexit \"${{FAKE_EXIT:-0}}\"\n",
            log = calls.display()
        ),
    )
    .expect("write the fake cargo");
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(&fake_cargo)
        .expect("metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_cargo, permissions).expect("make the fake cargo executable");

    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string())
    );
    for (status, success) in [(0, true), (1, false), (10, false)] {
        let output = Command::new("bash")
            .arg(hook(&repo.root, "pre-push"))
            .current_dir(&repo.root)
            .env("PATH", &path)
            .env("FAKE_EXIT", status.to_string())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .expect("run the installed hook");
        let text = combined(&output);
        assert_eq!(
            output.status.success(),
            success,
            "exit {status} must be treated as {}: {text}",
            if success { "success" } else { "failure" }
        );
        if !success {
            assert!(
                text.contains(&format!("commit-graph check failed (exit {status})")),
                "the hook must report the generator's status: {text}"
            );
            assert!(
                text.contains("scripts/publish.sh"),
                "the hook must name the command that fixes it: {text}"
            );
        }
    }
    let recorded = std::fs::read_to_string(&calls).expect("the fake generator recorded its calls");
    assert!(
        recorded
            .lines()
            .all(|line| line.contains("commit-graph --check")),
        "the pre-push hook only ever runs the read-only check: {recorded}"
    );
}

#[test]
fn autopublish_installs_a_guarded_hook_and_remove_is_scoped() {
    let repo = TempRepo::new("autopublish");
    let foreign = hook(&repo.root, "commit-msg");
    std::fs::write(&foreign, "#!/bin/bash\nexit 0\n").expect("write a foreign hook");

    let output = install(&repo.root, &["--autopublish"]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");

    let pre_push = std::fs::read_to_string(hook(&repo.root, "pre-push"))
        .expect("the pre-push hook is installed");
    assert!(pre_push.contains("Installed by scripts/install-hooks.sh."));
    assert!(pre_push.contains("commit-graph --check"));
    let post_commit = std::fs::read_to_string(hook(&repo.root, "post-commit"))
        .expect("the post-commit hook is installed");
    assert!(post_commit.contains("Installed by scripts/install-hooks.sh."));
    assert!(post_commit.contains("CLIMB_AUTOPUBLISH_RUNNING"));
    assert!(post_commit.contains("scripts/publish.sh"));
    assert!(text.contains("post-commit autopublish"), "{text}");

    // The guard stops the hook before it touches publish.sh. The guard is the
    // repository root, so the hook compares against the repository it is in.
    let guarded = Command::new("bash")
        .arg(hook(&repo.root, "post-commit"))
        .current_dir(&repo.root)
        .env("CLIMB_AUTOPUBLISH_RUNNING", &repo.root)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("run the installed hook");
    assert!(guarded.status.success());
    assert!(
        !repo.root.join("target/autopublish.log").exists(),
        "a guarded invocation must not publish or log"
    );

    let removed = install(&repo.root, &["--remove"]);
    let removed_text = combined(&removed);
    assert!(removed.status.success(), "{removed_text}");
    assert!(
        !hook(&repo.root, "pre-push").exists(),
        "our pre-push hook is removed"
    );
    assert!(!hook(&repo.root, "post-commit").exists());
    assert!(foreign.exists(), "a hook this script did not install stays");

    // A pre-push hook someone else wrote is reported and left alone.
    let foreign_pre_push = hook(&repo.root, "pre-push");
    std::fs::write(&foreign_pre_push, "#!/bin/bash\necho mine\n").expect("write a foreign hook");
    let scoped = install(&repo.root, &["--remove"]);
    assert!(scoped.status.success());
    assert!(foreign_pre_push.exists());
    assert!(combined(&scoped).contains("left untouched"));

    let unknown = install(&repo.root, &["--wat"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(combined(&unknown).contains("unknown argument"));
}
