//! Commit-graph generation: read the repository's history and write one JSON
//! document plus two self-contained SVG cards.
//!
//! The graph is a pure function of the history reachable from one commit and
//! the `[commit_graph]` configuration in `docs/readme-config.toml`. Nothing in
//! the output depends on the wall clock, the machine or the git version's
//! display defaults: dates are bucketed in a fixed-offset zone, the log format
//! is pinned, and the JSON is serialised with a fixed key order.

mod check;
mod svg;
mod text;

use anyhow::{bail, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) use check::check_docs;

/// Version of the JSON document shape.
pub(crate) const SCHEMA_VERSION: u32 = 1;

/// Configuration file, relative to the workspace root.
pub(crate) const CONFIG_PATH: &str = "docs/readme-config.toml";
/// Full card, relative to the workspace root.
pub(crate) const JSON_PATH: &str = "docs/media/commit-graph.json";
pub(crate) const SVG_PATH: &str = "docs/media/commit-graph.svg";
pub(crate) const COMPACT_PATH: &str = "docs/media/commit-graph-compact.svg";

#[derive(Args)]
pub(crate) struct CommitGraphArgs {
    /// Workspace root whose git history is read.
    #[arg(long, default_value = ".")]
    pub(crate) root: PathBuf,

    /// Commit (or ref) to plot the graph up to. Defaults to HEAD.
    #[arg(long)]
    pub(crate) as_of: Option<String>,

    /// Configuration file, relative to the root.
    #[arg(long, default_value = CONFIG_PATH)]
    pub(crate) config: PathBuf,

    /// Directory that receives the JSON and both SVG cards.
    #[arg(long, default_value = "docs/media")]
    pub(crate) out_dir: PathBuf,

    /// Verify the committed artifacts instead of regenerating them.
    ///
    /// Exits non-zero only when the graph is inconsistent (it does not
    /// reproduce from its own as-of commit, a card disagrees with the
    /// document, or a shape/budget rule fails). A graph that is merely behind
    /// `HEAD` prints a warning and still exits zero, so a pre-push hook can
    /// flag a stale graph without blocking the push.
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
    /// Optional lower bound, as an ISO date.
    #[serde(default)]
    pub(crate) since: Option<String>,
    /// Git author names excluded from the graph.
    #[serde(default)]
    pub(crate) exclude_authors: Vec<String>,
    /// Git author name -> display name.
    #[serde(default)]
    pub(crate) display_names: BTreeMap<String, String>,
    /// Patterns; a commit whose every changed path matches one of them is
    /// ignored.
    #[serde(default)]
    pub(crate) ignore_commits_whose_files_all_match: Vec<String>,
    /// Milestones drawn on the timeline.
    #[serde(default)]
    pub(crate) milestones: Vec<MilestoneConfig>,
    /// Per-file size ceiling for both SVG cards.
    pub(crate) budget_kb: u32,
    /// Warn when the committed graph is this many commits behind HEAD.
    #[serde(default = "default_staleness_commits")]
    pub(crate) staleness_commits: u64,
    /// Warn when the committed graph is this many days behind HEAD.
    #[serde(default = "default_staleness_days")]
    pub(crate) staleness_days: i64,
}

fn default_staleness_commits() -> u64 {
    20
}

fn default_staleness_days() -> i64 {
    7
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MilestoneConfig {
    pub(crate) date: String,
    pub(crate) label: String,
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
    pub(crate) commits: u64,
    pub(crate) active_days: u64,
    pub(crate) longest_streak: u64,
    pub(crate) current_streak: u64,
    pub(crate) lines_added: u64,
    pub(crate) lines_removed: u64,
    pub(crate) files_touched: u64,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GraphMilestone {
    pub(crate) date: String,
    pub(crate) label: String,
    pub(crate) future: bool,
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
    if let Some(since) = &config.since {
        parse_iso_date(since)
            .with_context(|| format!("commit_graph.since `{since}` is not an ISO date"))?;
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

fn ignorable_by_paths(config: &CommitGraphConfig, files: &[String]) -> bool {
    !files.is_empty()
        && files.iter().all(|path| {
            config
                .ignore_commits_whose_files_all_match
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
pub(crate) fn generate(
    root: &Path,
    config: &CommitGraphConfig,
    as_of: Option<&str>,
) -> Result<(CommitGraph, String)> {
    let offset = tz_offset_minutes(&config.timezone)?;
    let sha = resolve_commit(root, as_of.unwrap_or("HEAD"))?;
    let as_of_day = local_day(commit_timestamp(root, &sha)?, offset);
    let since = match &config.since {
        Some(date) => Some(parse_iso_date(date)?),
        None => None,
    };
    if let Some(start) = since {
        if start > as_of_day {
            bail!(
                "commit_graph.since ({}) is after the as-of date ({})",
                format_date(start),
                format_date(as_of_day)
            );
        }
    }

    let mut days: BTreeMap<i64, DayAccumulator> = BTreeMap::new();
    let mut authors: BTreeMap<String, u64> = BTreeMap::new();
    let mut commits = 0u64;
    let mut lines_added = 0u64;
    let mut lines_removed = 0u64;
    let mut files_touched = 0u64;
    let mut first_day: Option<i64> = None;

    for commit in read_commits(root, &sha)? {
        let day = local_day(commit.ts, offset);
        if since.is_some_and(|start| day < start) {
            continue;
        }
        if config
            .exclude_authors
            .iter()
            .any(|excluded| excluded == &commit.author)
        {
            continue;
        }
        if ignorable_by_paths(config, &commit.files) {
            continue;
        }
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
        entry.authors.insert(display.clone());
        *authors.entry(display).or_insert(0) += 1;
        commits += 1;
        lines_added = lines_added.saturating_add(commit.added);
        lines_removed = lines_removed.saturating_add(commit.removed);
        files_touched = files_touched.saturating_add(commit.files.len() as u64);
        first_day = Some(first_day.map_or(day, |current| current.min(day)));
    }

    let start = since
        .or(first_day)
        .with_context(|| format!("no commits to graph at {sha}"))?;
    if start > as_of_day {
        bail!(
            "the graph start date ({}) is after the as-of date ({})",
            format_date(start),
            format_date(as_of_day)
        );
    }

    let (longest_streak, current_streak) = streak_lengths(&days, as_of_day);
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
            Ok(GraphMilestone {
                date: milestone.date.clone(),
                label: milestone.label.clone(),
                future: day > as_of_day,
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

fn streak_lengths(days: &BTreeMap<i64, DayAccumulator>, as_of_day: i64) -> (u64, u64) {
    let active: Vec<i64> = days
        .iter()
        .filter(|(_, accumulator)| accumulator.commits > 0)
        .map(|(day, _)| *day)
        .collect();
    let mut longest = 0u64;
    let mut run = 0u64;
    let mut previous: Option<i64> = None;
    for day in &active {
        run = match previous {
            Some(previous) if *day == previous + 1 => run + 1,
            _ => 1,
        };
        longest = longest.max(run);
        previous = Some(*day);
    }
    let current = match active.last() {
        Some(last) if *last == as_of_day => run,
        _ => 0,
    };
    (longest, current)
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
    ensure_full_history(&args.root)?;
    if args.check {
        return cmd_commit_graph_check(&args);
    }
    let config_path = args.root.join(&args.config);
    let config = load_config(&config_path)?;
    let as_of = args.as_of.as_deref().unwrap_or("HEAD");
    let (graph, json) = generate(&args.root, &config, Some(as_of))?;
    let palette = svg::Palette::load(&args.root)?;
    let full = svg::render_full(&graph, &palette)?;
    let compact = svg::render_compact(&graph, &palette)?;

    let budget = config.budget_kb as usize * 1024;
    if full.len() > budget || compact.len() > budget {
        bail!(
            "commit-graph cards exceed the {} kB budget: full {}, compact {}; \
             reduce the number of distinct glyphs before reducing detail",
            config.budget_kb,
            size_label(full.len()),
            size_label(compact.len())
        );
    }

    let out_dir = args.root.join(&args.out_dir);
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;
    let json_path = out_dir.join("commit-graph.json");
    let full_path = out_dir.join("commit-graph.svg");
    let compact_path = out_dir.join("commit-graph-compact.svg");
    std::fs::write(&json_path, &json)
        .with_context(|| format!("failed to write {}", json_path.display()))?;
    std::fs::write(&full_path, &full)
        .with_context(|| format!("failed to write {}", full_path.display()))?;
    std::fs::write(&compact_path, &compact)
        .with_context(|| format!("failed to write {}", compact_path.display()))?;

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
    println!(
        "  as of {} ({})",
        &graph.as_of_sha[..7.min(graph.as_of_sha.len())],
        graph.as_of_date
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
        compact_path.display(),
        size_label(compact.len()),
        config.budget_kb
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
        &out_dir.join("commit-graph-compact.svg"),
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
