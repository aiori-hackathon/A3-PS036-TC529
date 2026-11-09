//! End-to-end checks of the binary's non-interactive surface: the no-TTY
//! guard, `--version` / `--help`, and `--dump-frame`.

use std::path::PathBuf;
use std::process::{Command, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_climb-tui")
}

fn sample(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/sample-output")
        .join(name)
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn without_a_tty_it_exits_with_status_two_and_says_how_to_get_a_frame() {
    let output = Command::new(bin())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the binary runs");
    assert_eq!(
        output.status.code(),
        Some(2),
        "no-TTY is a usage error, not a crash: {}",
        stderr(&output)
    );
    let error = stderr(&output);
    assert!(
        error.contains("interactive terminal"),
        "the no-TTY message must explain the requirement: {error}"
    );
    assert!(
        error.contains("--dump-frame"),
        "the no-TTY message must name the non-interactive alternative: {error}"
    );
    assert!(
        output.stdout.is_empty(),
        "a no-TTY run must not write a frame to stdout"
    );
}

#[test]
fn version_and_help_work_without_a_tty() {
    let version = Command::new(bin())
        .arg("--version")
        .output()
        .expect("--version runs");
    assert!(version.status.success(), "{}", stderr(&version));
    assert!(
        stdout(&version).contains("climb-tui"),
        "--version must name the binary: {}",
        stdout(&version)
    );

    let help = Command::new(bin())
        .arg("--help")
        .output()
        .expect("--help runs");
    assert!(help.status.success(), "{}", stderr(&help));
    let text = stdout(&help);
    for flag in [
        "--results",
        "--theme",
        "--ascii",
        "--replay",
        "--check",
        "--tour",
        "--dump-frame",
    ] {
        assert!(text.contains(flag), "--help must document {flag}:\n{text}");
    }
}

#[test]
fn dump_frame_renders_every_tab_without_a_tty() {
    let results = sample("climb-results.json");
    let results = results.to_str().expect("the sample path is UTF-8");
    for (tab, needle) in [
        ("overview", "Overview"),
        ("signing", "Signing"),
        ("verifying", "Verifying"),
        ("wire", "Wire"),
        ("memory", "Memory"),
        ("ladder", "Ladder"),
        ("live", "Live"),
        ("integrity", "Integrity"),
        ("method", "Method"),
    ] {
        let output = Command::new(bin())
            .args(["--results", results, "--dump-frame", tab, "120x40"])
            .output()
            .expect("--dump-frame runs");
        assert!(output.status.success(), "{tab}: {}", stderr(&output));
        let frame = stdout(&output);
        assert!(
            frame.lines().count() >= 24,
            "{tab} produced too few rows:\n{frame}"
        );
        assert!(
            frame.contains(needle),
            "{tab} frame does not name its tab {needle:?}:\n{frame}"
        );
    }
}

#[test]
fn dump_frame_explains_a_too_small_terminal() {
    let output = Command::new(bin())
        .args(["--dump-frame", "overview", "60x20"])
        .output()
        .expect("--dump-frame runs");
    assert!(output.status.success(), "{}", stderr(&output));
    let frame = stdout(&output);
    assert!(frame.contains("80x24"), "required size missing:\n{frame}");
    assert!(frame.contains("60x20"), "current size missing:\n{frame}");
}

#[test]
fn dump_frame_rejects_an_unknown_tab_and_a_bad_size() {
    let unknown = Command::new(bin())
        .args(["--dump-frame", "nope", "80x24"])
        .output()
        .expect("--dump-frame runs");
    assert!(!unknown.status.success());
    assert!(
        stderr(&unknown).contains("unknown tab"),
        "{}",
        stderr(&unknown)
    );

    let bad_size = Command::new(bin())
        .args(["--dump-frame", "overview", "80X24"])
        .output()
        .expect("--dump-frame runs");
    assert!(!bad_size.status.success());
    assert!(
        stderr(&bad_size).contains("WIDTHxHEIGHT"),
        "{}",
        stderr(&bad_size)
    );
}
