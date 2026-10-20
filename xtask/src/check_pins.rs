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

/// Run the pin check against the real workspace.
pub(crate) fn run(root: &Path) -> Result<()> {
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
    let environment_json = std::fs::read_to_string(&env_path)
        .with_context(|| format!("failed to read {}", env_path.display()))?;

    let mismatches = compare_pins(&dockerfile, &submodules, &environment_json);
    if mismatches.is_empty() {
        return Ok(());
    }
    for mismatch in &mismatches {
        eprintln!("{mismatch}");
    }
    bail!("check-pins: {} mismatch(es)", mismatches.len())
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
}
