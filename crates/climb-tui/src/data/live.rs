//! Live-tab data: loading the recorded DNS capture and projecting it into the
//! replay the Live tab draws.
//!
//! Loading never panics: a missing or unparseable capture becomes a
//! [`LiveLoadState`] variant the UI renders instead of crashing. The
//! projections — the step list, the two headline lanes and the bar layout —
//! are pure functions of the parsed [`DnsCapture`], so they are unit-testable
//! without a terminal or a file. Every number comes from the artifact; nothing
//! here is synthesized.

use std::path::{Path, PathBuf};

use climb_report::{DnsCapture, HEADLINE_QNAME, HEADLINE_QTYPE};

/// Default path for a capture written by a live `cargo xtask dns-demo --record`
/// run.
pub const LIVE_CAPTURE_PATH: &str = "target/dns-capture.json";
/// Default path for the committed recorded fixture.
pub const RECORDED_CAPTURE_PATH: &str = "docs/sample-output/dns-capture.recorded.json";

/// How many characters of the recording's git commit the header shows.
const COMMIT_SHORT_LEN: usize = 12;

/// Which source the loaded capture came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveSource {
    /// A live `cargo xtask dns-demo --record` run wrote this file.
    LiveRun,
    /// The committed `docs/sample-output/dns-capture.recorded.json` fixture.
    RecordedFixture,
    /// The fixture compiled into the binary.
    Embedded,
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
            LiveLoadState::Missing { path } => format!("missing: {}", path.display()),
            LiveLoadState::Invalid { path, .. } => format!("invalid: {}", path.display()),
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
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            return LiveLoadState::Invalid {
                path: path.to_path_buf(),
                error: error.to_string(),
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
        Some(bytes) => match serde_json::from_slice::<DnsCapture>(bytes) {
            Ok(capture) => {
                LiveLoadState::Loaded(Box::new(LiveView::new(capture, LiveSource::Embedded)))
            }
            Err(error) => LiveLoadState::Invalid {
                path: PathBuf::from(format!(
                    "<embedded {}>",
                    crate::data::embedded::DNS_CAPTURE_NAME
                )),
                error: error.to_string(),
            },
        },
        None => LiveLoadState::Missing {
            path: live.to_path_buf(),
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
mod tests {
    use super::*;
    use climb_report::{DnsCaptureFlags, DnsCaptureHost, DnsCaptureQuery};

    /// Test-data builders: a minimal capture with the same shape the recorder
    /// writes. The byte counts here are fixtures for the projection tests.
    fn query(pass: &str, qname: &str, qtype: &str, bytes: u64) -> DnsCaptureQuery {
        DnsCaptureQuery {
            label: pass.to_string(),
            qname: qname.to_string(),
            qtype: qtype.to_string(),
            response_bytes: bytes,
            answer_rrsig_bytes: None,
            flags: DnsCaptureFlags {
                ad: None,
                tc: None,
                rcode: Some(0),
            },
            transport: "tcp".to_string(),
            edns_udp_size: None,
            elapsed_ms: None,
            wire_hex: None,
        }
    }

    fn capture(queries: Vec<DnsCaptureQuery>, git_commit: Option<&str>) -> DnsCapture {
        DnsCapture {
            schema_version: 1,
            recorded_at: "2026-10-12T14:24:26+05:30".to_string(),
            git_commit: git_commit.map(str::to_string),
            host: DnsCaptureHost {
                hostname: "test-host".to_string(),
                uname: "Linux test".to_string(),
            },
            images: Vec::new(),
            algorithm: "SLH-DSA-SHA2-128s-MTL-SHA2-128".to_string(),
            zone: "climb.example.".to_string(),
            queries,
        }
    }

    fn sample_capture() -> DnsCapture {
        capture(
            vec![
                query("full", "www.climb.example.", "A", 24_438),
                query("condensed", "www.climb.example.", "A", 658),
                query("full", "climb.example.", "SOA", 24_465),
            ],
            None,
        )
    }

    fn view() -> LiveView {
        LiveView::new(sample_capture(), LiveSource::RecordedFixture)
    }

    #[test]
    fn step_list_has_a_query_and_an_answer_leg_per_recorded_packet() {
        let view = view();
        assert_eq!(
            view.steps().len(),
            6,
            "three recorded packets become six legs"
        );
        assert_eq!(view.steps()[0].direction, Direction::Query);
        assert_eq!(view.steps()[0].label, "query");
        assert_eq!(
            view.steps()[0].bytes,
            None,
            "the query size is not recorded"
        );
        assert_eq!(view.steps()[1].direction, Direction::Answer);
        assert_eq!(view.steps()[1].bytes, Some(24_438));
        assert_eq!(view.steps()[1].transport, "tcp");
    }

    #[test]
    fn answer_leg_labels_name_the_capture_pass() {
        let capture = capture(
            vec![
                query("full", "climb.example.", "SOA", 100),
                query("condensed", "climb.example.", "SOA", 20),
                query("unbound", "climb.example.", "SOA", 60),
                query("weird", "climb.example.", "SOA", 1),
            ],
            None,
        );
        let view = LiveView::new(capture, LiveSource::LiveRun);
        let labels: Vec<&str> = view
            .steps()
            .iter()
            .filter(|step| step.direction == Direction::Answer)
            .map(|step| step.label.as_str())
            .collect();
        assert_eq!(
            labels,
            vec![
                "answer (full)",
                "answer (condensed)",
                "answer (resolver)",
                "answer (weird)"
            ]
        );
    }

    #[test]
    fn steps_carry_a_running_byte_counter_across_the_capture() {
        let view = view();
        let running: Vec<u64> = view.steps().iter().map(|step| step.running_bytes).collect();
        // Each query leg shows the total before its packet, each answer leg
        // after it: 24,438 + 658 + 24,465 (the SOA record).
        assert_eq!(running, vec![0, 24_438, 24_438, 25_096, 25_096, 49_561]);
    }

    #[test]
    fn steps_mark_only_the_headline_question() {
        let view = view();
        let headline: Vec<bool> = view.steps().iter().map(|step| step.headline).collect();
        assert_eq!(headline, vec![true, true, true, true, false, false]);
        assert_eq!(view.steps()[4].question, "climb.example. SOA");
        assert_eq!(view.steps()[0].question, "www.climb.example. A");
    }

    #[test]
    fn headline_lanes_split_the_passes_with_lane_totals() {
        let lanes = view().lanes();
        assert_eq!(lanes.question, "www.climb.example. A");
        assert!(lanes.without_mtl.present);
        assert!(lanes.with_mtl.present);
        assert_eq!(lanes.without_mtl.total_bytes, 24_438);
        assert_eq!(lanes.with_mtl.total_bytes, 658);
        assert_eq!(lanes.max_bytes, 24_438);
        let reduction = lanes.reduction_percent.expect("both lanes present");
        assert!(
            (reduction - 97.307_37).abs() < 1e-3,
            "reduction={reduction}"
        );
    }

    #[test]
    fn lane_running_counters_accumulate_within_the_lane() {
        let lanes = view().lanes();
        let without: Vec<u64> = lanes
            .without_mtl
            .steps
            .iter()
            .map(|step| step.running_bytes)
            .collect();
        assert_eq!(without, vec![0, 24_438]);
        let with_mtl: Vec<u64> = lanes
            .with_mtl
            .steps
            .iter()
            .map(|step| step.running_bytes)
            .collect();
        assert_eq!(with_mtl, vec![0, 658]);
    }

    #[test]
    fn lanes_at_marks_rows_revealed_as_the_cursor_reaches_them() {
        let view = view();
        // The headline rows of the sample sit at indices 0/1 (full) and 2/3
        // (condensed).
        let early = view.lanes_at(0);
        assert!(early.without_mtl.steps[0].revealed, "cursor on the row");
        assert!(!early.without_mtl.steps[1].revealed, "row still pending");
        assert!(
            !early.with_mtl.steps[1].revealed,
            "the other lane is pending too"
        );
        let all = view.lanes();
        assert!(all.with_mtl.steps.iter().all(|step| step.revealed));
    }

    #[test]
    fn lanes_are_absent_when_the_capture_has_no_headline_rows() {
        let capture = capture(vec![query("full", "climb.example.", "SOA", 100)], None);
        let lanes = LiveView::new(capture, LiveSource::LiveRun).lanes();
        assert!(!lanes.without_mtl.present);
        assert!(!lanes.with_mtl.present);
        assert_eq!(lanes.max_bytes, 0);
        assert_eq!(lanes.reduction_percent, None);
    }

    #[test]
    fn bar_cells_is_linear_by_default() {
        // Test data: the sample's headline pair.
        assert_eq!(bar_cells(Some(24_438), 24_438, 40, false), 40);
        assert_eq!(bar_cells(Some(12_219), 24_438, 40, false), 20);
        assert_eq!(bar_cells(Some(658), 24_438, 40, false), 1, "rounds, min 1");
    }

    #[test]
    fn bar_cells_in_log_mode_lifts_small_steps() {
        // Test data: the sample's headline pair. Log scale over a 1-byte floor
        // makes 658 B of 24,438 B more than half the column.
        let log = bar_cells(Some(658), 24_438, 40, true);
        let linear = bar_cells(Some(658), 24_438, 40, false);
        assert!(log > 20, "log={log}");
        assert!(log > linear, "log={log} linear={linear}");
        assert_eq!(bar_cells(Some(24_438), 24_438, 40, true), 40, "max fills");
    }

    #[test]
    fn bar_cells_handles_missing_bytes_zero_width_and_never_overflows() {
        assert_eq!(bar_cells(None, 24_438, 40, false), 0, "no bar for no size");
        assert_eq!(
            bar_cells(Some(0), 24_438, 40, false),
            0,
            "zero draws nothing"
        );
        assert_eq!(bar_cells(Some(658), 24_438, 0, false), 0);
        assert_eq!(bar_cells(Some(658), 0, 40, false), 0, "no scale, no bar");
        assert_eq!(bar_cells(Some(99_999), 24_438, 40, false), 40, "clamps");
        assert_eq!(bar_cells(Some(1), 24_438, 40, true), 1, "log floor");
    }

    #[test]
    fn header_line_names_the_date_and_commit() {
        let mut capture = sample_capture();
        capture.git_commit = Some("312c68268c96878b0ee01922d94c447cc9d0287e".to_string());
        let header = LiveView::new(capture, LiveSource::LiveRun).header_line(false);
        assert_eq!(
            header, "RECORDED CAPTURE 2026-10-12 \u{b7} commit 312c68268c96",
            "the date is the timestamp's day, the commit is shortened"
        );
    }

    #[test]
    fn header_line_marks_an_absent_commit() {
        assert_eq!(
            view().header_line(false),
            "RECORDED CAPTURE 2026-10-12 \u{b7} commit \u{2014}"
        );
        assert_eq!(
            view().header_line(true),
            "RECORDED CAPTURE 2026-10-12 - commit -",
            "ascii mode stays ascii"
        );
    }

    #[test]
    fn flag_text_reports_observed_and_set_flags() {
        assert_eq!(flag_text(None, None, false), "\u{2014}", "unobserved");
        assert_eq!(flag_text(Some(true), None, false), "TC");
        assert_eq!(flag_text(None, Some(true), false), "ad");
        assert_eq!(flag_text(Some(false), Some(true), false), "\u{b7} ad");
        assert_eq!(flag_text(None, None, true), "-", "ascii dash");
        assert_eq!(flag_text(Some(false), None, true), ".", "ascii dot");
    }

    // ── loading ─────────────────────────────────────────────────────

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "climb-tui-live-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn sample_json() -> String {
        serde_json::to_string(&sample_capture()).expect("serialize")
    }

    #[test]
    fn load_of_missing_capture_is_missing_state() {
        let path = std::env::temp_dir().join("climb-tui-definitely-missing-capture.json");
        let _ = std::fs::remove_file(&path);
        let state = load_live(&path);
        assert!(matches!(state, LiveLoadState::Missing { .. }));
        assert_eq!(state.step_count(), 0);
        assert!(state.view().is_none());
    }

    #[test]
    fn load_of_garbage_capture_is_invalid_and_never_panics() {
        let root = temp_root("invalid");
        let path = root.join("dns-capture.json");
        std::fs::write(&path, "{\"queries\": not json").expect("write garbage");
        let state = load_live(&path);
        match &state {
            LiveLoadState::Invalid { error, .. } => assert!(!error.is_empty()),
            other => panic!("expected Invalid, got {other:?}"),
        }
        assert_eq!(state.step_count(), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn load_live_auto_prefers_target_over_the_sample_fixture() {
        let root = temp_root("prefer-live");
        let live = root.join("target/dns-capture.json");
        let recorded = root.join("sample/dns-capture.recorded.json");
        std::fs::create_dir_all(live.parent().unwrap()).expect("live parent");
        std::fs::create_dir_all(recorded.parent().unwrap()).expect("recorded parent");
        std::fs::write(&live, sample_json()).expect("write live");
        std::fs::write(&recorded, sample_json()).expect("write recorded");

        let state = load_live_auto_from(&live, &recorded);
        assert_eq!(
            state.view().map(LiveView::source),
            Some(&LiveSource::LiveRun)
        );
        assert_eq!(state.step_count(), 6);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn load_live_auto_falls_back_to_the_sample_fixture() {
        let root = temp_root("fallback");
        let live = root.join("target/dns-capture.json");
        let recorded = root.join("sample/dns-capture.recorded.json");
        std::fs::create_dir_all(recorded.parent().unwrap()).expect("recorded parent");
        std::fs::write(&recorded, sample_json()).expect("write recorded");

        let state = load_live_auto_from(&live, &recorded);
        assert_eq!(
            state.view().map(LiveView::source),
            Some(&LiveSource::RecordedFixture)
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn load_live_auto_names_the_live_path_when_nothing_exists_and_no_embedded_copy_is_offered() {
        let root = temp_root("nowhere");
        let live = root.join("target/dns-capture.json");
        let state =
            load_live_auto_with(&live, &root.join("sample/dns-capture.recorded.json"), None);
        match state {
            LiveLoadState::Missing { path } => {
                assert_eq!(path, live, "the remedy is a new recording at the live path")
            }
            other => panic!("expected Missing naming the live path, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn load_live_auto_falls_back_to_the_embedded_capture() {
        let root = temp_root("embedded");
        let state = load_live_auto_from(
            &root.join("target/dns-capture.json"),
            &root.join("sample/dns-capture.recorded.json"),
        );
        assert_eq!(
            state.view().map(LiveView::source),
            Some(&LiveSource::Embedded)
        );
        assert!(state.step_count() > 0);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn load_live_auto_treats_a_present_but_invalid_live_capture_as_an_error() {
        let root = temp_root("invalid-live");
        let live = root.join("target/dns-capture.json");
        let recorded = root.join("sample/dns-capture.recorded.json");
        std::fs::create_dir_all(live.parent().unwrap()).expect("live parent");
        std::fs::create_dir_all(recorded.parent().unwrap()).expect("recorded parent");
        // The live capture is corrupt while the fixture is valid: the loader
        // must surface the live error, not silently fall back.
        std::fs::write(&live, "{ not json").expect("write garbage");
        std::fs::write(&recorded, sample_json()).expect("write recorded");

        let state = load_live_auto_from(&live, &recorded);
        match state {
            LiveLoadState::Invalid { path, .. } => assert_eq!(path, live),
            other => panic!("expected Invalid naming the live path, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The committed sample capture must load through this loader and expose
    /// the same step and lane numbers the artifact itself records (§7c: the
    /// dashboard is one of the artifact's consumers).
    #[test]
    fn committed_sample_capture_exposes_steps_and_lanes_to_the_tui() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/sample-output/dns-capture.recorded.json");
        // No live capture at the first candidate, so the loader falls through
        // to the committed fixture and labels it as such.
        let absent = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("no-such-directory/dns-capture.json");
        let state = load_live_auto_from(&absent, &path);
        let LiveLoadState::Loaded(view) = state else {
            panic!("expected the committed sample capture to load, got {state:?}");
        };
        assert_eq!(view.source(), &LiveSource::RecordedFixture);

        // Read the expected headline pair straight from the parsed artifact.
        let find = |pass: &str| {
            view.capture()
                .queries
                .iter()
                .find(|q| {
                    q.label == pass
                        && q.qname.eq_ignore_ascii_case(HEADLINE_QNAME)
                        && q.qtype.eq_ignore_ascii_case(HEADLINE_QTYPE)
                })
                .map(|q| q.response_bytes)
        };
        let full = find("full").expect("the sample carries a full pass");
        let condensed = find("condensed").expect("the sample carries a condensed pass");

        assert_eq!(view.steps().len(), view.capture().queries.len() * 2);
        let lanes = view.lanes();
        assert_eq!(lanes.without_mtl.total_bytes, full);
        assert_eq!(lanes.with_mtl.total_bytes, condensed);
        assert_eq!(view.recorded_date(), &view.capture().recorded_at[..10]);
    }
}
