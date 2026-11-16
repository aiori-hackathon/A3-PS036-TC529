//! The demo's script: what each pane runs, and the numbers it quotes.
//!
//! Every number comes from `docs/sample-output/climb-results.json` through the
//! shared headline definitions in `climb-report`; nothing here is typed in.
//! The same five rows feed the cheat sheet and the closing summary, so the
//! pane the presenter leaves on screen and the line they end on cannot
//! disagree.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use climb_report::numbers::{headline_metrics, Metric};
use climb_report::ClimbResults;

use super::preflight::{SAMPLE_REPORT, SAMPLE_RESULTS, SITE_DIST};

/// IDs of the five numbers the cheat sheet shows, in reading order.
pub(crate) const CHEAT_SHEET_IDS: [&str; 5] = [
    "signing_speedup_medium_c_vs_a",
    "verifying_speedup_medium_cached_c_vs_a",
    "wire_reduction_medium_c_vs_a",
    "dns_reduction",
    "udp_fit_c",
];

/// The three the presenter ends on.
pub(crate) const REMEMBER_IDS: [&str; 3] = [
    "signing_speedup_medium_c_vs_a",
    "verifying_speedup_medium_cached_c_vs_a",
    "wire_reduction_medium_c_vs_a",
];

/// One verification command and the argument list it runs with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifyStep {
    pub(crate) name: &'static str,
    pub(crate) args: Vec<String>,
}

/// Everything the run needs, decided before the first pane starts.
#[derive(Debug, Clone)]
pub(crate) struct DemoPlan {
    pub(crate) root: PathBuf,
    pub(crate) results: PathBuf,
    /// The viewer reads a staged copy so its report-health badge can see the
    /// report beside the results document; the cheat sheet keeps reading the
    /// committed artifact.
    pub(crate) staged_results: PathBuf,
    pub(crate) tui: PathBuf,
    pub(crate) verification: Vec<VerifyStep>,
    pub(crate) server_root: PathBuf,
    pub(crate) server_url: String,
    pub(crate) port: u16,
    pub(crate) cheat_sheet: Vec<Metric>,
    pub(crate) rtt_pair: Option<(Metric, Metric)>,
    pub(crate) offline: bool,
}

impl DemoPlan {
    /// The tour keys the TUI pane prints above the dashboard.
    pub(crate) fn tour_keys(&self) -> &'static str {
        "t start tour · n next · p pause · Esc leave tour · 1-9 tabs · T theme · ? help · q quit"
    }
}

/// Load the committed artifact the demo reads.
pub(crate) fn load_results(path: &Path) -> Result<ClimbResults> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
}

fn metric_by_id(metrics: &[Metric], id: &str) -> Result<Metric> {
    metrics
        .iter()
        .find(|metric| metric.id == id)
        .cloned()
        .with_context(|| {
            format!(
                "the artifact carries no `{id}` headline row; regenerate it with `cargo xtask consolidate`"
            )
        })
}

/// Copy the committed pair into `target/demo/` so the viewer can see a report
/// beside its results document. The copy is refreshed on every run, with the
/// report copied second so it is never older than the results it describes.
pub(crate) fn stage_viewer(demo: &DemoPlan) -> Result<()> {
    let dir = demo
        .staged_results
        .parent()
        .context("the staged results path has no parent directory")?;
    std::fs::create_dir_all(dir.join("climb-report"))
        .with_context(|| format!("creating {}", dir.display()))?;
    std::fs::copy(&demo.results, &demo.staged_results).with_context(|| {
        format!(
            "copying {} to {}",
            demo.results.display(),
            demo.staged_results.display()
        )
    })?;
    let report = demo.root.join(SAMPLE_REPORT);
    if report.is_file() {
        std::fs::copy(&report, dir.join("climb-report/report.html"))
            .with_context(|| format!("copying {} to {}", report.display(), dir.display()))?;
    }
    Ok(())
}

/// The five cheat-sheet rows, read from the artifact's headline definitions.
pub(crate) fn cheat_sheet(results: &ClimbResults) -> Result<Vec<Metric>> {
    let metrics = headline_metrics(results);
    CHEAT_SHEET_IDS
        .iter()
        .map(|id| metric_by_id(&metrics, id))
        .collect()
}

/// The lowest emulated RTT at which both authoritative-server modes were
/// measured, as a plain/condensed pair.
pub(crate) fn rtt_pair(results: &ClimbResults) -> Result<Option<(Metric, Metric)>> {
    let metrics = headline_metrics(results);
    let mut delays: Vec<u64> = metrics
        .iter()
        .filter_map(|metric| {
            metric
                .id
                .strip_prefix("rtt_nsd_plain_")
                .and_then(|rest| rest.strip_suffix("ms"))
                .and_then(|delay| delay.parse().ok())
        })
        .collect();
    delays.sort_unstable();
    delays.dedup();
    for delay in delays {
        let plain = metrics
            .iter()
            .find(|metric| metric.id == format!("rtt_nsd_plain_{delay}ms"));
        let condensed = metrics
            .iter()
            .find(|metric| metric.id == format!("rtt_nsd_mtl-condensed_{delay}ms"));
        if let (Some(plain), Some(condensed)) = (plain, condensed) {
            return Ok(Some((plain.clone(), condensed.clone())));
        }
    }
    Ok(None)
}

/// The three closing rows, read from the same definitions as the cheat sheet.
pub(crate) fn remember(results: &ClimbResults) -> Result<Vec<Metric>> {
    let metrics = headline_metrics(results);
    REMEMBER_IDS
        .iter()
        .map(|id| metric_by_id(&metrics, id))
        .collect()
}

/// Build the plan, failing before any pane starts when the artifact cannot
/// supply the numbers the demo promises.
pub(crate) fn build(root: &Path, tui: &Path, port: u16, offline: bool) -> Result<DemoPlan> {
    let results_path = root.join(SAMPLE_RESULTS);
    let results = load_results(&results_path)?;
    Ok(DemoPlan {
        root: root.to_path_buf(),
        results: results_path,
        staged_results: root.join("target/demo/climb-results.json"),
        tui: tui.to_path_buf(),
        verification: vec![
            VerifyStep {
                name: "sample-check",
                args: vec!["sample-check".to_string()],
            },
            VerifyStep {
                name: "verify-numbers",
                args: vec!["verify-numbers".to_string()],
            },
            VerifyStep {
                name: "report --check",
                args: vec![
                    "report".to_string(),
                    "--format".to_string(),
                    "html".to_string(),
                    "--check".to_string(),
                    "--results".to_string(),
                    SAMPLE_RESULTS.to_string(),
                    "--out".to_string(),
                    SAMPLE_REPORT.to_string(),
                ],
            },
        ],
        server_root: root.join(SITE_DIST),
        server_url: format!("http://127.0.0.1:{port}"),
        port,
        cheat_sheet: cheat_sheet(&results)?,
        rtt_pair: rtt_pair(&results)?,
        offline,
    })
}

fn value_cell(metric: &Metric) -> String {
    metric.value_cell()
}

/// Print the five-row cheat sheet.
pub(crate) fn print_cheat_sheet(plan: &DemoPlan) {
    println!();
    println!(
        "cheat sheet — five numbers, read from {}",
        plan.results.display()
    );
    for (index, metric) in plan.cheat_sheet.iter().enumerate() {
        println!("{}. {}", index + 1, metric.label);
        println!(
            "   value {}   class {}   baseline {}",
            value_cell(metric),
            metric.class,
            metric.baseline
        );
        println!("   from {}", metric.pointer);
    }
}

/// Print the closing summary.
pub(crate) fn print_closing(plan: &DemoPlan) -> Result<()> {
    let results = load_results(&plan.results)?;
    println!();
    println!("the three numbers to remember");
    for metric in remember(&results)? {
        println!(
            "  {}  {}  — {} ({}, baseline {})",
            value_cell(&metric),
            metric.label,
            metric.class,
            metric.pointer,
            metric.baseline
        );
    }
    match &plan.rtt_pair {
        Some((plain, condensed)) => {
            println!();
            println!(
                "NSD at {} emulated RTT on the local bridge (not Internet latency)",
                rtt_delay(plain).unwrap_or_else(|| "the recorded".to_string())
            );
            for metric in [plain, condensed] {
                println!(
                    "  {}  {}  — {} (baseline {})",
                    value_cell(metric),
                    metric.label,
                    metric.class,
                    metric.baseline
                );
            }
        }
        None => println!("\nNSD RTT pair: not recorded in this artifact"),
    }
    Ok(())
}

fn rtt_delay(metric: &Metric) -> Option<String> {
    metric
        .id
        .strip_prefix("rtt_nsd_")?
        .rsplit_once('_')
        .map(|(_, suffix)| format!("{} ms", suffix.trim_end_matches("ms")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::preflight::DEMO_PORT;

    fn sample_results() -> ClimbResults {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join(SAMPLE_RESULTS);
        load_results(&path).expect("committed sample artifact")
    }

    fn sample_plan() -> DemoPlan {
        build(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("workspace root"),
            Path::new("target/release/climb-tui"),
            DEMO_PORT,
            false,
        )
        .expect("plan")
    }

    #[test]
    fn the_cheat_sheet_reads_five_headline_rows_from_the_artifact() {
        let rows = cheat_sheet(&sample_results()).expect("sheet");
        assert_eq!(rows.len(), 5);
        let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, CHEAT_SHEET_IDS);
        for row in &rows {
            assert!(!row.display.is_empty(), "{}", row.id);
            assert!(!row.class.is_empty(), "{}", row.id);
            assert!(!row.baseline.is_empty(), "{}", row.id);
            assert!(row.pointer.starts_with('/'), "{}", row.id);
        }
    }

    #[test]
    fn the_cheat_sheet_values_match_the_recorded_headline() {
        let rows = cheat_sheet(&sample_results()).expect("sheet");
        let by_id = |id: &str| rows.iter().find(|row| row.id == id).expect(id);
        assert_eq!(by_id("signing_speedup_medium_c_vs_a").display, "9,434");
        assert_eq!(
            by_id("verifying_speedup_medium_cached_c_vs_a").display,
            "77.34"
        );
        assert_eq!(by_id("wire_reduction_medium_c_vs_a").display, "96.508");
        assert_eq!(by_id("dns_reduction").display, "97.307");
        assert_eq!(by_id("udp_fit_c").display, "11 of 12");
        assert_eq!(by_id("udp_fit_c").class, "ANALYTICAL");
        assert_eq!(by_id("dns_reduction").baseline, "B");
    }

    #[test]
    fn the_plan_names_the_four_panes_and_the_verification_trio_in_order() {
        let plan = sample_plan();
        assert_eq!(plan.server_url, "http://127.0.0.1:8080");
        assert_eq!(plan.server_root, plan.root.join(SITE_DIST));
        assert_eq!(plan.results, plan.root.join(SAMPLE_RESULTS));
        let names: Vec<&str> = plan.verification.iter().map(|step| step.name).collect();
        assert_eq!(names, ["sample-check", "verify-numbers", "report --check"]);
        let report = &plan.verification[2];
        assert!(report.args.iter().any(|arg| arg == SAMPLE_RESULTS));
        assert!(report.args.iter().any(|arg| arg == SAMPLE_REPORT));
        assert!(report.args.iter().any(|arg| arg == "--check"));
    }

    #[test]
    fn the_plan_carries_the_brand_theme_tour_keys_and_recorded_data() {
        let plan = sample_plan();
        let tui = plan.tui.to_string_lossy();
        assert!(tui.ends_with("climb-tui"), "{tui}");
        assert!(plan.tour_keys().contains("t start tour"));
        assert!(plan.tour_keys().contains("Esc leave tour"));
    }

    #[test]
    fn the_rtt_pair_is_the_lowest_delay_with_both_modes() {
        let (plain, condensed) = rtt_pair(&sample_results()).expect("rtt").expect("pair");
        assert_eq!(plain.id, "rtt_nsd_plain_50ms");
        assert_eq!(condensed.id, "rtt_nsd_mtl-condensed_50ms");
        assert_eq!(plain.display, "4.021");
        assert_eq!(condensed.display, "1.01");
        assert_eq!(plain.class, "MEASURED");
        assert_eq!(condensed.baseline, "C");
    }

    #[test]
    fn the_three_to_remember_are_a_subset_of_the_cheat_sheet() {
        let remember = remember(&sample_results()).expect("remember");
        assert_eq!(
            remember
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            REMEMBER_IDS
        );
        let sheet = cheat_sheet(&sample_results()).expect("sheet");
        for row in remember {
            assert!(sheet.iter().any(|candidate| candidate.id == row.id));
        }
    }

    #[test]
    fn a_missing_headline_row_names_the_id_and_the_fix() {
        let mut results = sample_results();
        results.dns_anatomy = None;
        let error = cheat_sheet(&results).expect_err("must fail without dns rows");
        let message = format!("{error:#}");
        assert!(message.contains("dns_reduction"), "{message}");
        assert!(message.contains("cargo xtask consolidate"), "{message}");
    }
}
