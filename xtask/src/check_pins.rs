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
//! stays readable while still being tied to the real commit. The same command
//! also enforces that every `FROM` is digest-pinned and that every local
//! `COPY`/`ADD` source survives the Dockerfile's effective ignore file: an
//! ignored source makes the build fail before the first `RUN`, which is a
//! defect in the repository rather than in the registry or the runner.

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
/// reserved empty base and needs no digest, and a `FROM` that names an earlier
/// stage in the same file reuses that stage rather than pulling an image.
/// Pure, so the rule is unit-testable.
pub(crate) fn scan_from_pins(files: &[(String, String)]) -> Vec<ImagePinViolation> {
    let mut violations = Vec::new();
    for (file, text) in files {
        let mut stages: Vec<String> = Vec::new();
        for (idx, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            let upper = trimmed.to_ascii_uppercase();
            if !upper.starts_with("FROM ") {
                continue;
            }
            // Dockerfile flags such as `--platform=linux/amd64` precede the
            // image reference and are not the image.
            let mut tokens = trimmed[5..]
                .split_whitespace()
                .filter(|token| !token.starts_with("--"));
            let image = tokens.next().unwrap_or("");
            let alias = match (tokens.next(), tokens.next()) {
                (Some(keyword), Some(name)) if keyword.eq_ignore_ascii_case("AS") => Some(name),
                _ => None,
            };
            let reuses_stage = stages.iter().any(|stage| stage.eq_ignore_ascii_case(image));
            if !image.is_empty()
                && !image.eq_ignore_ascii_case("scratch")
                && !reuses_stage
                && !is_digest_pinned(image)
            {
                violations.push(ImagePinViolation {
                    file: file.clone(),
                    line: idx + 1,
                    image: image.to_string(),
                });
            }
            if let Some(alias) = alias {
                stages.push(alias.to_string());
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
    compare_pins_with_checkout(dockerfile, Some(git_submodules), environment_json)
}

/// Compare the Dockerfile comment tokens against the recorded fingerprint only.
///
/// A source tarball has no `.git` directory, so the submodule checkouts cannot
/// be read. The recorded pins are still compared; the missing checkouts are a
/// printed skip, not a mismatch.
pub(crate) fn compare_pins_without_checkout(
    dockerfile: &str,
    environment_json: &str,
) -> Vec<PinMismatch> {
    compare_pins_with_checkout(dockerfile, None, environment_json)
}

fn compare_pins_with_checkout(
    dockerfile: &str,
    git_submodules: Option<&[SubmodulePin]>,
    environment_json: &str,
) -> Vec<PinMismatch> {
    let mut mismatches = Vec::new();
    for pin in parse_dockerfile_pins(dockerfile) {
        let git_pin = git_submodules.and_then(|submodules| {
            submodules
                .iter()
                .find(|submodule| submodule.path == pin.submodule_path)
        });
        let env_sha = submodule_sha_from_environment(environment_json, &pin.submodule_path);

        if git_submodules.is_some() {
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

/// One local `COPY`/`ADD` source that the effective ignore file removes from
/// the build context, so the instruction fails at build time with
/// `"/<source>": not found`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BuildContextViolation {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) source: String,
    pub(crate) ignore_file: String,
}

impl std::fmt::Display for BuildContextViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: COPY/ADD source `{}` is excluded from the build context by {}",
            self.file, self.line, self.source, self.ignore_file
        )
    }
}

/// One parsed ignore rule: a path pattern and whether it re-includes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct IgnoreRule {
    negated: bool,
    segments: Vec<String>,
}

/// Parse an ignore file into rules. Blank lines and `#` comments are skipped,
/// a leading `/` anchors nothing extra (every pattern is context-relative) and
/// a trailing `/` only names a directory.
fn parse_ignore(text: &str) -> Vec<IgnoreRule> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let (negated, pattern) = match trimmed.strip_prefix('!') {
                Some(rest) => (true, rest.trim()),
                None => (false, trimmed),
            };
            let pattern = pattern.trim_start_matches('/').trim_end_matches('/');
            if pattern.is_empty() {
                return None;
            }
            Some(IgnoreRule {
                negated,
                segments: pattern.split('/').map(str::to_string).collect(),
            })
        })
        .collect()
}

/// `*` matches any run of characters inside one path segment, `?` matches one
/// character, everything else is literal.
fn segment_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0usize, 0usize);
    let (mut star, mut resume) = (None, 0usize);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            resume = t;
            p += 1;
        } else if let Some(star_index) = star {
            p = star_index + 1;
            resume += 1;
            t = resume;
        } else {
            return false;
        }
    }
    p == pattern.len() || pattern[p..].iter().all(|c| *c == '*')
}

/// Match a pattern's segments against a path's segments. `**` consumes zero or
/// more whole segments; every other segment uses [`segment_matches`].
fn segments_match(pattern: &[String], path: &[String]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((head, rest)) if head == "**" => {
            (0..=path.len()).any(|skip| segments_match(rest, &path[skip..]))
        }
        Some((head, rest)) => match path.split_first() {
            Some((first, tail)) => segment_matches(head, first) && segments_match(rest, tail),
            None => false,
        },
    }
}

/// True when the rules exclude `path`. A rule matches the path itself or any
/// parent directory (excluding a directory excludes its contents), and the
/// last matching rule wins, so a later `!` re-includes.
fn is_ignored(rules: &[IgnoreRule], path: &str) -> bool {
    let parts: Vec<String> = path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .map(str::to_string)
        .collect();
    let mut ignored = false;
    for rule in rules {
        let matches = (1..=parts.len()).any(|end| segments_match(&rule.segments, &parts[..end]));
        if matches {
            ignored = !rule.negated;
        }
    }
    ignored
}

/// True when a `COPY`/`ADD` source is read from the local build context. A URL,
/// an absolute path and an earlier stage (`--from=`) are not.
fn reads_local_context(source: &str) -> bool {
    !source.is_empty()
        && !source.starts_with('/')
        && !source.contains("://")
        && !source.starts_with("git@")
}

/// Every `(line, source)`: local `COPY`/`ADD` source arguments in a Dockerfile.
/// Line continuations are joined; `--from` copies are skipped; the JSON array
/// form is parsed. The final argument is the destination.
fn copy_sources(text: &str) -> Vec<(usize, Vec<String>)> {
    let mut out = Vec::new();
    for (line, logical) in logical_lines(text) {
        let trimmed = logical.trim_start();
        let Some(instruction) = trimmed.split_whitespace().next() else {
            continue;
        };
        if !instruction.eq_ignore_ascii_case("COPY") && !instruction.eq_ignore_ascii_case("ADD") {
            continue;
        }
        let mut rest = trimmed[instruction.len()..].trim_start();
        let mut from_stage = false;
        while let Some(flag) = rest.split_whitespace().next() {
            if !flag.starts_with("--") {
                break;
            }
            if flag.starts_with("--from") {
                from_stage = true;
            }
            rest = rest[flag.len()..].trim_start();
        }
        if from_stage {
            continue;
        }
        let mut arguments: Vec<String> = if rest.starts_with('[') {
            match serde_json::from_str::<Vec<String>>(rest) {
                Ok(values) => values,
                Err(_) => continue,
            }
        } else {
            rest.split_whitespace().map(str::to_string).collect()
        };
        // The last argument is the destination, never a context read.
        arguments.pop();
        let sources = arguments
            .into_iter()
            .filter(|source| reads_local_context(source))
            .collect::<Vec<_>>();
        if !sources.is_empty() {
            out.push((line, sources));
        }
    }
    out
}

/// Effective ignore file for a Dockerfile: `docker/<Dockerfile>.dockerignore`
/// when present (BuildKit honours it instead of the context-wide file), else
/// the repository `.dockerignore`.
fn effective_ignore(root: &Path, dockerfile: &str) -> Result<(String, String)> {
    let name = dockerfile.rsplit('/').next().unwrap_or(dockerfile);
    let specific = root.join("docker").join(format!("{name}.dockerignore"));
    match std::fs::read_to_string(&specific) {
        Ok(text) => Ok((format!("docker/{name}.dockerignore"), text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let context = root.join(".dockerignore");
            let text = match std::fs::read_to_string(&context) {
                Ok(text) => text,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("failed to read {}", context.display()))
                }
            };
            Ok((".dockerignore".to_string(), text))
        }
        Err(error) => Err(error).with_context(|| format!("failed to read {}", specific.display())),
    }
}

/// Every local `COPY`/`ADD` source that its Dockerfile's effective ignore file
/// removes from the build context. The build then fails before any `RUN` with
/// `"/<source>": not found`, which is a defect in the checked-in files, not a
/// registry or runner problem.
pub(crate) fn scan_build_context(root: &Path) -> Result<Vec<BuildContextViolation>> {
    let mut violations = Vec::new();
    for (file, text) in dockerfiles(root)? {
        let (ignore_file, ignore_text) = effective_ignore(root, &file)?;
        let rules = parse_ignore(&ignore_text);
        for (line, sources) in copy_sources(&text) {
            for source in sources {
                if is_ignored(&rules, &source) {
                    violations.push(BuildContextViolation {
                        file: file.clone(),
                        line,
                        source,
                        ignore_file: ignore_file.clone(),
                    });
                }
            }
        }
    }
    Ok(violations)
}

/// One `FROM` stage of the toolchain Dockerfile and the destinations its
/// `COPY`/`ADD` instructions write.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stage {
    name: Option<String>,
    base: String,
    copies: Vec<String>,
}

/// Split a Dockerfile into stages, keeping the destinations each stage writes.
fn dockerfile_stages(text: &str) -> Vec<Stage> {
    let mut stages: Vec<Stage> = Vec::new();
    for (_, logical) in logical_lines(text) {
        let trimmed = logical.trim_start();
        let Some(keyword) = trimmed.split_whitespace().next() else {
            continue;
        };
        let keyword_upper = keyword.to_ascii_uppercase();
        let rest = trimmed[keyword.len()..].trim_start();
        if keyword_upper == "FROM" {
            let mut tokens = rest.split_whitespace().filter(|t| !t.starts_with("--"));
            let Some(base) = tokens.next() else {
                continue;
            };
            let name = match (tokens.next(), tokens.next()) {
                (Some(kw), Some(name)) if kw.eq_ignore_ascii_case("AS") => Some(name.to_string()),
                _ => None,
            };
            stages.push(Stage {
                name,
                base: base.to_string(),
                copies: Vec::new(),
            });
            continue;
        }
        if keyword_upper != "COPY" && keyword_upper != "ADD" {
            continue;
        }
        let mut rest = rest;
        while let Some(flag) = rest.split_whitespace().next() {
            if !flag.starts_with("--") {
                break;
            }
            rest = rest[flag.len()..].trim_start();
        }
        let destination = if rest.starts_with('[') {
            serde_json::from_str::<Vec<String>>(rest)
                .ok()
                .and_then(|values| values.into_iter().last())
        } else {
            rest.split_whitespace().last().map(str::to_string)
        };
        if let (Some(stage), Some(destination)) = (stages.last_mut(), destination) {
            stage.copies.push(destination);
        }
    }
    stages
}

/// Paths that `docker/extract-libs.sh` copies out of the built image.
fn extraction_paths(script: &str) -> Vec<String> {
    script
        .lines()
        .filter_map(|line| {
            let (_, rest) = line.split_once("docker cp \"${container_id}:")?;
            let (path, _) = rest.split_once('"')?;
            Some(path.to_string())
        })
        .collect()
}

/// True when `docker create` is given a command after the image reference. A
/// scratch image needs a placeholder; the container is never started.
fn create_has_command(script: &str) -> bool {
    script
        .lines()
        .filter(|line| line.contains("docker create"))
        .any(|line| {
            let Some((_, rest)) = line.split_once("\"${IMAGE}\"") else {
                return false;
            };
            let trailing = rest.trim_end_matches([')', '"', '\'', ';', ' ']);
            !trailing.trim().is_empty()
        })
}

/// True when the default (last) stage's filesystem is the builder stage's,
/// directly or through a chain of stage aliases.
fn default_stage_is_builder(stages: &[Stage]) -> bool {
    let Some(last) = stages.last() else {
        return false;
    };
    let mut base = last.base.as_str();
    let mut seen = 0;
    loop {
        if base == "builder" || stages.last().and_then(|s| s.name.as_deref()) == Some("builder") {
            return true;
        }
        match stages
            .iter()
            .find(|stage| stage.name.as_deref() == Some(base))
        {
            Some(stage) => {
                if stage.base == "builder" {
                    return true;
                }
                base = stage.base.as_str();
            }
            None => return false,
        }
        seen += 1;
        if seen > stages.len() {
            return false;
        }
    }
}

/// Contract between the toolchain Dockerfile and `docker/extract-libs.sh`: the
/// image that a plain `docker build` produces (the last stage) must contain the
/// paths the script copies, and a scratch default stage must be created with a
/// placeholder command. Both failures were observed on the real image before
/// this check existed.
pub(crate) fn scan_extraction(dockerfile: &str, script: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let paths = extraction_paths(script);
    if paths.is_empty() {
        violations.push(
            "docker/extract-libs.sh reads no `docker cp \"${container_id}:<path>\"` source"
                .to_string(),
        );
        return violations;
    }
    let stages = dockerfile_stages(dockerfile);
    let Some(last) = stages.last() else {
        violations.push("the toolchain Dockerfile has no FROM stage".to_string());
        return violations;
    };
    let builder_backed = default_stage_is_builder(&stages);
    for path in &paths {
        let provided = if last.base == "scratch" {
            last.copies
                .iter()
                .any(|dest| path == dest || path.starts_with(&format!("{dest}/")))
        } else if builder_backed {
            path.starts_with("/usr/local/")
        } else {
            false
        };
        if !provided {
            violations.push(format!(
                "docker/extract-libs.sh copies `{path}` but the default image stage (base `{}`, name `{}`) does not provide it; a plain `docker build` builds the last stage, so make the builder stage last or copy from the artifact tree",
                last.base,
                last.name.as_deref().unwrap_or("-")
            ));
        }
    }
    if last.base == "scratch" && !create_has_command(script) {
        violations.push(
            "the default image stage is `scratch`, which `docker create \"${IMAGE}\"` cannot create without a placeholder command".to_string(),
        );
    }
    violations
}

/// True for a Dockerfile's file name. A per-Dockerfile ignore file sits beside
/// it and shares the prefix, but it is not a build definition.
fn is_dockerfile_name(name: &str) -> bool {
    name.starts_with("Dockerfile") && !name.ends_with(".dockerignore")
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
        if !is_dockerfile_name(name) {
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

    // Every local COPY/ADD source must survive the Dockerfile's effective
    // ignore file, or the build fails before any RUN step.
    for violation in scan_build_context(root)? {
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

    // The extraction step must read paths the default build target produces.
    let extract_path = root.join("docker").join("extract-libs.sh");
    let extract_script = std::fs::read_to_string(&extract_path)
        .with_context(|| format!("failed to read {}", extract_path.display()))?;
    for violation in scan_extraction(&dockerfile, &extract_script) {
        failures.push(violation);
    }

    let git_available = crate::sample_check::git_repository_present(root);
    if !git_available {
        println!(
            "check-pins: no git repository in {}; the submodule checkouts cannot be read, so the \
             checkout comparison is skipped and the recorded pins are still checked",
            root.display()
        );
    }
    let submodules: Vec<SubmodulePin> = if git_available {
        crate::pin_env::capture_submodules(root)
            .into_iter()
            .map(|s| SubmodulePin {
                path: s.path,
                sha: s.sha,
                describe: s.describe,
            })
            .collect()
    } else {
        Vec::new()
    };

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

    let mismatches = if git_available {
        compare_pins(&dockerfile, &submodules, &environment_json)
    } else {
        compare_pins_without_checkout(&dockerfile, &environment_json)
    };
    for mismatch in mismatches {
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
    fn a_dockerignore_file_is_not_a_dockerfile() {
        assert!(is_dockerfile_name("Dockerfile.build"));
        assert!(is_dockerfile_name("Dockerfile"));
        assert!(!is_dockerfile_name("Dockerfile.build.dockerignore"));
        assert!(!is_dockerfile_name("compose.build.yml"));
    }

    #[test]
    fn a_from_an_earlier_stage_needs_no_digest() {
        let files = vec![(
            "docker/Dockerfile.build".to_string(),
            "FROM alpine@sha256:bfe7ceb9f09ec0566c48fb93356e45a47a00be5a1dd14fa5b909e7709c66b034 AS builder\nFROM builder AS default\n"
                .to_string(),
        )];
        assert!(scan_from_pins(&files).is_empty());
        let unknown = vec![(
            "docker/Dockerfile.build".to_string(),
            "FROM builder AS default\n".to_string(),
        )];
        assert_eq!(scan_from_pins(&unknown).len(), 1);
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

    #[test]
    fn without_a_checkout_the_recorded_pins_are_still_compared() {
        // Without a checkout there is no `git describe` string, so a version
        // comment can only be compared when the Dockerfile names a SHA prefix.
        let dockerfile = DOCKERFILE.replace("0.16.0", "5a1a854");
        let mismatches = compare_pins_without_checkout(&dockerfile, ENVIRONMENT);
        assert!(mismatches.is_empty(), "{mismatches:?}");

        let stale = dockerfile.replace("53ce25d", "566c518");
        let mismatches = compare_pins_without_checkout(&stale, ENVIRONMENT);
        assert_eq!(mismatches.len(), 1, "{mismatches:?}");
        assert!(mismatches[0].message.contains("environment.json"));
        assert!(
            !mismatches[0].message.contains("submodule checkout"),
            "a missing checkout is a skip, not a mismatch: {mismatches:?}"
        );
    }

    #[test]
    fn a_tarball_without_git_skips_the_checkout_comparison() {
        let dir = std::env::temp_dir().join(format!(
            "climb-check-pins-tarball-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("docker")).expect("create the docker directory");
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        std::fs::copy(
            root.join("docker/Dockerfile.build"),
            dir.join("docker/Dockerfile.build"),
        )
        .expect("copy the toolchain Dockerfile");
        std::fs::copy(
            root.join("docker/extract-libs.sh"),
            dir.join("docker/extract-libs.sh"),
        )
        .expect("copy the extraction script");
        std::fs::write(dir.join("environment.json"), ENVIRONMENT).expect("write the fingerprint");

        assert!(!crate::sample_check::git_repository_present(&dir));
        run(&dir).expect("a tarball without git is a printed skip, not a failure");
        let _ = std::fs::remove_dir_all(&dir);
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

    /// Regression: the repository-wide `.dockerignore` excludes `third_party/`
    /// for the viewer image, so a builder that compiles the pinned submodules
    /// needs its own ignore file. Without it the build stops at the first
    /// `COPY third_party/...` with `"/third_party/...": not found`.
    #[test]
    fn every_dockerfile_copy_source_is_in_its_build_context() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let violations = scan_build_context(root).expect("the build contexts are readable");
        assert!(violations.is_empty(), "{violations:#?}");
    }

    #[test]
    fn an_ignored_copy_source_is_reported() {
        let text = "FROM debian@sha256:abc\nCOPY third_party/MTL /src/MTL\n";
        let rules = parse_ignore("target\nthird_party\n");
        let sources = copy_sources(text);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].1, vec!["third_party/MTL".to_string()]);
        assert!(is_ignored(&rules, &sources[0].1[0]));
    }

    #[test]
    fn a_negated_rule_reincludes_a_copy_source() {
        let rules = parse_ignore("third_party\n!third_party/MTL\n");
        assert!(is_ignored(&rules, "third_party/liboqs"));
        assert!(!is_ignored(&rules, "third_party/MTL"));
    }

    #[test]
    fn a_double_star_rule_matches_nested_and_root_paths() {
        // Docker's `**/` also matches zero directories, so `**/*.log` covers a
        // log file at the context root as well as one below a directory.
        let rules = parse_ignore("**/*.log\n");
        assert!(is_ignored(&rules, "target/ci/docker.log"));
        assert!(is_ignored(&rules, "docker.log"));
        let single = parse_ignore("*.log\n");
        assert!(is_ignored(&single, "docker.log"));
        assert!(!is_ignored(&single, "target/docker.log"));
    }

    #[test]
    fn a_directory_rule_also_excludes_its_contents() {
        let rules = parse_ignore("third_party\n");
        assert!(is_ignored(&rules, "third_party"));
        assert!(is_ignored(&rules, "third_party/MTL/README.md"));
        assert!(!is_ignored(&rules, "crates/third_party"));
    }

    #[test]
    fn copy_from_an_earlier_stage_is_not_read_from_the_context() {
        let text = "COPY --from=builder --chown=0:0 /usr/local/lib /out/lib\n";
        assert!(copy_sources(text).is_empty());
    }

    #[test]
    fn the_json_form_of_copy_is_checked_too() {
        let text = "COPY [\"third_party/MTL\", \"/src/MTL\"]\n";
        let sources = copy_sources(text);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].1, vec!["third_party/MTL".to_string()]);
        assert!(is_ignored(
            &parse_ignore("third_party\n"),
            "third_party/MTL"
        ));
    }

    #[test]
    fn urls_and_absolute_paths_are_not_context_reads() {
        assert!(!reads_local_context("https://example.test/archive.tar.gz"));
        assert!(!reads_local_context("/usr/local/lib"));
        assert!(!reads_local_context("git@example.test:repo.git"));
        assert!(reads_local_context("third_party/liboqs"));
        assert!(reads_local_context("."));
    }

    /// The extraction script reads `/usr/local/{include,lib}`; a plain
    /// `docker build` builds the last stage, so the last stage must be the
    /// builder. Before this contract the last stage was the scratch artifact
    /// tree, and `docker create` failed with "no command specified".
    #[test]
    fn the_toolchain_extraction_reads_a_path_the_default_stage_provides() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let dockerfile = std::fs::read_to_string(root.join("docker/Dockerfile.build"))
            .expect("toolchain Dockerfile is committed");
        let script = std::fs::read_to_string(root.join("docker/extract-libs.sh"))
            .expect("extraction script is committed");
        let violations = scan_extraction(&dockerfile, &script);
        assert!(violations.is_empty(), "{violations:#?}");
        assert_eq!(
            extraction_paths(&script),
            vec!["/usr/local/include", "/usr/local/lib"]
        );
    }

    #[test]
    fn an_extraction_from_a_scratch_default_stage_is_reported() {
        let dockerfile = "\
FROM debian@sha256:abc AS builder
FROM scratch AS final
COPY --from=builder /usr/local/lib /out/lib
";
        let script = "container_id=\"$(docker create \"${IMAGE}\")\"\n\
docker cp \"${container_id}:/usr/local/lib\" \"${DEST}/lib\"\n";
        let violations = scan_extraction(dockerfile, script);
        assert_eq!(violations.len(), 2, "{violations:#?}");
        assert!(violations[0].contains("/usr/local/lib"), "{violations:#?}");
        assert!(violations[0].contains("scratch"), "{violations:#?}");
        assert!(
            violations[1].contains("placeholder command"),
            "{violations:#?}"
        );
    }

    #[test]
    fn a_scratch_default_stage_with_the_artifact_tree_and_a_command_passes() {
        let dockerfile = "\
FROM debian@sha256:abc AS builder
FROM scratch AS final
COPY --from=builder /usr/local/lib /out/lib
COPY --from=builder /usr/local/include /out/include
";
        let script = "container_id=\"$(docker create \"${IMAGE}\" /out)\"\n\
docker cp \"${container_id}:/out/lib\" \"${DEST}/lib\"\n\
docker cp \"${container_id}:/out/include\" \"${DEST}/include\"\n";
        assert!(
            scan_extraction(dockerfile, script).is_empty(),
            "{:#?}",
            scan_extraction(dockerfile, script)
        );
        assert!(create_has_command(script));
        assert!(!create_has_command(
            "container_id=\"$(docker create \"${IMAGE}\")\"\n"
        ));
    }

    #[test]
    fn a_default_stage_that_is_not_the_builder_is_reported() {
        let dockerfile = "\
FROM debian@sha256:abc AS builder
FROM debian@sha256:abc AS runtime
COPY --from=builder /usr/local/lib /usr/local/lib
";
        let script = "docker cp \"${container_id}:/usr/local/lib\" \"${DEST}/lib\"\n";
        let violations = scan_extraction(dockerfile, script);
        assert_eq!(violations.len(), 1, "{violations:#?}");
        assert!(
            violations[0].contains("does not provide"),
            "{violations:#?}"
        );
    }
}
