//! Live-tab data: loading the recorded DNS capture and projecting it into the
//! replay the Live tab draws.
//!
//! Loading never panics: a missing or unparseable capture becomes a
//! [`LiveLoadState`] variant the UI renders instead of crashing. The
//! projections — the step list, the two headline lanes and the bar layout —
//! are pure functions of the parsed [`DnsCapture`], so they are unit-testable
//! without a terminal or a file. Every number comes from the artifact; nothing
//! here is synthesized.

use crate::data::read_artifact;

use std::path::{Path, PathBuf};

use climb_report::{DnsCapture, HEADLINE_QNAME, HEADLINE_QTYPE};

/// Default path for a capture written by a live `cargo xtask dns-demo --record`
/// run.
pub const LIVE_CAPTURE_PATH: &str = "target/dns-capture.json";
/// Default path for the committed recorded fixture.
pub const RECORDED_CAPTURE_PATH: &str = "docs/sample-output/dns-capture.recorded.json";
/// The same fixture as published by the website and fetched by the browser.
pub const SITE_CAPTURE_PATH: &str = "/data/dns-capture.recorded.json";

/// How many characters of the recording's git commit the header shows.
const COMMIT_SHORT_LEN: usize = 12;

/// Which source the loaded capture came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveSource {
    /// A live `cargo xtask dns-demo --record` run wrote this file.
    LiveRun,
    /// The committed `docs/sample-output/dns-capture.recorded.json` fixture.
    RecordedFixture,
    /// Fetched from the site origin (`/data/dns-capture.recorded.json`).
    Site,
    /// The fixture compiled into the binary.
    Embedded,
}

impl LiveSource {
    /// Lowercase tag used in provenance labels.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            LiveSource::LiveRun => "live run",
            LiveSource::RecordedFixture => "recorded fixture",
            LiveSource::Site => "site",
            LiveSource::Embedded => "embedded",
        }
    }
}

/// How the recorded DNS capture looks at load time.
#[derive(Debug, Clone, PartialEq)]
pub enum LiveLoadState {
    /// The capture parsed; the view holds the projected replay.
    Loaded(Box<LiveView>),
    /// No capture exists at any of the candidate paths.
    Missing { path: PathBuf },
    /// The file exists but could not be read or parsed.
    Invalid { path: PathBuf, error: String },
}

impl LiveLoadState {
    /// The parsed capture view, when one loaded.
    #[must_use]
    pub fn view(&self) -> Option<&LiveView> {
        match self {
            LiveLoadState::Loaded(view) => Some(view),
            _ => None,
        }
    }

    /// How many replay steps the loaded capture has; 0 when not loaded.
    #[must_use]
    pub fn step_count(&self) -> usize {
        self.view().map_or(0, |view| view.steps.len())
    }

    /// Short one-line summary for the trace log.
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            LiveLoadState::Loaded(view) => {
                format!("loaded: {} steps ({:?})", view.steps.len(), view.source)
            }
            LiveLoadState::Missing { path } => format!("missing: {}", crate::paths::display(path)),
            LiveLoadState::Invalid { path, .. } => {
                format!("invalid: {}", crate::paths::display(path))
            }
        }
    }
}

/// Load the recorded DNS capture from a specific path, never panicking.
pub fn load_live(path: &Path) -> LiveLoadState {
    load_live_from(path, LiveSource::LiveRun)
}

/// Load the capture from a specific path with a given source label.
fn load_live_from(path: &Path, source: LiveSource) -> LiveLoadState {
    if std::fs::metadata(path).is_err() {
        return LiveLoadState::Missing {
            path: path.to_path_buf(),
        };
    }
    let text = match read_artifact(path) {
        Ok(text) => text,
        Err(error) => {
            return LiveLoadState::Invalid {
                path: path.to_path_buf(),
                error,
            };
        }
    };
    match serde_json::from_str::<DnsCapture>(&text) {
        Ok(capture) => LiveLoadState::Loaded(Box::new(LiveView::new(capture, source))),
        Err(error) => LiveLoadState::Invalid {
            path: path.to_path_buf(),
            error: error.to_string(),
        },
    }
}

/// Load the DNS capture, trying the standard sources in order:
/// 1. `target/dns-capture.json` (live run)
/// 2. `docs/sample-output/dns-capture.recorded.json` (committed fixture)
/// 3. the compiled-in copy of the same fixture
///
/// The first one that exists wins. A present-but-unparseable capture is a hard
/// [`LiveLoadState::Invalid`] naming its path, never silently skipped in
/// favour of the next candidate; only a genuinely absent file falls through.
pub fn load_live_auto() -> LiveLoadState {
    load_live_auto_with(
        Path::new(LIVE_CAPTURE_PATH),
        Path::new(RECORDED_CAPTURE_PATH),
        Some(crate::data::embedded::DNS_CAPTURE),
    )
}

/// [`load_live_auto`] with explicit candidate paths, so the precedence is
/// unit-testable without touching the working directory.
pub fn load_live_auto_from(live: &Path, recorded: &Path) -> LiveLoadState {
    load_live_auto_with(live, recorded, Some(crate::data::embedded::DNS_CAPTURE))
}

/// [`load_live_auto_from`] with the compiled-in fixture made optional, so a
/// test can still prove the "nothing anywhere" state.
pub fn load_live_auto_with(
    live: &Path,
    recorded: &Path,
    embedded_bytes: Option<&[u8]>,
) -> LiveLoadState {
    match load_live_from(live, LiveSource::LiveRun) {
        LiveLoadState::Missing { .. } => {}
        other => return other,
    }
    match load_live_from(recorded, LiveSource::RecordedFixture) {
        LiveLoadState::Missing { .. } => {}
        other => return other,
    }
    match embedded_bytes {
        Some(bytes) => load_live_embedded_bytes(bytes),
        None => LiveLoadState::Missing {
            path: live.to_path_buf(),
        },
    }
}

/// Parse the compiled-in capture without consulting the filesystem.
///
/// The browser build has no artifact files; it replays the same recording the
/// binary carries, labelled as embedded.
#[must_use]
pub fn load_live_embedded() -> LiveLoadState {
    load_live_embedded_bytes(crate::data::embedded::DNS_CAPTURE)
}

/// Parse a DNS capture fetched from the site origin.
///
/// The capture goes through the same [`DnsCapture`] parser and
/// [`LiveView`] projection as every other source.
#[must_use]
pub fn load_live_site(bytes: &[u8]) -> LiveLoadState {
    load_live_bytes(bytes, PathBuf::from(SITE_CAPTURE_PATH), LiveSource::Site)
}

/// Parse a compiled-in DNS capture.
fn load_live_embedded_bytes(bytes: &[u8]) -> LiveLoadState {
    load_live_bytes(
        bytes,
        PathBuf::from(format!(
            "<embedded {}>",
            crate::data::embedded::DNS_CAPTURE_NAME
        )),
        LiveSource::Embedded,
    )
}

fn load_live_bytes(bytes: &[u8], path: PathBuf, source: LiveSource) -> LiveLoadState {
    match serde_json::from_slice::<DnsCapture>(bytes) {
        Ok(capture) => LiveLoadState::Loaded(Box::new(LiveView::new(capture, source))),
        Err(error) => LiveLoadState::Invalid {
            path,
            error: error.to_string(),
        },
    }
}

/// Which way one recorded packet leg travelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// The client's outbound question; the capture records responses only, so
    /// this leg carries no observed size.
    Query,
    /// The server's inbound answer.
    Answer,
}

/// One step of the replay: a single packet leg of one recorded query.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveStep {
    /// The capture pass this packet belongs to (`full`, `condensed`, …).
    pub pass: String,
    /// The question this packet exchanged, as `qname qtype`.
    pub question: String,
    /// Travelling direction of this leg.
    pub direction: Direction,
    /// Transport the capture observed for the exchange.
    pub transport: String,
    /// Human label: `query`, `answer (full)`, `answer (condensed)`, …
    pub label: String,
    /// Observed answer size in bytes; `None` on the query leg, which the
    /// capture does not record.
    pub bytes: Option<u64>,
    /// Truncation flag as observed (or `None` when not observed).
    pub tc: Option<bool>,
    /// Authenticated-data flag as observed (or `None` when not observed).
    pub ad: Option<bool>,
    /// Response code as observed.
    pub rcode: Option<u8>,
    /// Whether this packet belongs to the headline question.
    pub headline: bool,
    /// Recorded bytes up to and including this step, across the whole capture.
    pub running_bytes: u64,
}

/// A render-ready row of one headline lane.
#[derive(Debug, Clone, PartialEq)]
pub struct LaneStep {
    /// Index into [`LiveView::steps`], tying the row to the replay cursor.
    pub index: usize,
    pub label: String,
    pub direction: Direction,
    pub bytes: Option<u64>,
    pub tc: Option<bool>,
    pub ad: Option<bool>,
    /// Recorded bytes up to and including this row, within this lane.
    pub running_bytes: u64,
    /// Whether the replay cursor has reached this row.
    pub revealed: bool,
}

/// One of the two side-by-side lanes: one capture pass of the headline query.
#[derive(Debug, Clone, PartialEq)]
pub struct Lane {
    /// Panel title (`without MTL · full pass` / `with MTL · condensed pass`).
    pub title: &'static str,
    /// Whether this pass exists in the capture at all.
    pub present: bool,
    /// The headline query's steps for this pass, in replay order.
    pub steps: Vec<LaneStep>,
    /// Sum of every recorded answer byte in this lane.
    pub total_bytes: u64,
}

/// The Live tab's headline projection: the same question exchanged twice.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadlineLanes {
    /// The headline question, e.g. `www.climb.example. A`.
    pub question: String,
    /// The exchange without MTL amortization (full signatures).
    pub without_mtl: Lane,
    /// The exchange with MTL (condensed proofs).
    pub with_mtl: Lane,
    /// Largest step size across both lanes; the shared bar-scale maximum.
    pub max_bytes: u64,
    /// Byte reduction of the with-MTL lane against the without-MTL lane, when
    /// both passes are present.
    pub reduction_percent: Option<f64>,
}

/// A parsed capture with its derived replay projections.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveView {
    capture: DnsCapture,
    source: LiveSource,
    steps: Vec<LiveStep>,
}

impl LiveView {
    /// Project a parsed capture into its replay steps.
    pub fn new(capture: DnsCapture, source: LiveSource) -> Self {
        let steps = steps_for(&capture);
        Self {
            capture,
            source,
            steps,
        }
    }

    /// The parsed capture document.
    #[must_use]
    pub fn capture(&self) -> &DnsCapture {
        &self.capture
    }

    /// Where this capture was loaded from.
    #[must_use]
    pub fn source(&self) -> &LiveSource {
        &self.source
    }

    /// Source label for display (`live run` / `recorded fixture`).
    #[must_use]
    pub fn source_label(&self) -> &'static str {
        match self.source {
            LiveSource::LiveRun => "live run",
            LiveSource::RecordedFixture | LiveSource::Embedded => "recorded fixture",
            LiveSource::Site => "site recording",
        }
    }

    /// The replay step list: one query leg and one answer leg per recorded
    /// packet, in capture order.
    #[must_use]
    pub fn steps(&self) -> &[LiveStep] {
        &self.steps
    }

    /// The leading `YYYY-MM-DD` of the recording timestamp.
    #[must_use]
    pub fn recorded_date(&self) -> &str {
        self.capture
            .recorded_at
            .get(..10)
            .unwrap_or(&self.capture.recorded_at)
    }

    /// The commit the recording was made at, when one was recorded.
    #[must_use]
    pub fn commit(&self) -> Option<&str> {
        self.capture.git_commit.as_deref()
    }

    /// The tab's provenance header: `RECORDED CAPTURE <date> · commit <sha>`,
    /// with an em dash where the capture carries no commit (`-` in ASCII mode).
    #[must_use]
    pub fn header_line(&self, ascii: bool) -> String {
        let sep = if ascii { "-" } else { "\u{b7}" };
        let dash = if ascii { "-" } else { "\u{2014}" };
        let commit = match self.commit() {
            Some(commit) => commit.chars().take(COMMIT_SHORT_LEN).collect::<String>(),
            None => dash.to_string(),
        };
        format!(
            "RECORDED CAPTURE {} {sep} commit {commit}",
            self.recorded_date()
        )
    }

    /// The headline question string, e.g. `www.climb.example. A`.
    #[must_use]
    pub fn question(&self) -> String {
        format!("{HEADLINE_QNAME} {HEADLINE_QTYPE}")
    }

    /// The two headline lanes with the replay cursor at `cursor`: rows at or
    /// before the cursor are revealed, later rows are pending.
    #[must_use]
    pub fn lanes_at(&self, cursor: usize) -> HeadlineLanes {
        let without_mtl = self.lane("full", "without MTL \u{b7} full pass", cursor);
        let with_mtl = self.lane("condensed", "with MTL \u{b7} condensed pass", cursor);
        let max_bytes = without_mtl
            .steps
            .iter()
            .chain(with_mtl.steps.iter())
            .filter_map(|step| step.bytes)
            .max()
            .unwrap_or(0);
        let reduction_percent =
            if without_mtl.present && with_mtl.present && without_mtl.total_bytes > 0 {
                Some(
                    (without_mtl.total_bytes as f64 - with_mtl.total_bytes as f64)
                        / without_mtl.total_bytes as f64
                        * 100.0,
                )
            } else {
                None
            };
        HeadlineLanes {
            question: self.question(),
            without_mtl,
            with_mtl,
            max_bytes,
            reduction_percent,
        }
    }

    /// The headline lanes with every row revealed ([`Self::lanes_at`] with the
    /// cursor past the end).
    #[must_use]
    pub fn lanes(&self) -> HeadlineLanes {
        self.lanes_at(usize::MAX)
    }

    /// One headline lane for one capture pass.
    fn lane(&self, pass: &str, title: &'static str, cursor: usize) -> Lane {
        let mut steps = Vec::new();
        let mut total: u64 = 0;
        for (index, step) in self.steps.iter().enumerate() {
            if !step.headline || !step.pass.eq_ignore_ascii_case(pass) {
                continue;
            }
            if let Some(bytes) = step.bytes {
                total = total.saturating_add(bytes);
            }
            steps.push(LaneStep {
                index,
                label: step.label.clone(),
                direction: step.direction,
                bytes: step.bytes,
                tc: step.tc,
                ad: step.ad,
                running_bytes: total,
                revealed: index <= cursor,
            });
        }
        Lane {
            title,
            present: !steps.is_empty(),
            steps,
            total_bytes: total,
        }
    }
}

/// The replay step list for a capture: a query leg (no recorded size) and an
/// answer leg per recorded packet, plus a running byte counter across the
/// whole capture.
fn steps_for(capture: &DnsCapture) -> Vec<LiveStep> {
    let mut steps = Vec::with_capacity(capture.queries.len() * 2);
    let mut running: u64 = 0;
    for query in &capture.queries {
        let headline = query.qname.eq_ignore_ascii_case(HEADLINE_QNAME)
            && query.qtype.eq_ignore_ascii_case(HEADLINE_QTYPE);
        let question = format!("{} {}", query.qname, query.qtype);
        let common = |direction, label: String, bytes, running| LiveStep {
            pass: query.label.clone(),
            question: question.clone(),
            direction,
            transport: query.transport.clone(),
            label,
            bytes,
            tc: query.flags.tc,
            ad: query.flags.ad,
            rcode: query.flags.rcode,
            headline,
            running_bytes: running,
        };
        steps.push(common(Direction::Query, "query".to_string(), None, running));
        running = running.saturating_add(query.response_bytes);
        steps.push(common(
            Direction::Answer,
            answer_label(&query.label),
            Some(query.response_bytes),
            running,
        ));
    }
    steps
}

/// The label for one answer leg: the capture pass decides (`full` → the full
/// signature, `condensed` → the MTL condensed proof, `unbound` → the
/// recursive resolver's validated answer); an unknown pass keeps its own name.
fn answer_label(pass: &str) -> String {
    match pass.to_ascii_lowercase().as_str() {
        "full" => "answer (full)".to_string(),
        "condensed" => "answer (condensed)".to_string(),
        "unbound" => "answer (resolver)".to_string(),
        other => format!("answer ({other})"),
    }
}

/// Bar length in cells for one step, on a linear or base-10 log scale.
///
/// Linear maps `bytes / max_bytes` onto the width; log maps
/// `log10(bytes) / log10(max)` over a 1-byte floor, so a small answer stays
/// visible beside a large one. Steps with no recorded size (`None`, or a zero
/// response) draw no bar, a positive step always draws at least one cell, and
/// the result never exceeds `width`.
#[must_use]
pub fn bar_cells(bytes: Option<u64>, max_bytes: u64, width: u16, log_scale: bool) -> u16 {
    let Some(bytes) = bytes else {
        return 0;
    };
    if bytes == 0 || width == 0 || max_bytes == 0 {
        return 0;
    }
    let frac = if log_scale && max_bytes > 1 {
        (bytes as f64).log10() / (max_bytes as f64).log10()
    } else {
        bytes as f64 / max_bytes as f64
    };
    let cells = (frac.clamp(0.0, 1.0) * f64::from(width)).round() as u16;
    cells.clamp(1, width)
}

/// The flag column for one step: `TC` and `ad` for set flags, a middle dot
/// for an observed-but-clear flag, and an em dash when the capture observed
/// neither.
#[must_use]
pub fn flag_text(tc: Option<bool>, ad: Option<bool>, ascii: bool) -> String {
    let dot = if ascii { "." } else { "\u{b7}" };
    let dash = if ascii { "-" } else { "\u{2014}" };
    let mut marks = Vec::new();
    if let Some(set) = tc {
        marks.push(if set { "TC" } else { dot });
    }
    if let Some(set) = ad {
        marks.push(if set { "ad" } else { dot });
    }
    if marks.is_empty() {
        dash.to_string()
    } else {
        marks.join(" ")
    }
}

#[cfg(test)]
mod tests;
