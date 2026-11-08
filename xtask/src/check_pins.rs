//! `cargo xtask check-pins`: keep the pinned-dependency record honest.
//!
//! The vendored `third_party/` submodules are pinned to exact commits, and two
//! places restate those pins for a human reader: the comment block at the top
//! of the toolchain `Dockerfile`, and the `environment.json` fingerprint. A pin
//! that drifts in one place while the submodule moves in another is a silent
//! provenance bug — the build would claim a commit it does not use.
//!
//! This check reads the three sources and fails when they disagree:
//!
//! 1. `git submodule status` (the checkouts actually present);
//! 2. `environment.json`'s recorded `submodules` list;
//! 3. the short-SHA comments in `docker/Dockerfile.build`.
//!
//! The comparison is on the SHA prefix the Dockerfile names, so the comment
//! stays readable while still being tied to the real commit.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::path::{Path, PathBuf};

#[derive(Args)]
pub(crate) struct CheckPinsArgs {
    /// Workspace root holding `docker/` and `environment.json`.
    #[arg(long, default_value = ".")]
    pub(crate) root: PathBuf,
}

/// One pinned-dependency source row: the submodule path, its checked-out SHA,
/// and its `git describe` string (when the checkout carries one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubmodulePin {
    pub(crate) path: String,
    pub(crate) sha: String,
    pub(crate) describe: Option<String>,
}

/// True when the Dockerfile token identifies this submodule: either a prefix of
/// its SHA, or its exact `git describe` string (used where the comment names a
/// release version rather than a commit).
fn token_matches(token: &str, submodule: &SubmodulePin) -> bool {
    submodule.sha.starts_with(token) || submodule.describe.as_deref() == Some(token)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DockerfilePin {
    /// Human label as it appears in the Dockerfile comment, e.g. `libMTL`.
    pub(crate) label: String,
    /// Submodule path under `third_party/`, e.g. `third_party/MTL`.
    pub(crate) submodule_path: String,
    /// Token written in the Dockerfile comment: a SHA prefix or a version.
    pub(crate) comment_token: String,
}

/// The pins this check enforces. Extending the list is the only edit needed
/// when a new vendored dependency is pinned.
const PINS: &[(&str, &str)] = &[
    ("libMTL", "third_party/MTL"),
    ("liboqs", "third_party/liboqs"),
];

/// Parse the `# <label>:  <token> (git submodule at <path>)` comment lines of a
/// Dockerfile. Only the configured [`PINS`] labels are returned, in order.
pub(crate) fn parse_dockerfile_pins(dockerfile: &str) -> Vec<DockerfilePin> {
    let mut out = Vec::new();
    for (label, path) in PINS {
        let prefix = format!("# {label}:");
        if let Some(line) = dockerfile
            .lines()
            .find(|l| l.trim_start().starts_with(&prefix))
        {
            let rest = line.trim_start().trim_start_matches(&prefix).trim();
            // The token is the first whitespace-delimited field.
            if let Some(token) = rest.split_whitespace().next() {
                out.push(DockerfilePin {
                    label: (*label).to_string(),
                    submodule_path: (*path).to_string(),
                    comment_token: token.to_string(),
                });
            }
        }
    }
    out
}

/// Look up the recorded SHA of `path` in an `environment.json` document,
/// returning `None` when the file carries no such submodule.
pub(crate) fn submodule_sha_from_environment(env_json: &str, path: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(env_json).ok()?;
    let submodules = value.get("submodules")?.as_array()?;
    for entry in submodules {
        if entry.get("path").and_then(|p| p.as_str()) == Some(path) {
            return entry
                .get("sha")
                .and_then(|s| s.as_str())
                .map(str::to_string);
        }
    }
    None
}

/// One disagreement between the pin sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PinMismatch {
    pub(crate) label: String,
    pub(crate) message: String,
}

impl std::fmt::Display for PinMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.label, self.message)
    }
}

/// One container image reference that is not pinned by digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImagePinViolation {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) image: String,
}

impl std::fmt::Display for ImagePinViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: container image `{}` is not pinned by digest (expected `image@sha256:<64 hex>`)",
            self.file, self.line, self.image
        )
    }
}

/// True when `image` names an immutable digest (`name@sha256:<64 hex>`).
pub(crate) fn is_digest_pinned(image: &str) -> bool {
    let Some((_, digest)) = image.split_once("@sha256:") else {
        return false;
    };
    digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit())
}

/// Scan every `FROM` line in the supplied `(file name, text)` pairs. A bare
/// tag (`debian:bookworm-slim`) can move; only a digest cannot. `scratch` is a
/// reserved empty base and needs no digest. Pure, so the rule is unit-testable.
pub(crate) fn scan_from_pins(files: &[(String, String)]) -> Vec<ImagePinViolation> {
    let mut violations = Vec::new();
    for (file, text) in files {
        for (idx, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            let upper = trimmed.to_ascii_uppercase();
            if !upper.starts_with("FROM ") {
                continue;
            }
            // Dockerfile flags such as `--platform=linux/amd64` precede the
            // image reference and are not the image.
            let image = trimmed[5..]
                .split_whitespace()
                .find(|token| !token.starts_with("--"))
                .unwrap_or("");
            if image.is_empty() || image.eq_ignore_ascii_case("scratch") {
                continue;
            }
            if !is_digest_pinned(image) {
                violations.push(ImagePinViolation {
                    file: file.clone(),
                    line: idx + 1,
                    image: image.to_string(),
                });
            }
        }
    }
    violations
}

/// One apt package installed without an explicit `=version` pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AptPinViolation {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) package: String,
}

impl std::fmt::Display for AptPinViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: apt package `{}` is not version-pinned (expected `package=version`)",
            self.file, self.line, self.package
        )
    }
}

/// Join Dockerfile line continuations, keeping the physical line each logical
/// line starts on.
fn logical_lines(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut start = 1;
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim_end();
        if current.is_empty() {
            start = idx + 1;
        }
        if let Some(prefix) = trimmed.strip_suffix('\\') {
            current.push_str(prefix);
            current.push(' ');
        } else {
            current.push_str(trimmed);
            out.push((start, std::mem::take(&mut current)));
        }
    }
    if !current.is_empty() {
        out.push((start, current));
    }
    out
}

/// Scan `apt-get install` invocations and require `package=version` for every
/// direct package. Option flags are skipped. Only the toolchain Dockerfile is
/// passed here; the live-pipeline images carry a documented, unpinned set.
pub(crate) fn scan_apt_pins(file: &str, text: &str) -> Vec<AptPinViolation> {
    let mut violations = Vec::new();
    for (line, logical) in logical_lines(text) {
        let Some(after) = logical.split_once("apt-get install").map(|(_, rest)| rest) else {
            continue;
        };
        // Stop at the next shell command on the same logical line.
        let packages = after.split("&&").next().unwrap_or(after);
        for token in packages.split_whitespace() {
            if token.starts_with('-') {
                continue;
            }
            let pinned = token
                .split_once('=')
                .is_some_and(|(name, version)| !name.is_empty() && !version.is_empty());
            if !pinned {
                violations.push(AptPinViolation {
                    file: file.to_string(),
                    line,
                    package: token.to_string(),
                });
            }
        }
    }
    violations
}

/// Compare the Dockerfile comment tokens against the git checkouts and the
/// recorded environment fingerprint. Pure: takes the three inputs as text /
/// structs, so it is unit-testable without git or the filesystem.
pub(crate) fn compare_pins(
    dockerfile: &str,
    git_submodules: &[SubmodulePin],
    environment_json: &str,
) -> Vec<PinMismatch> {
    let mut mismatches = Vec::new();
    for pin in parse_dockerfile_pins(dockerfile) {
        let git_pin = git_submodules.iter().find(|s| s.path == pin.submodule_path);
        let env_sha = submodule_sha_from_environment(environment_json, &pin.submodule_path);

        match git_pin {
            Some(submodule) if token_matches(&pin.comment_token, submodule) => {}
            Some(submodule) => mismatches.push(PinMismatch {
                label: pin.label.clone(),
                message: format!(
                    "the toolchain Dockerfile comment pins `{}` but the submodule at {} is {}",
                    pin.comment_token, pin.submodule_path, submodule.sha
                ),
            }),
            None => mismatches.push(PinMismatch {
                label: pin.label.clone(),
                message: format!(
                    "no submodule checkout found for {} (expected in `git submodule status`)",
                    pin.submodule_path
                ),
            }),
        }

        if let Some(sha) = env_sha {
            let env_pin = SubmodulePin {
                path: pin.submodule_path.clone(),
                sha,
                describe: git_pin.and_then(|s| s.describe.clone()),
            };
            if !token_matches(&pin.comment_token, &env_pin) {
                mismatches.push(PinMismatch {
                    label: pin.label.clone(),
                    message: format!(
                        "the toolchain Dockerfile comment pins `{}` but environment.json records {} for {}",
                        pin.comment_token, env_pin.sha, pin.submodule_path
                    ),
                });
            }
        }
    }
    mismatches
}

/// Read every `docker/Dockerfile*` as `(relative name, text)`, sorted by name.
fn dockerfiles(root: &Path) -> Result<Vec<(String, String)>> {
    let dir = root.join("docker");
    let mut files = Vec::new();
    let entries =
        std::fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("failed to read {}", dir.display()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("Dockerfile") {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        files.push((format!("docker/{name}"), text));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

/// Run the pin check against the real workspace.
pub(crate) fn run(root: &Path) -> Result<()> {
    let files = dockerfiles(root)?;

    let mut failures: Vec<String> = Vec::new();

    // Every base image in every Dockerfile must be digest-pinned.
    for violation in scan_from_pins(&files) {
        failures.push(violation.to_string());
    }

    // The toolchain image installs build tools directly from apt; those
    // direct packages must carry an explicit version.
    if let Some((_, build)) = files
        .iter()
        .find(|(name, _)| name == "docker/Dockerfile.build")
    {
        for violation in scan_apt_pins("docker/Dockerfile.build", build) {
            failures.push(violation.to_string());
        }
    }

    // The vendored submodules must agree with their restatement in the
    // toolchain Dockerfile comment and the environment fingerprint.
    let dockerfile_path = root.join("docker").join("Dockerfile.build");
    let dockerfile = std::fs::read_to_string(&dockerfile_path)
        .with_context(|| format!("failed to read {}", dockerfile_path.display()))?;

    let submodules: Vec<SubmodulePin> = crate::pin_env::capture_submodules(root)
        .into_iter()
        .map(|s| SubmodulePin {
            path: s.path,
            sha: s.sha,
            describe: s.describe,
        })
        .collect();

    let env_path = root.join("environment.json");
    let environment_json = match std::fs::read_to_string(&env_path) {
        Ok(text) => text,
        // The fingerprint is generated (and gitignored), so a clean checkout
        // legitimately lacks it. The Dockerfile-vs-submodule comparison still
        // runs; only the third source is unavailable.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            println!(
                "check-pins: {} absent; skipping the environment-fingerprint comparison",
                env_path.display()
            );
            "{}".to_string()
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", env_path.display()))
        }
    };

    for mismatch in compare_pins(&dockerfile, &submodules, &environment_json) {
        failures.push(mismatch.to_string());
    }

    if failures.is_empty() {
        return Ok(());
    }
    for failure in &failures {
        eprintln!("{failure}");
    }
    bail!("check-pins: {} mismatch(es)", failures.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCKERFILE: &str = "\
# Pinned dependencies
# liboqs:      0.16.0  (git submodule at third_party/liboqs)
# libMTL:      53ce25d (git submodule at third_party/MTL)
";

    const ENVIRONMENT: &str = r#"{
  "submodules": [
    {"path": "third_party/MTL", "sha": "53ce25dcc35c15e051c5263e41aaeaede18a8aa7"},
    {"path": "third_party/liboqs", "sha": "5a1a854b0dc9f2141bdc771c555ee60c37950183"}
  ]
}"#;

    fn git_pins() -> Vec<SubmodulePin> {
        vec![
            SubmodulePin {
                path: "third_party/MTL".to_string(),
                sha: "53ce25dcc35c15e051c5263e41aaeaede18a8aa7".to_string(),
                describe: Some("MTLLIB_1_2_1-3-g53ce25d".to_string()),
            },
            SubmodulePin {
                path: "third_party/liboqs".to_string(),
                sha: "5a1a854b0dc9f2141bdc771c555ee60c37950183".to_string(),
                describe: Some("0.16.0".to_string()),
            },
        ]
    }

    #[test]
    fn parses_the_configured_pins_in_order() {
        let pins = parse_dockerfile_pins(DOCKERFILE);
        assert_eq!(pins.len(), 2);
        assert_eq!(pins[0].label, "libMTL");
        assert_eq!(pins[1].label, "liboqs");
        assert_eq!(pins[0].comment_token, "53ce25d");
        assert_eq!(pins[0].submodule_path, "third_party/MTL");
    }

    #[test]
    fn agreeing_sources_produce_no_mismatch() {
        let mismatches = compare_pins(DOCKERFILE, &git_pins(), ENVIRONMENT);
        assert!(mismatches.is_empty(), "{mismatches:?}");
    }

    #[test]
    fn a_version_comment_matches_the_submodule_describe() {
        // `liboqs: 0.16.0` names a release, not a commit; it is consistent with
        // the submodule whose `git describe` is exactly `0.16.0`.
        let liboqs = parse_dockerfile_pins(DOCKERFILE)
            .into_iter()
            .find(|p| p.label == "liboqs")
            .expect("liboqs pin parsed");
        assert_eq!(liboqs.comment_token, "0.16.0");
        assert!(compare_pins(DOCKERFILE, &git_pins(), ENVIRONMENT).is_empty());
    }

    #[test]
    fn a_stale_dockerfile_pin_is_reported() {
        let stale = DOCKERFILE.replace("53ce25d", "566c518");
        let mismatches = compare_pins(&stale, &git_pins(), ENVIRONMENT);
        // A stale token disagrees with both the checkout and the fingerprint.
        assert_eq!(mismatches.len(), 2, "{mismatches:?}");
        assert!(mismatches.iter().all(|m| m.label == "libMTL"));
        assert!(mismatches[0].message.contains("566c518"), "{mismatches:?}");
        assert!(
            mismatches[0].message.contains("third_party/MTL"),
            "{mismatches:?}"
        );
    }

    #[test]
    fn a_missing_submodule_checkout_is_reported() {
        let only_liboqs = vec![git_pins()[1].clone()];
        let mismatches = compare_pins(DOCKERFILE, &only_liboqs, ENVIRONMENT);
        assert_eq!(mismatches, vec![PinMismatch {
            label: "libMTL".to_string(),
            message: "no submodule checkout found for third_party/MTL (expected in `git submodule status`)".to_string(),
        }]);
    }

    #[test]
    fn an_environment_fingerprint_that_disagrees_is_reported() {
        let env = ENVIRONMENT.replace(
            "53ce25dcc35c15e051c5263e41aaeaede18a8aa7",
            "0000000000000000000000000000000000000000",
        );
        let mismatches = compare_pins(DOCKERFILE, &git_pins(), &env);
        assert_eq!(mismatches.len(), 1, "{mismatches:?}");
        assert!(
            mismatches[0].message.contains("environment.json"),
            "{mismatches:?}"
        );
    }

    #[test]
    fn environment_lookup_returns_none_for_an_absent_path() {
        assert!(submodule_sha_from_environment(ENVIRONMENT, "third_party/nope").is_none());
        assert_eq!(
            submodule_sha_from_environment(ENVIRONMENT, "third_party/MTL").as_deref(),
            Some("53ce25dcc35c15e051c5263e41aaeaede18a8aa7")
        );
    }

    #[test]
    fn the_shipped_pins_agree() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        run(root).expect("the committed pins agree");
    }

    #[test]
    fn a_bare_tag_from_is_a_violation() {
        let files = vec![(
            "docker/Dockerfile.tui".to_string(),
            "FROM alpine:3.22\n".to_string(),
        )];
        let violations = scan_from_pins(&files);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].line, 1);
        assert_eq!(violations[0].image, "alpine:3.22");
    }

    #[test]
    fn a_digest_pinned_from_passes() {
        let files = vec![(
            "docker/Dockerfile.tui".to_string(),
            "FROM alpine:3.22@sha256:bfe7ceb9f09ec0566c48fb93356e45a47a00be5a1dd14fa5b909e7709c66b034\n"
                .to_string(),
        )];
        assert!(scan_from_pins(&files).is_empty());
    }

    #[test]
    fn scratch_and_platform_flags_are_ignored() {
        let files = vec![(
            "docker/Dockerfile.build".to_string(),
            "FROM --platform=linux/amd64 scratch AS final\n".to_string(),
        )];
        assert!(scan_from_pins(&files).is_empty());
    }

    #[test]
    fn a_digest_must_be_64_hex_characters() {
        assert!(is_digest_pinned(&format!(
            "alpine@sha256:{}",
            "a".repeat(64)
        )));
        assert!(!is_digest_pinned(&format!(
            "alpine@sha256:{}",
            "a".repeat(63)
        )));
        assert!(!is_digest_pinned(&format!(
            "alpine@sha256:{}z",
            "a".repeat(63)
        )));
    }

    #[test]
    fn every_shipped_dockerfile_from_is_digest_pinned() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let files = dockerfiles(root).expect("docker directory is readable");
        assert!(!files.is_empty(), "no Dockerfiles found");
        let violations = scan_from_pins(&files);
        assert!(violations.is_empty(), "{violations:#?}");
    }

    #[test]
    fn an_unpinned_apt_package_is_a_violation() {
        let text = "RUN apt-get update && apt-get install -y --no-install-recommends \\\n    autoconf \\\n    cmake=3.25.1-1 \\\n    && rm -rf /var/lib/apt/lists/*\n";
        let violations = scan_apt_pins("docker/Dockerfile.build", text);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].package, "autoconf");
        assert_eq!(violations[0].line, 1);
    }

    #[test]
    fn version_pinned_apt_packages_pass() {
        let text = "RUN apt-get update && apt-get install -y --no-install-recommends \\\n    autoconf=2.71-3 \\\n    automake=1:1.16.5-1.3 \\\n    && rm -rf /var/lib/apt/lists/*\n";
        assert!(scan_apt_pins("docker/Dockerfile.build", text).is_empty());
    }

    #[test]
    fn the_toolchain_dockerfile_pins_every_apt_package() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let text = std::fs::read_to_string(root.join("docker/Dockerfile.build"))
            .expect("toolchain Dockerfile is committed");
        let violations = scan_apt_pins("docker/Dockerfile.build", &text);
        assert!(violations.is_empty(), "{violations:#?}");
    }

    #[test]
    fn the_toolchain_dockerfile_verifies_the_openssl_archive() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let text = std::fs::read_to_string(root.join("docker/Dockerfile.build"))
            .expect("toolchain Dockerfile is committed");
        assert!(text.contains("ARG OPENSSL_SHA256="), "{text}");
        assert!(text.contains("sha256sum -c -"), "{text}");
    }

    /// Every image that downloads the OpenSSL release archive must verify it
    /// against the recorded digest, not merely name the version.
    #[test]
    fn every_dockerfile_that_downloads_openssl_verifies_the_archive() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let docker_dir = root.join("docker");
        let mut checked = 0;
        for entry in std::fs::read_dir(&docker_dir).expect("docker/ exists") {
            let path = entry.expect("readable entry").path();
            if !path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("Dockerfile"))
            {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("Dockerfile is readable");
            if !text.contains("openssl-${OPENSSL_VERSION}.tar.gz") {
                continue;
            }
            checked += 1;
            let label = path.display();
            assert!(
                text.contains("ARG OPENSSL_SHA256="),
                "{label} has no digest"
            );
            assert!(
                text.contains("sha256sum -c -"),
                "{label} downloads OpenSSL without verifying it"
            );
        }
        assert!(
            checked >= 3,
            "expected every OpenSSL image, found {checked}"
        );
    }
}
