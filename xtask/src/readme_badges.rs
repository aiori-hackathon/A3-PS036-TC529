//! `cargo xtask readme-badges`: generate the README badge block from one
//! configuration file.
//!
//! The block between the `<!-- badges:begin -->` and `<!-- badges:end -->`
//! markers holds two centred rows of badges: an identity row (the hackathon,
//! the problem statement and the track) and a proof row (the language, the
//! introduction video and the claims count). Every badge is an `<a>` element
//! wrapping an `<img>` element, so the link sits on the anchor and the image
//! carries the alternative text.
//!
//! The block is generated, never hand-edited: the command rewrites it in
//! place, and `cargo xtask docs-check` regenerates it in memory and fails when
//! the committed copy has drifted.
//!
//! The badge images are served by the shields.io static endpoint; every query
//! value is percent-encoded in code, so a space becomes `%20`, an ampersand
//! `%26`, a slash `%2F` and a plus `%2B`.

use anyhow::{bail, Context, Result};
use clap::Args;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use climb_report::ClimbResults;

/// Marker opening the managed badge block in the README.
pub(crate) const BADGES_BEGIN: &str = "<!-- badges:begin -->";

/// Marker closing the managed badge block in the README.
pub(crate) const BADGES_END: &str = "<!-- badges:end -->";

/// The badge configuration file, relative to the workspace root.
pub(crate) const DEFAULT_CONFIG: &str = "docs/readme-config.toml";

/// The consolidated results artifact the claims count is read from. The
/// committed sample is used by default so that a fresh clone regenerates the
/// same block as the repository carries.
pub(crate) const DEFAULT_RESULTS: &str = "docs/sample-output/climb-results.json";

/// The shields.io endpoint that serves the static badge images.
const SHIELDS_ENDPOINT: &str = "https://img.shields.io/static/v1";

/// The placeholder in a badge message that carries the claims count.
const CLAIMS_PLACEHOLDER: &str = "{claims}";

/// The message a badge carries when no claims count can be computed.
const CLAIMS_UNAVAILABLE: &str = "unavailable";

/// The two badge rows of the block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    /// The identity row: hackathon, problem statement, track.
    Identity,
    /// The proof row: language, introduction video, claims count.
    Proof,
}

/// The badge slots in block order: the three identity badges, then the three
/// proof badges. The order is fixed by the two-row layout; each slot names
/// the configuration table it is rendered from.
const SLOT_KEYS: [(Row, &str); 6] = [
    (Row::Identity, "hackathon"),
    (Row::Identity, "problem_statement"),
    (Row::Identity, "track"),
    (Row::Proof, "built_with"),
    (Row::Proof, "video"),
    (Row::Proof, "claims"),
];

/// The configuration file, parsed.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    badges: Badges,
    /// The shared file also carries the commit-graph table, which has its own
    /// typed reader; the badge renderer only has to tolerate it here.
    #[allow(dead_code)] // read by serde to satisfy deny_unknown_fields, not by this module
    #[serde(default)]
    commit_graph: Option<toml::Value>,
}

/// The `badges` table of the configuration file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Badges {
    /// The shields.io rendering style.
    style: String,
    /// The colour of the small logo glyph.
    logo_color: String,
    /// The link targets a badge may name.
    links: BTreeMap<String, String>,
    /// The brand colours a badge may name.
    colours: BTreeMap<String, String>,
    /// The identity-row badges.
    hackathon: BadgeSpec,
    /// The problem-statement badge.
    problem_statement: BadgeSpec,
    /// The track badge.
    track: BadgeSpec,
    /// The language badge.
    built_with: BadgeSpec,
    /// The introduction-video badge.
    video: BadgeSpec,
    /// The claims badge.
    claims: BadgeSpec,
}

impl Badges {
    /// The configuration of one badge slot.
    fn spec(&self, key: &str) -> &BadgeSpec {
        match key {
            "hackathon" => &self.hackathon,
            "problem_statement" => &self.problem_statement,
            "track" => &self.track,
            "built_with" => &self.built_with,
            "video" => &self.video,
            "claims" => &self.claims,
            other => unreachable!("unknown badge slot `{other}`"),
        }
    }

    /// The hex value of one named brand colour, without the leading `#`.
    fn colour(&self, name: &str, badge: &str) -> Result<String> {
        let value = self
            .colours
            .get(name)
            .with_context(|| format!("badge `{badge}` names the unknown colour `{name}`"))?;
        Ok(value.trim_start_matches('#').to_string())
    }
}

/// The configuration of one badge.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BadgeSpec {
    /// The left-hand text of the badge.
    label: String,
    /// The right-hand text of the badge; `{claims}` is substituted.
    message: String,
    /// The alternative text of the badge image.
    alt: String,
    /// The key of the link target, in `badges.links`.
    link: String,
    /// The key of the label colour, in `badges.colours`.
    label_color: String,
    /// The key of the message colour, in `badges.colours`.
    message_color: String,
    /// The shields.io logo slug, when the badge carries one.
    #[serde(default)]
    logo: Option<String>,
}

/// The claims count behind the claims badge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaimsCount {
    /// `passed` of `total` claims re-resolve in the results artifact.
    Count {
        /// The claims that re-resolve against the artifact.
        passed: usize,
        /// The claims the registry holds.
        total: usize,
    },
    /// The results artifact is missing, so no count can be computed.
    Missing,
    /// The results artifact carries no claims at all.
    Empty,
}

impl ClaimsCount {
    /// The message the claims badge carries.
    pub(crate) fn message(&self) -> String {
        match self {
            ClaimsCount::Count { passed, total } => format!("{passed}/{total}"),
            ClaimsCount::Missing | ClaimsCount::Empty => CLAIMS_UNAVAILABLE.to_string(),
        }
    }

    /// Whether a count could be computed at all.
    pub(crate) fn is_available(&self) -> bool {
        matches!(self, ClaimsCount::Count { .. })
    }
}

/// One rendered badge: the anchor it wraps and the image it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Badge {
    /// The row the badge belongs to.
    pub(crate) row: Row,
    /// The left-hand text of the badge.
    pub(crate) label: String,
    /// The right-hand text of the badge.
    pub(crate) message: String,
    /// The anchor target.
    pub(crate) href: String,
    /// The alternative text of the image.
    pub(crate) alt: String,
    /// The image URL, served by the shields.io static endpoint.
    pub(crate) image_url: String,
}

/// What the command produced.
#[derive(Debug)]
pub(crate) struct Outcome {
    /// The badges, in block order.
    pub(crate) badges: Vec<Badge>,
    /// The claims count the block was built with.
    pub(crate) claims: ClaimsCount,
    /// Whether the README was rewritten.
    pub(crate) written: bool,
}

#[derive(Args)]
pub(crate) struct ReadmeBadgesArgs {
    /// Badge configuration file to read.
    #[arg(long, default_value = DEFAULT_CONFIG)]
    pub(crate) config: PathBuf,

    /// README whose managed block is regenerated.
    #[arg(long, default_value = "README.md")]
    pub(crate) readme: PathBuf,

    /// Consolidated results artifact the claims count is read from.
    #[arg(long, default_value = DEFAULT_RESULTS)]
    pub(crate) results: PathBuf,

    /// Fail when the results artifact is missing, instead of reporting the
    /// claims count as unavailable.
    #[arg(long)]
    pub(crate) strict: bool,

    /// Compare the committed block with a fresh generation instead of writing.
    #[arg(long)]
    pub(crate) check: bool,
}

/// Percent-encode a value for use in a URL query parameter.
///
/// Every byte outside the unreserved set (`A-Z`, `a-z`, `0-9`, `-`, `.`, `_`,
/// `~`) is written as `%XX` with upper-case hex digits, so a space becomes
/// `%20`, an ampersand `%26`, a slash `%2F` and a plus `%2B`.
pub(crate) fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Escape a value for use inside a double-quoted HTML attribute.
fn escape_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Compare two JSON values, with float tolerance.
fn values_match(expected: &Value, found: &Value) -> bool {
    match (expected, found) {
        (Value::Number(a), Value::Number(b)) => match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => a == b || ((a - b) / a.abs().max(b.abs()).max(1.0)).abs() < 1e-9,
            _ => a == b,
        },
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| values_match(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| values_match(v, w)))
        }
        _ => expected == found,
    }
}

/// Read and parse the badge configuration file.
fn load_config(path: &Path) -> Result<ConfigFile> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
}

/// The claims of `registry` that re-resolve inside `raw`.
///
/// A claim counts as verified only when its pointer resolves in the
/// document the registry was read from and resolves to the value the
/// registry recorded. A pointer that does not resolve there, or that
/// resolves to another value, leaves the claim unverified — the badge count
/// must never over-report what the artifact itself proves.
fn verified_claims<'a>(
    registry: &'a [climb_report::html::Claim],
    raw: &Value,
) -> Vec<&'a climb_report::html::Claim> {
    registry
        .iter()
        .filter(|claim| {
            raw.pointer(&claim.pointer)
                .is_some_and(|found| values_match(&claim.value, found))
        })
        .collect()
}

/// Count the claims of the results artifact that re-resolve inside it.
///
/// The registry is built exactly as the report builds it, and every claim is
/// re-resolved against the artifact the registry was read from: a claim whose
/// pointer no longer resolves, or resolves to another value, is not verified.
pub(crate) fn claims_count(results_path: &Path) -> Result<ClaimsCount> {
    let text = match std::fs::read_to_string(results_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ClaimsCount::Missing);
        }
        Err(error) => {
            return Err(anyhow::Error::new(error)
                .context(format!("failed to read {}", results_path.display())));
        }
    };
    let raw: Value = serde_json::from_str(&text)
        .with_context(|| format!("{} is not valid JSON", results_path.display()))?;
    let parsed: ClimbResults = serde_json::from_value(raw.clone())
        .with_context(|| format!("failed to parse {}", results_path.display()))?;
    let registry = climb_report::claims(&parsed).with_context(|| {
        format!(
            "failed to build the claims registry of {}",
            results_path.display()
        )
    })?;
    let total = registry.len();
    if total == 0 {
        return Ok(ClaimsCount::Empty);
    }
    let passed = verified_claims(&registry, &raw).len();
    Ok(ClaimsCount::Count { passed, total })
}

/// Build the badge URLs and the block they are rendered into.
fn build(config: &ConfigFile, claims: &ClaimsCount) -> Result<Vec<Badge>> {
    let message = claims.message();
    let mut badges = Vec::with_capacity(SLOT_KEYS.len());
    for (row, key) in SLOT_KEYS {
        let spec = config.badges.spec(key);
        let href = config
            .badges
            .links
            .get(&spec.link)
            .with_context(|| {
                format!(
                    "badge `{key}` names the unknown link target `{}`",
                    spec.link
                )
            })?
            .clone();
        let label_color = config.badges.colour(&spec.label_color, key)?;
        let message_color = config.badges.colour(&spec.message_color, key)?;
        let message = spec.message.replace(CLAIMS_PLACEHOLDER, &message);
        let alt = spec.alt.replace(CLAIMS_PLACEHOLDER, &message);
        let image_url = shields_url(config, spec, &message, &label_color, &message_color);
        badges.push(Badge {
            row,
            label: spec.label.clone(),
            message,
            href,
            alt,
            image_url,
        });
    }
    Ok(badges)
}

/// The shields.io URL of one badge image.
fn shields_url(
    config: &ConfigFile,
    spec: &BadgeSpec,
    message: &str,
    label_color: &str,
    message_color: &str,
) -> String {
    let mut url = format!(
        "{SHIELDS_ENDPOINT}?label={}&message={}&color={}&labelColor={}&style={}",
        percent_encode(&spec.label),
        percent_encode(message),
        percent_encode(message_color),
        percent_encode(label_color),
        percent_encode(&config.badges.style),
    );
    if let Some(logo) = &spec.logo {
        url.push_str(&format!(
            "&logo={}&logoColor={}",
            percent_encode(logo),
            percent_encode(&config.badges.logo_color)
        ));
    }
    url
}

/// Render the managed block: two centred rows of anchored badge images.
pub(crate) fn render_block(badges: &[Badge]) -> String {
    let mut out = String::new();
    out.push_str(BADGES_BEGIN);
    out.push('\n');
    out.push_str("<div align=\"center\">\n");
    let mut row: Option<Row> = None;
    for badge in badges {
        if row != Some(badge.row) {
            if row.is_some() {
                out.push_str("  <br>\n");
            }
            row = Some(badge.row);
        }
        out.push_str(&format!(
            "  <a href=\"{}\" target=\"_blank\" rel=\"noopener\"><img alt=\"{}\" src=\"{}\"></a>\n",
            escape_attribute(&badge.href),
            escape_attribute(&badge.alt),
            escape_attribute(&badge.image_url)
        ));
    }
    out.push_str("</div>\n");
    out.push_str(BADGES_END);
    out.push('\n');
    out
}

/// The managed block of a README, including its markers. `None` when the
/// markers are absent or out of order.
pub(crate) fn extract_block(readme: &str) -> Option<String> {
    let begin = readme.find(BADGES_BEGIN)?;
    let end = readme.find(BADGES_END)?;
    if end < begin {
        return None;
    }
    Some(readme[begin..end + BADGES_END.len()].to_string())
}

/// Replace the managed block of a README. Fails when the markers are absent.
pub(crate) fn replace_block(readme: &str, block: &str) -> Result<String> {
    let begin = readme
        .find(BADGES_BEGIN)
        .with_context(|| format!("the README carries no `{BADGES_BEGIN}` marker"))?;
    let end = readme
        .find(BADGES_END)
        .with_context(|| format!("the README carries no `{BADGES_END}` marker"))?;
    if end < begin {
        bail!("the `{BADGES_BEGIN}` marker comes after the `{BADGES_END}` marker");
    }
    let mut out = String::with_capacity(readme.len() + block.len());
    out.push_str(&readme[..begin]);
    out.push_str(block.trim_end_matches('\n'));
    out.push_str(&readme[end + BADGES_END.len()..]);
    Ok(out)
}

/// A line of a diff between the generated block and the committed one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DiffLine {
    /// A line both sides carry.
    Context(String),
    /// A line only the generated block carries.
    Removed(String),
    /// A line only the committed block carries.
    Added(String),
}

/// The differences between the generated block and the committed one.
pub(crate) fn diff_lines(expected: &str, actual: &str) -> Vec<DiffLine> {
    let expected: Vec<&str> = expected.lines().collect();
    let actual: Vec<&str> = actual.lines().collect();

    // Longest common subsequence of the two line sequences.
    let mut lengths = vec![vec![0usize; actual.len() + 1]; expected.len() + 1];
    for i in (0..expected.len()).rev() {
        for j in (0..actual.len()).rev() {
            lengths[i][j] = if expected[i] == actual[j] {
                lengths[i + 1][j + 1] + 1
            } else {
                lengths[i + 1][j].max(lengths[i][j + 1])
            };
        }
    }

    let mut lines = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < expected.len() && j < actual.len() {
        if expected[i] == actual[j] {
            lines.push(DiffLine::Context(expected[i].to_string()));
            i += 1;
            j += 1;
        } else if lengths[i + 1][j] >= lengths[i][j + 1] {
            lines.push(DiffLine::Removed(expected[i].to_string()));
            i += 1;
        } else {
            lines.push(DiffLine::Added(actual[j].to_string()));
            j += 1;
        }
    }
    while i < expected.len() {
        lines.push(DiffLine::Removed(expected[i].to_string()));
        i += 1;
    }
    while j < actual.len() {
        lines.push(DiffLine::Added(actual[j].to_string()));
        j += 1;
    }
    lines
}

/// Render a diff as text, for the error message of a drift check.
pub(crate) fn render_diff(lines: &[DiffLine]) -> String {
    let mut out = String::new();
    for line in lines {
        match line {
            DiffLine::Context(text) => out.push_str(&format!("  {text}\n")),
            DiffLine::Removed(text) => out.push_str(&format!("- {text}\n")),
            DiffLine::Added(text) => out.push_str(&format!("+ {text}\n")),
        }
    }
    out
}

/// Generate the badge block from the configuration and the results artifact.
pub(crate) fn generate(
    config_path: &Path,
    results_path: &Path,
) -> Result<(Vec<Badge>, String, ClaimsCount)> {
    let config = load_config(config_path)?;
    let claims = claims_count(results_path)?;
    let badges = build(&config, &claims)?;
    let block = render_block(&badges);
    Ok((badges, block, claims))
}

/// Run the badge generation.
pub(crate) fn run(args: &ReadmeBadgesArgs) -> Result<Outcome> {
    let (badges, block, claims) = generate(&args.config, &args.results)?;

    if args.strict && !claims.is_available() {
        bail!(
            "{} is missing, so the claims badge would read `{}` — rerun without `--strict` \
             to generate the block anyway",
            args.results.display(),
            CLAIMS_UNAVAILABLE
        );
    }

    let readme = std::fs::read_to_string(&args.readme)
        .with_context(|| format!("failed to read {}", args.readme.display()))?;

    if args.check {
        let committed = extract_block(&readme).with_context(|| {
            format!(
                "{} carries no managed badge block — run `cargo xtask readme-badges`",
                args.readme.display()
            )
        })?;
        if committed.trim_end() != block.trim_end() {
            bail!(
                "{}: the managed badge block has drifted from the generated one\n{}",
                args.readme.display(),
                render_diff(&diff_lines(&block, &committed))
            );
        }
        return Ok(Outcome {
            badges,
            claims,
            written: false,
        });
    }

    let updated = replace_block(&readme, &block)?;
    std::fs::write(&args.readme, &updated)
        .with_context(|| format!("failed to write {}", args.readme.display()))?;

    Ok(Outcome {
        badges,
        claims,
        written: true,
    })
}

/// The `readme-badges` subcommand.
pub(crate) fn cmd_readme_badges(args: ReadmeBadgesArgs) -> Result<()> {
    let outcome = run(&args)?;
    for (index, badge) in outcome.badges.iter().enumerate() {
        println!("badge {}/{}", index + 1, outcome.badges.len());
        println!("  label  : {}", badge.label);
        println!("  message: {}", badge.message);
        println!("  image  : {}", badge.image_url);
        println!("  link   : {}", badge.href);
    }
    match &outcome.claims {
        ClaimsCount::Count { passed, total } => {
            println!("claims verified: {passed}/{total}");
        }
        ClaimsCount::Missing => {
            eprintln!(
                "warning: {} is missing — the claims badge reads `{}`",
                args.results.display(),
                CLAIMS_UNAVAILABLE
            );
        }
        ClaimsCount::Empty => {
            eprintln!(
                "warning: {} carries no claims — the claims badge reads `{}`",
                args.results.display(),
                CLAIMS_UNAVAILABLE
            );
        }
    }
    if outcome.written {
        println!("wrote the badge block in {}", args.readme.display());
    } else {
        println!("the badge block in {} is up to date", args.readme.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A configuration exercising every slot, written to a temporary file.
    fn write_config(dir: &Path) -> PathBuf {
        let path = dir.join("readme-config.toml");
        let text = r##"
[badges]
style = "for-the-badge"
logo_color = "white"

[badges.links]
portal = "https://portal.example.invalid/guideline/"
video = "https://video.example.invalid/watch?v=abc"
rust = "https://rust.example.invalid/"
claims = "docs/deck-facts.md"

[badges.colours]
orange = "#C55E33"
slate = "#38454E"
teal = "#5C8E8D"

[badges.hackathon]
label = "AIORI 3"
message = "Hackathon"
alt = "AIORI 3 Hackathon"
link = "portal"
label_color = "slate"
message_color = "orange"

[badges.problem_statement]
label = "problem statement"
message = "PS-036"
alt = "Problem statement PS-036"
link = "portal"
label_color = "slate"
message_color = "orange"

[badges.track]
label = "track 2"
message = "Cyber Security"
alt = "Track 2 Cyber Security"
link = "portal"
label_color = "slate"
message_color = "orange"

[badges.built_with]
label = "built with"
message = "Rust"
alt = "Built with Rust"
link = "rust"
label_color = "slate"
message_color = "teal"
logo = "rust"

[badges.video]
label = "intro video"
message = "YouTube"
alt = "YouTube introduction video"
link = "video"
label_color = "slate"
message_color = "teal"
logo = "youtube"

[badges.claims]
label = "claims verified"
message = "{claims}"
alt = "Claims verified: {claims}"
link = "claims"
label_color = "teal"
message_color = "orange"
"##;
        std::fs::write(&path, text).expect("write the configuration");
        path
    }

    /// A results artifact carrying the claims of an empty run.
    fn write_results(dir: &Path) -> PathBuf {
        let path = dir.join("climb-results.json");
        let text = r##"{
  "version": "1.0",
  "generated_at": "2026-01-01T00:00:00Z",
  "workspace_root": "/tmp/climb",
  "benchmark": { "signing": [], "verifying": [] },
  "wire_size": [],
  "memory": {
    "measured": [],
    "analytical_model": {
      "tree_page_size_bytes": 4096,
      "tree_max_pages": 0,
      "randomizer_pages": 0,
      "hash_size_bytes": 32
    }
  },
  "environment": {
    "schema_version": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "system": {},
    "toolchain": {},
    "cpu": {},
    "submodules": [],
    "docker": {},
    "benchmark_runtime": {}
  },
  "derived": {
    "overall_wire_reduction_percent": null,
    "largest_wire_reduction_percent": null,
    "overall_verifying_trust_cached_speedup_avg": null,
    "headline": null
  },
  "warnings": []
}"##;
        std::fs::write(&path, text).expect("write the results");
        path
    }

    /// A temporary directory for one test.
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "climb-badges-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the temporary directory");
        dir
    }

    #[test]
    fn percent_encoding_covers_spaces_ampersands_slashes_and_plus_signs() {
        assert_eq!(percent_encode("AIORI 3"), "AIORI%203");
        assert_eq!(percent_encode("a&b"), "a%26b");
        assert_eq!(percent_encode("a/b"), "a%2Fb");
        assert_eq!(percent_encode("a+b"), "a%2Bb");
    }

    #[test]
    fn percent_encoding_leaves_the_unreserved_set_alone() {
        assert_eq!(percent_encode("AZaz09-._~"), "AZaz09-._~");
        assert_eq!(percent_encode(""), "");
    }

    #[test]
    fn percent_encoding_handles_non_ascii_text() {
        // A multi-byte character is encoded byte by byte, never as raw UTF-8.
        let encoded = percent_encode("é");
        assert_eq!(encoded, "%C3%A9");
        assert!(encoded.is_ascii());
    }

    #[test]
    fn the_generated_block_carries_two_centred_rows_of_anchored_images() {
        let dir = temp_dir("rows");
        let config = write_config(&dir);
        let results = write_results(&dir);
        let (badges, block, _) = generate(&config, &results).expect("generate the block");

        assert_eq!(badges.len(), 6);
        assert_eq!(badges[0].row, Row::Identity);
        assert_eq!(badges[2].row, Row::Identity);
        assert_eq!(badges[3].row, Row::Proof);
        assert_eq!(badges[5].row, Row::Proof);

        assert!(block.starts_with(BADGES_BEGIN));
        assert!(block.contains(BADGES_END));
        assert!(block.contains("<div align=\"center\">"));
        assert_eq!(
            block.matches("<br>").count(),
            1,
            "one break between the rows"
        );

        // Every badge is an anchor wrapping an image: the link is on the
        // anchor, never inside the image.
        assert_eq!(block.matches("<a href=").count(), 6);
        assert_eq!(block.matches("<img ").count(), 6);
        assert!(!block.contains("]("), "no markdown image links");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_badge_image_carries_a_meaningful_alt_text() {
        let dir = temp_dir("alt");
        let config = write_config(&dir);
        let results = write_results(&dir);
        let (badges, _, _) = generate(&config, &results).expect("generate the block");

        for badge in &badges {
            assert!(!badge.alt.trim().is_empty(), "{badge:?} has no alt text");
            assert!(
                badge.alt.chars().count() > 4,
                "{badge:?} has a terse alt text"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_image_urls_use_the_static_endpoint_with_the_configured_colours() {
        let dir = temp_dir("url");
        let config = write_config(&dir);
        let results = write_results(&dir);
        let (badges, _, _) = generate(&config, &results).expect("generate the block");

        for badge in &badges {
            assert!(
                badge
                    .image_url
                    .starts_with("https://img.shields.io/static/v1?"),
                "{badge:?}"
            );
            assert!(badge.image_url.contains("style=for-the-badge"), "{badge:?}");
        }
        // The identity row and the language badge sit on the slate label and
        // carry an orange message; the video badge carries a teal message and
        // the claims badge swaps the two.
        for badge in &badges[..3] {
            assert!(badge.image_url.contains("labelColor=38454E"), "{badge:?}");
            assert!(badge.image_url.contains("color=C55E33"), "{badge:?}");
        }
        assert!(
            badges[3].image_url.contains("labelColor=38454E"),
            "{:?}",
            badges[3]
        );
        assert!(
            badges[3].image_url.contains("color=5C8E8D"),
            "{:?}",
            badges[3]
        );
        assert!(
            badges[4].image_url.contains("color=5C8E8D"),
            "{:?}",
            badges[4]
        );
        assert!(
            badges[5].image_url.contains("labelColor=5C8E8D"),
            "{:?}",
            badges[5]
        );
        assert!(
            badges[5].image_url.contains("color=C55E33"),
            "{:?}",
            badges[5]
        );

        assert!(badges[3].image_url.contains("logo=rust"), "{:?}", badges[3]);
        assert!(
            badges[3].image_url.contains("logoColor=white"),
            "{:?}",
            badges[3]
        );
        assert!(
            badges[4].image_url.contains("logo=youtube"),
            "{:?}",
            badges[4]
        );
        assert!(!badges[0].image_url.contains("logo="), "{:?}", badges[0]);
        assert!(!badges[5].image_url.contains("logo="), "{:?}", badges[5]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_generation_is_stable_across_runs() {
        let dir = temp_dir("stable");
        let config = write_config(&dir);
        let results = write_results(&dir);
        let (_, first, _) = generate(&config, &results).expect("generate the block");
        let (_, second, _) = generate(&config, &results).expect("generate the block");
        assert_eq!(first, second);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_results_artifact_yields_an_unavailable_claims_message() {
        let dir = temp_dir("missing");
        let config = write_config(&dir);
        let missing = dir.join("not-there.json");
        let (_, block, claims) = generate(&config, &missing).expect("generate the block");

        assert_eq!(claims, ClaimsCount::Missing);
        assert_eq!(claims.message(), "unavailable");
        assert!(block.contains("message=unavailable"), "{block}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strict_mode_fails_when_the_results_artifact_is_missing() {
        let dir = temp_dir("strict");
        let config = write_config(&dir);
        let results = dir.join("not-there.json");
        let readme = dir.join("README.md");
        std::fs::write(
            &readme,
            format!("# Title\n\n{BADGES_BEGIN}\nold\n{BADGES_END}\n"),
        )
        .expect("write the README");

        let args = ReadmeBadgesArgs {
            config,
            readme: readme.clone(),
            results,
            strict: true,
            check: false,
        };
        let error = run(&args).expect_err("strict mode must fail").to_string();
        assert!(error.contains("unavailable"), "unexpected message: {error}");
        let text = std::fs::read_to_string(&readme).expect("read the README");
        assert!(text.contains("old"), "strict mode must not write the block");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_generated_block_round_trips_through_the_readme() {
        let dir = temp_dir("roundtrip");
        let config = write_config(&dir);
        let results = write_results(&dir);
        let readme = dir.join("README.md");
        std::fs::write(
            &readme,
            format!("# Title\n\n{BADGES_BEGIN}\nold\n{BADGES_END}\n\nafter\n"),
        )
        .expect("write the README");

        let args = ReadmeBadgesArgs {
            config: config.clone(),
            readme: readme.clone(),
            results: results.clone(),
            strict: false,
            check: false,
        };
        let outcome = run(&args).expect("generate the block");
        assert!(outcome.written);

        // The README must now carry exactly the generated block.
        let (_, expected, _) = generate(&config, &results).expect("generate the block");
        let text = std::fs::read_to_string(&readme).expect("read the README");
        let committed = extract_block(&text).expect("the README carries the block");
        assert_eq!(committed.trim_end(), expected.trim_end());

        // A second run in check mode must find the committed block identical
        // to a fresh generation.
        let check_args = ReadmeBadgesArgs {
            config,
            readme: readme.clone(),
            results,
            strict: false,
            check: true,
        };
        let checked = run(&check_args).expect("the committed block is up to date");
        assert!(!checked.written);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_injected_edit_is_reported_as_a_drift() {
        let dir = temp_dir("drift");
        let config = write_config(&dir);
        let results = write_results(&dir);
        let readme = dir.join("README.md");
        std::fs::write(
            &readme,
            format!("# Title\n\n{BADGES_BEGIN}\nold\n{BADGES_END}\n"),
        )
        .expect("write the README");

        let args = ReadmeBadgesArgs {
            config: config.clone(),
            readme: readme.clone(),
            results: results.clone(),
            strict: false,
            check: false,
        };
        run(&args).expect("generate the block");

        // Inject an edit into the managed block.
        let text = std::fs::read_to_string(&readme).expect("read the README");
        let edited = text.replace("AIORI%203", "AIORI%204");
        assert_ne!(edited, text, "the injection must change the block");
        std::fs::write(&readme, edited).expect("write the edited README");

        let check_args = ReadmeBadgesArgs {
            config,
            readme,
            results,
            strict: false,
            check: true,
        };
        let error = run(&check_args)
            .expect_err("the edit must be caught")
            .to_string();
        assert!(error.contains("drifted"), "unexpected message: {error}");
        assert!(error.contains("AIORI%203"), "unexpected message: {error}");
        assert!(error.contains("AIORI%204"), "unexpected message: {error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_diff_marks_the_changed_lines_only() {
        let lines = diff_lines("a\nb\nc\n", "a\nx\nc\n");
        assert_eq!(
            lines,
            vec![
                DiffLine::Context("a".to_string()),
                DiffLine::Removed("b".to_string()),
                DiffLine::Added("x".to_string()),
                DiffLine::Context("c".to_string()),
            ]
        );
    }

    #[test]
    fn a_configuration_with_an_unknown_key_is_rejected() {
        let dir = temp_dir("unknown");
        let path = dir.join("bad.toml");
        std::fs::write(
            &path,
            "[badges]\nstyle = \"for-the-badge\"\nsecret = \"x\"\n",
        )
        .expect("write the configuration");
        let error = load_config(&path).expect_err("an unknown key must be rejected");
        let message = format!("{error:#}");
        assert!(
            message.contains("unknown field"),
            "unexpected message: {message}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_committed_configuration_parses_and_names_every_slot() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        let config = load_config(&root.join(DEFAULT_CONFIG)).expect("the configuration parses");
        for (_, key) in SLOT_KEYS {
            let spec = config.badges.spec(key);
            assert!(!spec.label.is_empty(), "{key} has no label");
            assert!(!spec.message.is_empty(), "{key} has no message");
            assert!(!spec.alt.is_empty(), "{key} has no alt text");
            assert!(
                config.badges.links.contains_key(&spec.link),
                "{key} names an unknown link"
            );
            assert!(
                config.badges.colours.contains_key(&spec.label_color),
                "{key} names an unknown label colour"
            );
            assert!(
                config.badges.colours.contains_key(&spec.message_color),
                "{key} names an unknown message colour"
            );
        }
    }

    /// The workspace root, one level above the `xtask` crate.
    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace")
            .to_path_buf()
    }

    /// A claim the artifact cannot prove is not counted as verified.
    ///
    /// The registry is built from the parsed document, so a claim whose
    /// pointer the artifact does not carry — or carries with another value —
    /// cannot re-resolve and must not be counted. The badge must never
    /// over-report what the artifact proves.
    #[test]
    fn a_claim_that_cannot_re_resolve_in_the_artifact_is_not_verified() {
        // Test data: a registry of three claims and a document that resolves
        // one of them, resolves another to a different value, and does not
        // carry the third at all.
        let registry = vec![
            climb_report::html::Claim {
                id: "resolved".to_string(),
                pointer: "/derived/headline".to_string(),
                value: Value::String("a headline".to_string()),
                formatted: "a headline".to_string(),
            },
            climb_report::html::Claim {
                id: "drifted".to_string(),
                pointer: "/version".to_string(),
                value: Value::String("1.0".to_string()),
                formatted: "1.0".to_string(),
            },
            climb_report::html::Claim {
                id: "absent".to_string(),
                pointer: "/derived/bound_ok".to_string(),
                value: Value::Bool(true),
                formatted: "ok".to_string(),
            },
        ];
        let raw: Value = serde_json::json!({
            "version": "2.0",
            "derived": { "headline": "a headline" },
        });
        let verified: Vec<&str> = verified_claims(&registry, &raw)
            .into_iter()
            .map(|claim| claim.id.as_str())
            .collect();
        assert_eq!(
            verified,
            vec!["resolved"],
            "only the claim that re-resolves to the recorded value is verified"
        );
    }

    /// The claims count of a real artifact is the number of its claims that
    /// re-resolve inside it.
    /// The claims count of a real artifact is the number of its claims that
    /// re-resolve inside it.
    #[test]
    fn the_claims_count_counts_the_claims_that_re_resolve_in_the_artifact() {
        let dir = temp_dir("claims");
        let results = write_results(&dir);
        let count = claims_count(&results).expect("the fixture parses");

        let raw: Value =
            serde_json::from_str(&std::fs::read_to_string(&results).expect("read the fixture"))
                .expect("the fixture is JSON");
        let parsed: ClimbResults = serde_json::from_value(raw.clone()).expect("the fixture parses");
        let registry = climb_report::claims(&parsed).expect("the registry builds");
        assert!(!registry.is_empty(), "the fixture must carry claims");
        let re_resolved = verified_claims(&registry, &raw).len();
        assert_eq!(
            count,
            ClaimsCount::Count {
                passed: re_resolved,
                total: registry.len(),
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_claim_of_the_committed_sample_re_resolves_in_the_artifact() {
        let root = workspace_root();
        let count = claims_count(&root.join(DEFAULT_RESULTS)).expect("the sample artifact parses");
        match count {
            ClaimsCount::Count { passed, total } => {
                assert!(total > 0, "the sample artifact carries no claims");
                assert_eq!(
                    passed, total,
                    "not every claim of the sample artifact re-resolves in it"
                );
            }
            other => panic!("the sample artifact must carry claims: {other:?}"),
        }
    }
}
