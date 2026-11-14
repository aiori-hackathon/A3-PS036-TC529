//! Build-configuration guard for the viewer job.
//!
//! The viewer job tests `climb-tui` with `--no-default-features`, where the
//! `terminal` feature (and therefore the `climb-tui` binary) is absent. Any
//! integration test that executes that binary must declare the feature as a
//! requirement, otherwise Cargo still schedules the target and every test in
//! it fails with "No such file or directory" for the missing executable.

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives under the workspace root")
        .to_path_buf()
}

#[test]
fn the_terminal_binary_tests_require_the_terminal_feature() {
    let path = workspace_root().join("crates/climb-tui/Cargo.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let manifest: toml::Value = toml::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is not valid TOML: {error}", path.display()));
    let targets = manifest
        .get("test")
        .and_then(toml::Value::as_array)
        .expect("crates/climb-tui/Cargo.toml declares its terminal test targets");

    for name in ["cli", "pty"] {
        let target = targets
            .iter()
            .find(|target| target.get("name").and_then(toml::Value::as_str) == Some(name))
            .unwrap_or_else(|| panic!("the `{name}` test target is not declared"));
        let features = target
            .get("required-features")
            .and_then(toml::Value::as_array)
            .unwrap_or_else(|| panic!("the `{name}` test target has no required-features"));
        assert!(
            features
                .iter()
                .any(|feature| feature.as_str() == Some("terminal")),
            "the `{name}` test target runs the terminal binary and must require `terminal`"
        );
    }
}
