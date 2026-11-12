//! Test-only access to the committed artifact fixtures.
//!
//! The Integrity data and UI tests must read the committed sample under
//! `docs/sample-output/`, never a stale generated file under `target/` and
//! never a path that happens to resolve from the current directory. Every
//! fixture access goes through [`fixture`], which rejects a `target/`
//! component outright, so a test that accidentally points at the build tree
//! fails with the offending path instead of silently reading another run.
//!
//! `CLIMB_ARTIFACT_DIR` selects an explicit fixture directory. That is for a
//! deliberate copy of the committed sample (for example a temp directory that
//! a test is about to mutate); the default is the committed directory.

use std::path::{Component, Path, PathBuf};

/// The committed sample directory relative to the workspace root.
pub(crate) const DEFAULT_ARTIFACT_DIR: &str = "docs/sample-output";

/// The workspace root, from this crate's manifest directory.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

/// Resolve the fixture directory from an optional `CLIMB_ARTIFACT_DIR` value.
///
/// A relative value is resolved against the workspace root, so a test can set
/// `CLIMB_ARTIFACT_DIR=target/fixtures` and still be rejected by
/// [`guard_committed`] rather than silently reading the build tree.
pub(crate) fn resolve_artifact_dir(env_value: Option<&str>) -> PathBuf {
    match env_value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                workspace_root().join(path)
            }
        }
        None => workspace_root().join(DEFAULT_ARTIFACT_DIR),
    }
}

/// The fixture directory for this test process.
pub(crate) fn artifact_dir() -> PathBuf {
    let value = std::env::var("CLIMB_ARTIFACT_DIR").ok();
    resolve_artifact_dir(value.as_deref())
}

/// Reject a fixture path that points into the build tree.
///
/// Returns the path unchanged on success so call sites read naturally:
/// `fixtures::guard_committed(path)`.
pub(crate) fn guard_committed(path: &Path) -> &Path {
    let under_target = path.components().any(|component| match component {
        Component::Normal(name) => name.eq_ignore_ascii_case("target"),
        _ => false,
    });
    assert!(
        !under_target,
        "refusing to read a test fixture under target/: {} — use the committed sample under \
         docs/sample-output/ or set CLIMB_ARTIFACT_DIR to an explicit copy",
        path.display()
    );
    path
}

/// Resolve one committed fixture by file name.
pub(crate) fn fixture(name: &str) -> PathBuf {
    let path = artifact_dir().join(name);
    guard_committed(&path).to_path_buf()
}

/// Copy one committed fixture into `destination`, never through `target/`.
pub(crate) fn copy_fixture(name: &str, destination: &Path) -> PathBuf {
    let source = fixture(name);
    std::fs::copy(&source, destination).unwrap_or_else(|error| {
        panic!(
            "failed to copy fixture {} to {}: {error}",
            source.display(),
            destination.display()
        )
    });
    destination.to_path_buf()
}

/// A unique temporary directory for one test.
pub(crate) fn temp_root(prefix: &str, tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_artifact_dir_is_the_committed_sample() {
        let dir = resolve_artifact_dir(None);
        assert!(
            dir.ends_with(DEFAULT_ARTIFACT_DIR),
            "{} must end with {DEFAULT_ARTIFACT_DIR}",
            dir.display()
        );
        assert!(dir.join("climb-results.json").is_file());
    }

    #[test]
    fn an_explicit_artifact_dir_is_honoured() {
        let dir = resolve_artifact_dir(Some("/tmp/climb-explicit-fixtures"));
        assert_eq!(dir, PathBuf::from("/tmp/climb-explicit-fixtures"));
        let relative = resolve_artifact_dir(Some("docs/sample-output"));
        assert!(relative.ends_with(DEFAULT_ARTIFACT_DIR));
        assert!(relative.is_absolute());
    }

    #[test]
    fn a_blank_artifact_dir_falls_back_to_the_committed_sample() {
        let dir = resolve_artifact_dir(Some("   "));
        assert!(dir.ends_with(DEFAULT_ARTIFACT_DIR));
    }

    #[test]
    #[should_panic(expected = "refusing to read a test fixture under target/")]
    fn a_path_under_target_is_rejected() {
        guard_committed(Path::new("/work/target/climb-results.json"));
    }

    #[test]
    #[should_panic(expected = "refusing to read a test fixture under target/")]
    fn a_relative_path_under_target_is_rejected() {
        guard_committed(Path::new("target/climb-results.json"));
    }

    #[test]
    fn a_committed_fixture_resolves_through_the_guard() {
        let path = fixture("climb-results.json");
        assert!(path.is_file(), "{} must exist", path.display());
        assert!(!path.components().any(|c| c.as_os_str() == "target"));
    }
}
