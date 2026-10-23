//! Commit-graph generation: read the public history of the repository and
//! write one JSON document plus two self-contained SVG cards.
//!
//! The graph is a pure function of the history reachable from one commit and
//! the `[commit_graph]` configuration in `docs/readme-config.toml`. Nothing in
//! the output depends on the wall clock, the machine or the git version's
//! display defaults: dates are bucketed in a fixed-offset zone, the log format
//! is pinned, and the JSON is serialised with a fixed key order.
//!
//! The committed card must describe the public repository, never a private
//! development checkout whose history has been rewritten. Unless the checkout
//! itself is the public repository, history is read from a bare cache of
//! `source_url` at `source_ref`; the cache is cloned on first use and fetched
//! afterwards. `--source local` is an offline escape hatch for inspection
//! only: such a graph must not be committed.

mod check;
mod svg;
mod text;

use anyhow::{bail, Context, Result};
use clap::{Args, ValueEnum};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) use check::check_docs;

/// Version of the JSON document shape.
pub(crate) const SCHEMA_VERSION: u32 = 3;

/// Configuration file, relative to the workspace root.
pub(crate) const CONFIG_PATH: &str = "docs/readme-config.toml";
/// Full card, relative to the workspace root.
pub(crate) const JSON_PATH: &str = "docs/media/commit-graph.json";
pub(crate) const SVG_PATH: &str = "docs/media/commit-graph.svg";
pub(crate) const NARROW_PATH: &str = "docs/media/commit-graph-narrow.svg";
pub(crate) const COMPACT_PATH: &str = "docs/media/commit-graph-compact.svg";
pub(crate) const COMPACT_NARROW_PATH: &str = "docs/media/commit-graph-compact-narrow.svg";

/// Self-imposed ceiling for one card. Generation refuses to write past it.
pub(crate) const CARD_BUDGET_BYTES: usize = 60 * 1024;
/// Self-imposed ceiling for one compact strip.
pub(crate) const STRIP_BUDGET_BYTES: usize = 25 * 1024;

/// Default public repository the committed card describes.
const DEFAULT_SOURCE_URL: &str = "https://github.com/aiori-hackathon/A3-PS036-TC529.git";
/// Default human-facing URL of that repository.
const DEFAULT_REPO_WEB_URL: &str = "https://github.com/aiori-hackathon/A3-PS036-TC529";

/// Where the history comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub(crate) enum HistorySource {
    /// Read this checkout when its `origin` is the public repository,
    /// otherwise read the public cache.
    #[default]
    Auto,
    /// Read the public repository through a bare cache, fetching when needed.
    Public,
    /// Read this checkout offline. A graph built this way must not be
    /// committed: it may describe a private, rewritten history.
    Local,
}

#[derive(Args)]
pub(crate) struct CommitGraphArgs {
    /// Workspace root used for configuration and output.
    #[arg(long, default_value = ".")]
    pub(crate) root: PathBuf,

    /// Commit (or ref) to plot the graph up to. Defaults to HEAD for a local
    /// checkout and the configured source ref for the public cache.
    #[arg(long)]
    pub(crate) as_of: Option<String>,

    /// Configuration file, relative to the root.
    #[arg(long, default_value = CONFIG_PATH)]
    pub(crate) config: PathBuf,

    /// Directory that receives the JSON and both SVG cards.
    #[arg(long, default_value = "docs/media")]
    pub(crate) out_dir: PathBuf,

    /// Which history to read: the public repository (default), this checkout,
    /// or automatically the public one unless this checkout is the public one.
    #[arg(long, value_enum, default_value_t = HistorySource::Auto)]
    pub(crate) source: HistorySource,

    /// Verify the committed artifacts instead of regenerating them.
    ///
    /// Exits non-zero only when the graph is inconsistent (it does not
    /// reproduce from a history that contains its as-of commit, a card
    /// disagrees with the document, or a shape/budget rule fails). A graph that
    /// is merely behind `HEAD` prints a warning and still exits zero, and a
    /// checkout that simply lacks the as-of commit skips the history checks, so
    /// a hook can never fail offline.
    #[arg(long)]
    pub(crate) check: bool,
}

/// One `[commit_graph]` table. Unknown keys are rejected so a typo cannot
/// silently change nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommitGraphConfig {
    /// Zone used to bucket commits into calendar days.
    pub(crate) timezone: String,
    /// Public repository whose history the committed card describes.
    #[serde(default = "default_source_url")]
    pub(crate) source_url: String,
    /// Branch (or ref) of the public repository that the graph follows.
    #[serde(default = "default_source_ref")]
    pub(crate) source_ref: String,
    /// Bare cache of the public history, relative to the workspace root.
    #[serde(default = "default_source_cache")]
    pub(crate) source_cache: String,
    /// Human-facing URL used to build the README's commit permalink.
    #[serde(default = "default_repo_web_url")]
    pub(crate) repo_web_url: String,
    /// Git author names hidden from the contributor breakdown. Their commits
    /// are still counted: the totals must equal the commit count the public
    /// repository shows.
    #[serde(default)]
    pub(crate) exclude_authors: Vec<String>,
    /// Git author name -> display name.
    #[serde(default)]
    pub(crate) display_names: BTreeMap<String, String>,
    /// Patterns; a commit whose every changed path matches one of them is
    /// counted and reported as a graph refresh rather than hidden.
    #[serde(default)]
    pub(crate) refresh_commits_whose_files_all_match: Vec<String>,
    /// Milestones drawn on the timeline.
    #[serde(default)]
    pub(crate) milestones: Vec<MilestoneConfig>,
    /// Presentation knobs. Every value has a default, so an older
    /// configuration file keeps working.
    #[serde(default)]
    pub(crate) style: StyleConfig,
    /// Per-file size ceiling for both SVG cards.
    pub(crate) budget_kb: u32,
    /// Per-file size ceiling for both compact strips.
    #[serde(default = "default_strip_budget_kb")]
    pub(crate) strip_budget_kb: u32,
    /// Warn when the committed graph is this many commits behind HEAD.
    #[serde(default = "default_staleness_commits")]
    pub(crate) staleness_commits: u64,
    /// Warn when the committed graph is this many days behind HEAD.
    #[serde(default = "default_staleness_days")]
    pub(crate) staleness_days: i64,
}

fn default_source_url() -> String {
    DEFAULT_SOURCE_URL.to_string()
}

fn default_source_ref() -> String {
    "main".to_string()
}

fn default_source_cache() -> String {
    "target/commit-graph-source.git".to_string()
}

fn default_repo_web_url() -> String {
    DEFAULT_REPO_WEB_URL.to_string()
}

fn default_staleness_commits() -> u64 {
    20
}

fn default_staleness_days() -> i64 {
    7
}

fn default_strip_budget_kb() -> u32 {
    25
}

/// Presentation knobs for the four SVG variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StyleConfig {
    /// Colour of the staircase and the day bars. Absent keeps the sampled
    /// brand orange.
    #[serde(default)]
    pub(crate) accent: Option<String>,
    /// Title set at the top of each card.
    #[serde(default = "default_title")]
    pub(crate) title: String,
    /// Draw the milestone flags. With false the chart keeps the staircase.
    #[serde(default = "default_true")]
    pub(crate) show_flags: bool,
    /// Draw the day bars. With false the card keeps the staircase.
    #[serde(default = "default_true")]
    pub(crate) show_bars: bool,
    /// Face for the title and the large numerals: "Instrument Serif" or
    /// "JetBrains Mono". One line swaps every display numeral.
    #[serde(default = "default_display_font")]
    pub(crate) display_font: String,
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            accent: None,
            title: default_title(),
            show_flags: true,
            show_bars: true,
            display_font: default_display_font(),
        }
    }
}

fn default_title() -> String {
    "The Climb".to_string()
}

fn default_display_font() -> String {
    "Instrument Serif".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MilestoneConfig {
    pub(crate) date: String,
    pub(crate) label: String,
    /// Short form for the flag label. Absent keeps the full label.
    #[serde(default)]
    pub(crate) short: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CommitGraph {
    pub(crate) schema_version: u32,
    pub(crate) as_of_sha: String,
    pub(crate) as_of_date: String,
    pub(crate) timezone: String,
    pub(crate) range: GraphRange,
    pub(crate) totals: GraphTotals,
    pub(crate) per_day: Vec<DayEntry>,
    pub(crate) per_author: Vec<AuthorEntry>,
    pub(crate) milestones: Vec<GraphMilestone>,
    pub(crate) config_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GraphRange {
    pub(crate) start: String,
    pub(crate) end: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GraphTotals {
    /// Every commit reachable from `as_of_sha`, including graph refreshes.
    /// This equals `git rev-list --count <as_of_sha>`.
    pub(crate) commits: u64,
    /// Date of the earliest commit in the range.
    pub(crate) first_commit_date: String,
    /// Date of the as-of commit.
    pub(crate) last_commit_date: String,
    /// The day with the most commits; the earliest day wins a tie so the
    /// document is deterministic.
    pub(crate) busiest_day: BusiestDay,
    /// First and last day of the longest run of consecutive active days.
    pub(crate) longest_streak_range: StreakRange,
    /// The subset of `commits` whose every changed file is a graph artifact.
    pub(crate) refresh_commits: u64,
    pub(crate) active_days: u64,
    pub(crate) longest_streak: u64,
    pub(crate) current_streak: u64,
    pub(crate) lines_added: u64,
    pub(crate) lines_removed: u64,
    pub(crate) files_touched: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BusiestDay {
    pub(crate) date: String,
    pub(crate) commits: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StreakRange {
    pub(crate) start: String,
    pub(crate) end: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DayEntry {
    pub(crate) date: String,
    pub(crate) commits: u64,
    pub(crate) added: u64,
    pub(crate) removed: u64,
    pub(crate) authors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AuthorEntry {
    pub(crate) display: String,
    pub(crate) commits: u64,
}

/// Where a milestone sits relative to the as-of date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MilestoneStatus {
    Done,
    Upcoming,
}

impl MilestoneStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Upcoming => "upcoming",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GraphMilestone {
    pub(crate) date: String,
    pub(crate) label: String,
    /// Short flag label, when the configuration supplies one.
    #[serde(default)]
    pub(crate) short: String,
    /// Kept for readers of schema 2; always consistent with `status`.
    pub(crate) future: bool,
    /// Computed from the as-of date, so a flag cannot claim a state the data
    /// does not support.
    pub(crate) status: MilestoneStatus,
}

/// The `[commit_graph]` table of the README configuration file.
#[derive(Deserialize)]
struct ReadmeConfig {
    commit_graph: Option<CommitGraphConfig>,
}

/// Read and validate the commit-graph configuration.
pub(crate) fn load_config(path: &Path) -> Result<CommitGraphConfig> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed: ReadmeConfig = toml::from_str(&text)
        .with_context(|| format!("{} is not valid TOML configuration", path.display()))?;
    let config = parsed
        .commit_graph
        .with_context(|| format!("{} carries no [commit_graph] table", path.display()))?;
    validate_config(&config)?;
    Ok(config)
}

fn validate_config(config: &CommitGraphConfig) -> Result<()> {
    tz_offset_minutes(&config.timezone)?;
    if config.budget_kb == 0 {
        bail!("commit_graph.budget_kb must be greater than zero");
    }
    if config.strip_budget_kb == 0 {
        bail!("commit_graph.strip_budget_kb must be greater than zero");
    }
    if let Some(accent) = &config.style.accent {
        if accent.len() != 7
            || !accent.starts_with('#')
            || !accent[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            bail!("commit_graph.style.accent must be a six-digit hex colour, such as #C55E33");
        }
    }
    if config.style.title.trim().is_empty() {
        bail!("commit_graph.style.title must not be empty");
    }
    if config.style.display_font != "Instrument Serif"
        && config.style.display_font != "JetBrains Mono"
    {
        bail!(
            "commit_graph.style.display_font is `{}`; use `Instrument Serif` or `JetBrains Mono`",
            config.style.display_font
        );
    }
    if config.source_url.trim().is_empty() {
        bail!("commit_graph.source_url must name the public repository");
    }
    if config.source_ref.trim().is_empty()
        || config.source_ref.starts_with('-')
        || config.source_ref.contains(char::is_whitespace)
    {
        bail!("commit_graph.source_ref is not a usable ref name");
    }
    if config.source_cache.trim().is_empty() {
        bail!("commit_graph.source_cache must name the bare cache directory");
    }
    if config.repo_web_url.trim().is_empty() {
        bail!("commit_graph.repo_web_url must name the public repository's web page");
    }
    for milestone in &config.milestones {
        parse_iso_date(&milestone.date).with_context(|| {
            format!(
                "commit_graph milestone date `{}` is not an ISO date",
                milestone.date
            )
        })?;
        if milestone.label.trim().is_empty() {
            bail!(
                "commit_graph milestone on {} carries an empty label",
                milestone.date
            );
        }
        if let Some(short) = &milestone.short {
            if short.trim().is_empty() {
                bail!(
                    "commit_graph milestone on {} carries an empty short label",
                    milestone.date
                );
            }
        }
    }
    Ok(())
}

/// The offset of a fixed-zone timezone from UTC, in minutes.
///
/// Only fixed offsets are accepted: a zone with daylight saving would make the
/// day buckets depend on the tz database installed on the machine that runs the
/// generator, and therefore break byte-for-byte reproducibility.
pub(crate) fn tz_offset_minutes(name: &str) -> Result<i32> {
    match name {
        "UTC" | "Etc/UTC" | "GMT" => Ok(0),
        "Asia/Kolkata" | "Asia/Calcutta" => Ok(330),
        _ => {
            if let Some(rest) = name
                .strip_prefix("UTC+")
                .or_else(|| name.strip_prefix("UTC-"))
            {
                let sign = if name.starts_with("UTC-") { -1 } else { 1 };
                let (hours, minutes) = rest.split_once(':').unwrap_or((rest, "0"));
                let hours: i32 = hours
                    .parse()
                    .with_context(|| format!("unsupported timezone `{name}`"))?;
                let minutes: i32 = minutes
                    .parse()
                    .with_context(|| format!("unsupported timezone `{name}`"))?;
                if !(0..=23).contains(&hours) || !(0..=59).contains(&minutes) {
                    bail!("unsupported timezone `{name}`: offset out of range");
                }
                Ok(sign * (hours * 60 + minutes))
            } else {
                bail!("unsupported timezone `{name}`: use Asia/Kolkata or a fixed UTC+HH:MM offset")
            }
        }
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = (month + 9) % 12;
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Inverse of [`days_from_civil`].
pub(crate) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    (year + i64::from(month <= 2), month, day)
}

/// Parse `YYYY-MM-DD` into days since the epoch.
pub(crate) fn parse_iso_date(text: &str) -> Result<i64> {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        bail!("`{text}` is not an ISO date (YYYY-MM-DD)");
    }
    let year: i64 = text[0..4]
        .parse()
        .with_context(|| format!("`{text}` has a non-numeric year"))?;
    let month: i64 = text[5..7]
        .parse()
        .with_context(|| format!("`{text}` has a non-numeric month"))?;
    let day: i64 = text[8..10]
        .parse()
        .with_context(|| format!("`{text}` has a non-numeric day"))?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        bail!("`{text}` is not a valid calendar date");
    }
    let days = days_from_civil(year, month, day);
    // Reject dates such as 2026-02-31 by requiring the round trip to match.
    if civil_from_days(days) != (year, month, day) {
        bail!("`{text}` is not a valid calendar date");
    }
    Ok(days)
}

/// Format days since the epoch as `YYYY-MM-DD`.
pub(crate) fn format_date(days: i64) -> String {
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn local_day(timestamp: i64, offset_minutes: i32) -> i64 {
    let local = timestamp + i64::from(offset_minutes) * 60;
    local.div_euclid(86_400)
}

/// Run one git command in `root`, returning stdout on success.
fn run_git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .context("failed to run git")?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

/// True when the repository is a shallow checkout.
pub(crate) fn is_shallow(root: &Path) -> Result<bool> {
    let output = run_git(root, &["rev-parse", "--is-shallow-repository"])?;
    Ok(String::from_utf8_lossy(&output).trim() == "true")
}

/// Fail with an actionable message when the checkout lacks full history.
pub(crate) fn ensure_full_history(root: &Path) -> Result<()> {
    if is_shallow(root)? {
        bail!(
            "this checkout is a shallow clone, so its history is incomplete; \
             the commit graph cannot be built from it. Run `git fetch --unshallow` \
             (or clone without --depth) and try again."
        );
    }
    Ok(())
}

/// Where a graph's history is read from.
#[derive(Debug, Clone)]
pub(crate) struct History {
    /// Directory accepted by `git -C`: a checkout or a bare repository.
    pub(crate) path: PathBuf,
    /// True when the history came from the public cache rather than this
    /// checkout.
    pub(crate) public: bool,
}

/// The bare cache directory named by the configuration.
pub(crate) fn cache_path(root: &Path, config: &CommitGraphConfig) -> PathBuf {
    root.join(&config.source_cache)
}

/// Normalise a git remote URL enough to compare an `origin` with the configured
/// source. Handles the scp-like `git@host:path` form, a trailing slash and a
/// trailing `.git`, which are the differences a clone introduces.
pub(crate) fn normalize_remote(raw: &str) -> String {
    let mut text = raw.trim().to_string();
    if let Some(rest) = text.strip_prefix("git@") {
        text = format!("https://{}", rest.replacen(':', "/", 1));
    } else if let Some(rest) = text.strip_prefix("ssh://git@") {
        text = format!("https://{rest}");
    }
    while text.ends_with('/') {
        text.pop();
    }
    if let Some(stripped) = text.strip_suffix(".git") {
        text = stripped.to_string();
    }
    while text.ends_with('/') {
        text.pop();
    }
    text
}

/// True when this checkout's `origin` is the configured public repository, so
/// its own history is already the public history.
pub(crate) fn is_public_checkout(root: &Path, config: &CommitGraphConfig) -> Result<bool> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["remote", "get-url", "origin"])
        .output()
        .context("failed to run git")?;
    if !output.status.success() {
        return Ok(false);
    }
    let origin = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(!origin.is_empty() && normalize_remote(&origin) == normalize_remote(&config.source_url))
}

/// True when `path` contains the commit named by `sha`.
pub(crate) fn commit_exists(path: &Path, sha: &str) -> Result<bool> {
    if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(false);
    }
    let revision = format!("{sha}^{{commit}}");
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["cat-file", "-e", &revision])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .context("failed to run git")?;
    Ok(output.status.success())
}

/// True when `sha` is reachable from `HEAD` in `path`.
pub(crate) fn is_ancestor(path: &Path, sha: &str) -> Result<bool> {
    if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(false);
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["merge-base", "--is-ancestor", sha, "HEAD"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .context("failed to run git")?;
    Ok(output.status.success())
}

/// Run `git` in the bare cache without any credential prompt.
fn run_git_offline(path: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .output()
        .context("failed to run git")?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

/// Clone the public history into the bare cache on first use, then fetch it.
/// No credentials are requested and no prompt is ever shown.
fn ensure_cache(root: &Path, config: &CommitGraphConfig) -> Result<PathBuf> {
    let cache = cache_path(root, config);
    let bare = cache.exists()
        && Command::new("git")
            .arg("-C")
            .arg(&cache)
            .args(["rev-parse", "--is-bare-repository"])
            .output()
            .map(|output| {
                output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
            })
            .unwrap_or(false);
    if bare {
        if let Err(error) = run_git_offline(
            &cache,
            &[
                "fetch",
                "--quiet",
                "--prune",
                &config.source_url,
                "+refs/heads/*:refs/heads/*",
                "+refs/tags/*:refs/tags/*",
            ],
        ) {
            bail!(
                "cannot refresh the public history from {}: {error}\n\
                 The committed commit graph must describe the public repository, never this \
                 checkout's private log.\n\
                 If the network is unavailable, run `cargo xtask commit-graph --source local` to \
                 build a graph from this checkout for inspection only. An offline graph describes \
                 this local history and must not be committed.",
                config.source_url
            );
        }
        return Ok(cache);
    }
    if cache.exists() {
        bail!(
            "{} exists but is not a bare git repository; remove it and run \
             `cargo xtask commit-graph --source public` again",
            cache.display()
        );
    }
    if let Some(parent) = cache.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let output = Command::new("git")
        .args(["clone", "--bare", "--quiet", &config.source_url])
        .arg(&cache)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .output()
        .context("failed to run git clone")?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "cannot read the public history from {}: {detail}\n\
             The committed commit graph must describe the public repository, never this \
             checkout's private log.\n\
             If the network is unavailable, run `cargo xtask commit-graph --source local` to \
             build a graph from this checkout for inspection only. An offline graph describes \
             this local history and must not be committed.",
            config.source_url
        );
    }
    Ok(cache)
}

/// Resolve the history a graph should be read from.
pub(crate) fn acquire_history(
    root: &Path,
    config: &CommitGraphConfig,
    source: HistorySource,
) -> Result<History> {
    match source {
        HistorySource::Local => Ok(History {
            path: root.to_path_buf(),
            public: is_public_checkout(root, config).unwrap_or(false),
        }),
        HistorySource::Public => Ok(History {
            path: ensure_cache(root, config)?,
            public: true,
        }),
        HistorySource::Auto => {
            if is_public_checkout(root, config)? {
                Ok(History {
                    path: root.to_path_buf(),
                    public: true,
                })
            } else {
                Ok(History {
                    path: ensure_cache(root, config)?,
                    public: true,
                })
            }
        }
    }
}

/// The public commit permalink the README should carry.
pub(crate) fn commit_permalink(config: &CommitGraphConfig, sha: &str) -> String {
    format!("{}/commit/{sha}", config.repo_web_url.trim_end_matches('/'))
}

/// Point the README's commit-graph image link at the public as-of commit.
///
/// Only the `href` of the first anchor inside the marker-bracketed block is
/// replaced; every other byte of the README is preserved. Returns the rewritten
/// text, or `None` when the README has no such block or no anchor.
pub(crate) fn rewrite_readme_permalink(text: &str, permalink: &str) -> Option<String> {
    const STRIP_BEGIN: &str = "<!-- commit-graph-strip:begin -->";
    const STRIP_END: &str = "<!-- commit-graph-strip:end -->";
    const BEGIN: &str = "<!-- commit-graph:begin -->";
    const END: &str = "<!-- commit-graph:end -->";
    let mut rewritten = text.to_string();
    let mut changed = false;
    for (begin_marker, end_marker) in [(STRIP_BEGIN, STRIP_END), (BEGIN, END)] {
        if let Some(next) = rewrite_block_permalink(&rewritten, begin_marker, end_marker, permalink)
        {
            rewritten = next;
            changed = true;
        }
    }
    changed.then_some(rewritten)
}

/// Replace the first `href` inside one marker-bracketed block.
fn rewrite_block_permalink(
    text: &str,
    begin_marker: &str,
    end_marker: &str,
    permalink: &str,
) -> Option<String> {
    let begin = text.find(begin_marker)? + begin_marker.len();
    let end = begin + text[begin..].find(end_marker)?;
    let body = &text[begin..end];
    let href = body.find("href=\"")? + "href=\"".len();
    let close = href + body[href..].find('"')?;
    let mut rewritten = String::with_capacity(text.len());
    let href = begin + href;
    let close = begin + close;
    rewritten.push_str(&text[..href]);
    rewritten.push_str(permalink);
    rewritten.push_str(&text[close..]);
    Some(rewritten)
}

/// Update the README permalink in place when the block is present.
fn update_readme_permalink(root: &Path, config: &CommitGraphConfig, sha: &str) -> Result<()> {
    let path = root.join("README.md");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let permalink = commit_permalink(config, sha);
    if let Some(rewritten) = rewrite_readme_permalink(&text, &permalink) {
        if rewritten != text {
            std::fs::write(&path, rewritten)
                .with_context(|| format!("failed to write {}", path.display()))?;
        }
    }
    Ok(())
}

fn resolve_commit(root: &Path, spec: &str) -> Result<String> {
    let spec = spec.trim();
    if spec.is_empty()
        || spec.starts_with('-')
        || spec.len() > 200
        || spec.contains(char::is_whitespace)
    {
        bail!("`{spec}` is not a usable commit or ref name");
    }
    let revision = format!("{spec}^{{commit}}");
    let output = run_git(root, &["rev-parse", "--verify", &revision])
        .with_context(|| format!("cannot resolve `{spec}` to a commit"))?;
    let sha = String::from_utf8_lossy(&output).trim().to_string();
    if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("git resolved `{spec}` to an unexpected object name `{sha}`");
    }
    Ok(sha)
}

fn commit_timestamp(root: &Path, sha: &str) -> Result<i64> {
    let output = run_git(root, &["show", "-s", "--format=%at", sha])?;
    let text = String::from_utf8_lossy(&output);
    text.trim()
        .parse()
        .with_context(|| format!("commit {sha} carries no numeric author timestamp"))
}

struct RawCommit {
    ts: i64,
    author: String,
    added: u64,
    removed: u64,
    files: Vec<String>,
}

/// Read the pinned log format: one record per commit, fields separated by unit
/// separators, followed by the `--numstat` block.
fn read_commits(root: &Path, sha: &str) -> Result<Vec<RawCommit>> {
    let output = run_git(
        root,
        &[
            "log",
            sha,
            "--no-color",
            "--numstat",
            "--no-renames",
            "--format=%x1e%H%x1f%at%x1f%aN",
        ],
    )?;
    let text = String::from_utf8_lossy(&output);
    let mut commits = Vec::new();
    for record in text.split('\u{1e}') {
        let record = record.trim_start_matches('\n');
        if record.trim().is_empty() {
            continue;
        }
        let mut lines = record.lines();
        let header = lines.next().unwrap_or_default();
        let fields: Vec<&str> = header.split('\u{1f}').collect();
        if fields.len() < 3 {
            bail!("unexpected git log record: `{header}`");
        }
        let ts: i64 = fields[1]
            .parse()
            .with_context(|| format!("commit {} carries no numeric timestamp", fields[0]))?;
        let mut added = 0u64;
        let mut removed = 0u64;
        let mut files = Vec::new();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let mut parts = line.splitn(3, '\t');
            let added_field = parts.next().unwrap_or_default();
            let removed_field = parts.next().unwrap_or_default();
            let Some(path) = parts.next() else {
                continue;
            };
            // Binary files report `-` for both counts but still count as a
            // touched file.
            added = added.saturating_add(added_field.parse().unwrap_or(0));
            removed = removed.saturating_add(removed_field.parse().unwrap_or(0));
            files.push(path.to_string());
        }
        commits.push(RawCommit {
            ts,
            author: fields[2].trim().to_string(),
            added,
            removed,
            files,
        });
    }
    Ok(commits)
}

/// Match a repository-relative path against a small glob: `*` matches any run
/// of characters except `/`, `**` matches across separators, `?` matches one
/// character.
pub(crate) fn glob_match(pattern: &str, path: &str) -> bool {
    fn helper(pattern: &[u8], path: &[u8]) -> bool {
        if pattern.is_empty() {
            return path.is_empty();
        }
        match pattern[0] {
            b'*' => {
                if pattern.get(1) == Some(&b'*') {
                    let rest = &pattern[2..];
                    (0..=path.len()).any(|i| helper(rest, &path[i..]))
                } else {
                    let rest = &pattern[1..];
                    (0..=path.len())
                        .take_while(|i| *i == 0 || path.get(i - 1) != Some(&b'/'))
                        .any(|i| helper(rest, &path[i..]))
                }
            }
            b'?' => !path.is_empty() && path[0] != b'/' && helper(&pattern[1..], &path[1..]),
            byte => !path.is_empty() && path[0] == byte && helper(&pattern[1..], &path[1..]),
        }
    }
    helper(pattern.as_bytes(), path.as_bytes())
}

/// True when every path a commit touched is a graph artifact, so the commit is
/// a media-only graph refresh. Such commits are counted like any other and
/// reported separately instead of being hidden.
fn is_refresh_commit(config: &CommitGraphConfig, files: &[String]) -> bool {
    !files.is_empty()
        && files.iter().all(|path| {
            config
                .refresh_commits_whose_files_all_match
                .iter()
                .any(|pattern| glob_match(pattern, path))
        })
}

#[derive(Default)]
struct DayAccumulator {
    commits: u64,
    added: u64,
    removed: u64,
    authors: BTreeSet<String>,
}

/// Build the graph document and its serialised JSON form.
///
/// Every commit reachable from `as_of` is counted: the totals are what the
/// public repository shows at that commit, so nothing (a graph refresh, a bot
/// commit or an excluded display name) can make them disagree with
/// `git rev-list --count`. The author breakdown is the only place a name is
/// hidden, and that never removes a commit from the totals or the per-day
/// counts.
pub(crate) fn generate(
    root: &Path,
    config: &CommitGraphConfig,
    as_of: Option<&str>,
) -> Result<(CommitGraph, String)> {
    let offset = tz_offset_minutes(&config.timezone)?;
    let sha = resolve_commit(root, as_of.unwrap_or("HEAD"))?;
    let as_of_day = local_day(commit_timestamp(root, &sha)?, offset);

    let mut days: BTreeMap<i64, DayAccumulator> = BTreeMap::new();
    let mut authors: BTreeMap<String, u64> = BTreeMap::new();
    let mut commits = 0u64;
    let mut refresh_commits = 0u64;
    let mut lines_added = 0u64;
    let mut lines_removed = 0u64;
    let mut files_touched = 0u64;
    let mut first_day: Option<i64> = None;

    for commit in read_commits(root, &sha)? {
        let day = local_day(commit.ts, offset);
        let hidden_author = config
            .exclude_authors
            .iter()
            .any(|excluded| excluded == &commit.author);
        let display = config
            .display_names
            .get(&commit.author)
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| {
                let trimmed = commit.author.trim();
                if trimmed.is_empty() {
                    "unknown".to_string()
                } else {
                    trimmed.to_string()
                }
            });

        let entry = days.entry(day).or_default();
        entry.commits += 1;
        entry.added = entry.added.saturating_add(commit.added);
        entry.removed = entry.removed.saturating_add(commit.removed);
        if !hidden_author {
            entry.authors.insert(display.clone());
            *authors.entry(display).or_insert(0) += 1;
        }
        commits += 1;
        if is_refresh_commit(config, &commit.files) {
            refresh_commits += 1;
        }
        lines_added = lines_added.saturating_add(commit.added);
        lines_removed = lines_removed.saturating_add(commit.removed);
        files_touched = files_touched.saturating_add(commit.files.len() as u64);
        first_day = Some(first_day.map_or(day, |current| current.min(day)));
    }

    let start = first_day.with_context(|| format!("no commits to graph at {sha}"))?;
    if start > as_of_day {
        bail!(
            "the graph start date ({}) is after the as-of date ({})",
            format_date(start),
            format_date(as_of_day)
        );
    }

    let streak = streak_stats(&days, as_of_day);
    let longest_streak = streak.longest;
    let current_streak = streak.current;
    let per_day: Vec<DayEntry> = (start..=as_of_day)
        .map(|day| {
            let accumulator = days.remove(&day).unwrap_or_default();
            DayEntry {
                date: format_date(day),
                commits: accumulator.commits,
                added: accumulator.added,
                removed: accumulator.removed,
                authors: accumulator.authors.into_iter().collect(),
            }
        })
        .collect();
    let busiest = per_day
        .iter()
        .fold(None::<&DayEntry>, |best, day| match best {
            Some(current) if current.commits >= day.commits => Some(current),
            _ => Some(day),
        })
        .map(|day| BusiestDay {
            date: day.date.clone(),
            commits: day.commits,
        })
        .unwrap_or(BusiestDay {
            date: format_date(as_of_day),
            commits: 0,
        });
    let mut per_author: Vec<AuthorEntry> = authors
        .into_iter()
        .map(|(display, commits)| AuthorEntry { display, commits })
        .collect();
    per_author.sort_by(|left, right| {
        right
            .commits
            .cmp(&left.commits)
            .then_with(|| left.display.cmp(&right.display))
    });

    let milestones = config
        .milestones
        .iter()
        .map(|milestone| {
            let day = parse_iso_date(&milestone.date)?;
            let status = if day > as_of_day {
                MilestoneStatus::Upcoming
            } else {
                MilestoneStatus::Done
            };
            Ok(GraphMilestone {
                date: milestone.date.clone(),
                label: milestone.label.clone(),
                short: milestone
                    .short
                    .clone()
                    .unwrap_or_else(|| milestone.label.clone()),
                future: status == MilestoneStatus::Upcoming,
                status,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let graph = CommitGraph {
        schema_version: SCHEMA_VERSION,
        as_of_sha: sha,
        as_of_date: format_date(as_of_day),
        timezone: config.timezone.clone(),
        range: GraphRange {
            start: format_date(start),
            end: format_date(as_of_day),
        },
        totals: GraphTotals {
            commits,
            first_commit_date: format_date(start),
            last_commit_date: format_date(as_of_day),
            busiest_day: busiest,
            longest_streak_range: StreakRange {
                start: format_date(streak.start),
                end: format_date(streak.end),
            },
            refresh_commits,
            active_days: per_day.iter().filter(|day| day.commits > 0).count() as u64,
            longest_streak,
            current_streak,
            lines_added,
            lines_removed,
            files_touched,
        },
        per_day,
        per_author,
        milestones,
        config_hash: config_hash(config)?,
    };
    let json = to_json(&graph)?;
    Ok((graph, json))
}

/// Longest and current streak, plus the exact day range of the longest run.
struct StreakStats {
    longest: u64,
    current: u64,
    start: i64,
    end: i64,
}

fn streak_stats(days: &BTreeMap<i64, DayAccumulator>, as_of_day: i64) -> StreakStats {
    let active: Vec<i64> = days
        .iter()
        .filter(|(_, accumulator)| accumulator.commits > 0)
        .map(|(day, _)| *day)
        .collect();
    let mut longest = 0u64;
    let mut start = as_of_day;
    let mut end = as_of_day;
    let mut run = 0u64;
    let mut run_start = as_of_day;
    let mut previous: Option<i64> = None;
    for day in &active {
        run = match previous {
            Some(previous) if *day == previous + 1 => run + 1,
            _ => {
                run_start = *day;
                1
            }
        };
        if run > longest {
            longest = run;
            start = run_start;
            end = *day;
        }
        previous = Some(*day);
    }
    let current = match active.last() {
        Some(last) if *last == as_of_day => run,
        _ => 0,
    };
    StreakStats {
        longest,
        current,
        start,
        end,
    }
}

/// Stable hash of the configuration, recorded in the JSON so a document can be
/// tied to the rules that produced it.
pub(crate) fn config_hash(config: &CommitGraphConfig) -> Result<String> {
    let bytes =
        serde_json::to_vec(config).context("failed to serialise the commit-graph config")?;
    let digest = Sha256::digest(&bytes);
    Ok(format!("{digest:x}"))
}

/// Serialise the document with a fixed key order and one trailing newline.
pub(crate) fn to_json(graph: &CommitGraph) -> Result<String> {
    let mut text =
        serde_json::to_string_pretty(graph).context("failed to serialise the commit graph")?;
    text.push('\n');
    Ok(text)
}

fn size_label(bytes: usize) -> String {
    format!("{:.1} kB", bytes as f64 / 1024.0)
}

/// Generate the JSON and both cards, then write them.
pub(crate) fn cmd_commit_graph(args: CommitGraphArgs) -> Result<()> {
    if args.check {
        return cmd_commit_graph_check(&args);
    }
    let config_path = args.root.join(&args.config);
    let config = load_config(&config_path)?;
    let history = acquire_history(&args.root, &config, args.source)?;
    ensure_full_history(&history.path)?;
    let default_as_of = if history.public && history.path != args.root {
        config.source_ref.as_str()
    } else {
        "HEAD"
    };
    let as_of = args.as_of.as_deref().unwrap_or(default_as_of);
    let (graph, json) = generate(&history.path, &config, Some(as_of))?;
    let palette = svg::Palette::load(&args.root, &config.style)?;
    let full = svg::render_full(&graph, &palette, &config.style)?;
    let narrow = svg::render_narrow(&graph, &palette, &config.style)?;
    let compact = svg::render_compact(&graph, &palette, &config.style)?;
    let compact_narrow = svg::render_compact_narrow(&graph, &palette, &config.style)?;

    let card_budget = (config.budget_kb as usize * 1024).min(CARD_BUDGET_BYTES);
    let strip_budget = (config.strip_budget_kb as usize * 1024).min(STRIP_BUDGET_BYTES);
    if full.len() > card_budget
        || narrow.len() > card_budget
        || compact.len() > strip_budget
        || compact_narrow.len() > strip_budget
    {
        bail!(
            "commit-graph artifacts exceed the self-imposed size budget: \
             cards {}/{} B of {} B, strips {}/{} B of {} B; \
             reduce the number of distinct glyphs before reducing detail",
            full.len(),
            narrow.len(),
            card_budget,
            compact.len(),
            compact_narrow.len(),
            strip_budget
        );
    }

    let out_dir = args.root.join(&args.out_dir);
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;
    let json_path = out_dir.join("commit-graph.json");
    let full_path = out_dir.join("commit-graph.svg");
    let narrow_path = out_dir.join("commit-graph-narrow.svg");
    let compact_path = out_dir.join("commit-graph-compact.svg");
    let compact_narrow_path = out_dir.join("commit-graph-compact-narrow.svg");
    std::fs::write(&json_path, &json)
        .with_context(|| format!("failed to write {}", json_path.display()))?;
    std::fs::write(&full_path, &full)
        .with_context(|| format!("failed to write {}", full_path.display()))?;
    std::fs::write(&narrow_path, &narrow)
        .with_context(|| format!("failed to write {}", narrow_path.display()))?;
    std::fs::write(&compact_path, &compact)
        .with_context(|| format!("failed to write {}", compact_path.display()))?;
    std::fs::write(&compact_narrow_path, &compact_narrow)
        .with_context(|| format!("failed to write {}", compact_narrow_path.display()))?;
    update_readme_permalink(&args.root, &config, &graph.as_of_sha)?;

    let totals = &graph.totals;
    println!(
        "commit graph: {} commits by {} author(s) across {} active day(s), {}..{} ({})",
        totals.commits,
        graph.per_author.len(),
        totals.active_days,
        graph.range.start,
        graph.range.end,
        graph.timezone
    );
    if history.public {
        if history.path == args.root {
            println!(
                "  history source: this checkout (origin is {})",
                config.source_url
            );
        } else {
            println!(
                "  history source: public {} (ref {}, cache {})",
                config.source_url, config.source_ref, config.source_cache
            );
        }
    } else {
        println!(
            "  history source: LOCAL CHECKOUT (origin is not {}); this graph must not be committed",
            config.source_url
        );
    }
    println!("  public as of {} ({})", graph.as_of_sha, graph.as_of_date);
    println!(
        "  of which {} graph refresh commit(s)",
        totals.refresh_commits
    );
    println!(
        "  lines +{} / -{}, {} files touched, longest streak {} day(s), current {} day(s)",
        totals.lines_added,
        totals.lines_removed,
        totals.files_touched,
        totals.longest_streak,
        totals.current_streak
    );
    println!(
        "  wrote {} ({})",
        json_path.display(),
        size_label(json.len())
    );
    println!(
        "  wrote {} ({}, budget {} kB)",
        full_path.display(),
        size_label(full.len()),
        config.budget_kb
    );
    println!(
        "  wrote {} ({}, budget {} kB)",
        narrow_path.display(),
        size_label(narrow.len()),
        config.budget_kb
    );
    println!(
        "  wrote {} ({}, budget {} kB)",
        compact_path.display(),
        size_label(compact.len()),
        config.strip_budget_kb
    );
    println!(
        "  wrote {} ({}, budget {} kB)",
        compact_narrow_path.display(),
        size_label(compact_narrow.len()),
        config.strip_budget_kb
    );
    println!(
        "  README permalink: {}",
        commit_permalink(&config, &graph.as_of_sha)
    );
    Ok(())
}

/// Verify the committed artifacts without writing anything.
///
/// Consistency violations (a document that does not reproduce, a card that
/// disagrees, a broken shape or budget rule) are a hard failure. Staleness
/// relative to `HEAD` is reported as a warning and does not change the exit
/// status: a graph one commit behind is the normal state immediately after a
/// commit, so a hook must not block a push for it.
pub(crate) fn cmd_commit_graph_check(args: &CommitGraphArgs) -> Result<()> {
    let out_dir = args.root.join(&args.out_dir);
    let outcome = check::check_with_paths(
        &args.root,
        &out_dir.join("commit-graph.json"),
        &out_dir.join("commit-graph.svg"),
        &out_dir.join("commit-graph-narrow.svg"),
        &out_dir.join("commit-graph-compact.svg"),
        &out_dir.join("commit-graph-compact-narrow.svg"),
    )?;
    for warning in &outcome.warnings {
        eprintln!("warning: {warning}");
    }
    if let Some(note) = &outcome.note {
        eprintln!("note: {note}");
    }
    if outcome.violations.is_empty() {
        println!(
            "commit-graph --check: the committed graph is consistent with its as-of commit{}",
            if outcome.warnings.is_empty() {
                ""
            } else {
                " (stale relative to HEAD; run `cargo xtask commit-graph` to refresh)"
            }
        );
        return Ok(());
    }
    for violation in &outcome.violations {
        eprintln!("{violation}");
    }
    bail!(
        "commit-graph --check: {} inconsistent artifact(s)",
        outcome.violations.len()
    )
}

#[cfg(test)]
mod tests;
