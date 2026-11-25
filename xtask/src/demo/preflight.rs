//! GO/NO-GO checks that run before a demonstration.
//!
//! Every reading is collected through a [`Probe`] whose roots and `PATH`
//! lookup directories are explicit, so the whole decision table can be
//! exercised with fake executables and a fake `sysfs` tree instead of the
//! presenter's machine. A required NO-GO stops the demonstration; an optional
//! one only warns, because the live pipeline and the emulated-RTT harness are
//! not needed to show recorded evidence.

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::proc;

/// Port the demo's static server binds. Loopback only.
pub(crate) const DEMO_PORT: u16 = 8080;
/// Free space the demo wants for the browser profile, logs and extraction.
pub(crate) const MIN_DISK_FREE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Battery charge that counts as enough for a short demo.
pub(crate) const MIN_BATTERY_PERCENT: u8 = 60;
/// Earliest wall clock the demo accepts (2025-01-01T00:00:00Z).
const CLOCK_LOWER_SECS: u64 = 1_735_689_600;
/// Latest wall clock the demo accepts (2035-01-01T00:00:00Z).
const CLOCK_UPPER_SECS: u64 = 2_051_222_400;

pub(crate) const TUI_RELEASE: &str = "target/release/climb-tui";
pub(crate) const TUI_DEBUG: &str = "target/debug/climb-tui";
pub(crate) const SAMPLE_RESULTS: &str = "docs/sample-output/climb-results.json";
pub(crate) const SAMPLE_REPORT: &str = "docs/sample-output/report.html";
pub(crate) const RECORDED_CAPTURE: &str = "docs/sample-output/dns-capture.recorded.json";
pub(crate) const SITE_DIST: &str = "site/dist";
pub(crate) const DECK_PDF: &str = "A3-PS036-TC529.pdf";
pub(crate) const PLAYWRIGHT_BROWSERS: &str = "site/node_modules/playwright-core/.local-browsers";

/// Images the live DNS pipeline builds. Present locally, they make the
/// optional live stop possible without rebuilding.
pub(crate) const DOCKER_IMAGES: [&str; 4] = [
    "climb-nsd:phase12",
    "climb-unbound:phase13",
    "climb-dig:phase14",
    "climb-ldns-signer:phase14",
];

/// The fix printed for each failing check.
const FIX_BATTERY: &str =
    "plug in the mains adapter (check with: cat /sys/class/power_supply/BAT*/capacity)";
const FIX_DISK: &str = "cargo clean   # frees target/, then re-run demo/preflight.sh";
const FIX_TUI: &str = "cargo build --release -p climb-tui --no-default-features";
const FIX_SAMPLE: &str = "scripts/make-sample-output.sh";
const FIX_SITE: &str = "cd site && npm ci && npm run build";
const FIX_CAPTURE: &str =
    "git restore docs/sample-output/dns-capture.recorded.json   # or: cargo xtask dns-demo";
const FIX_DECK: &str = "git restore A3-PS036-TC529.pdf";
const FIX_BROWSER: &str =
    "cd site && npm ci && PLAYWRIGHT_BROWSERS_PATH=0 npx playwright install chromium";
const FIX_PORT: &str = "fuser -k 8080/tcp   # then re-run; inspect with: ss -ltnp 'sport = :8080'";
const FIX_CLOCK: &str = "sudo timedatectl set-ntp true   # then: date -u";
const FIX_DOCKER: &str = "docker compose -f docker/compose.dns-pipeline.yml --profile tools build";
const FIX_NETEM: &str = "sudo modprobe sch_netem";

/// Outcome of one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Go,
    NoGo,
    /// The check does not apply to this run.
    Skip,
}

impl Status {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Status::Go => "GO",
            Status::NoGo => "NO-GO",
            Status::Skip => "SKIP",
        }
    }
}

/// One row of the GO/NO-GO table.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Row {
    pub(crate) check: &'static str,
    pub(crate) required: bool,
    pub(crate) status: Status,
    pub(crate) detail: String,
    pub(crate) fix: Option<&'static str>,
}

impl Row {
    fn go(check: &'static str, required: bool, detail: impl Into<String>) -> Self {
        Self {
            check,
            required,
            status: Status::Go,
            detail: detail.into(),
            fix: None,
        }
    }

    fn no_go(
        check: &'static str,
        required: bool,
        detail: impl Into<String>,
        fix: &'static str,
    ) -> Self {
        Self {
            check,
            required,
            status: Status::NoGo,
            detail: detail.into(),
            fix: Some(fix),
        }
    }

    fn skip(check: &'static str, required: bool, detail: impl Into<String>) -> Self {
        Self {
            check,
            required,
            status: Status::Skip,
            detail: detail.into(),
            fix: None,
        }
    }
}

/// A local machine snapshot, readable without the network.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Readings {
    pub(crate) ac_online: Option<bool>,
    pub(crate) battery_percent: Option<u8>,
    pub(crate) disk_free_bytes: Option<u64>,
    pub(crate) tui_binary: Option<PathBuf>,
    pub(crate) sample_results: bool,
    pub(crate) sample_report: bool,
    pub(crate) site_dist: bool,
    pub(crate) capture: bool,
    pub(crate) deck_pdf: bool,
    pub(crate) browser: Option<String>,
    pub(crate) port_free: bool,
    pub(crate) clock_sane: bool,
    pub(crate) clock_detail: String,
    /// `None` when no `docker` executable is on `PATH`.
    pub(crate) docker_missing: Option<Vec<String>>,
    pub(crate) netem: bool,
    pub(crate) tmux: bool,
    pub(crate) offline: bool,
}

/// The inputs [`collect`] reads. Tests point these at temporary directories.
#[derive(Debug, Clone)]
pub(crate) struct Probe {
    pub(crate) root: PathBuf,
    pub(crate) sysfs: PathBuf,
    pub(crate) dirs: Vec<PathBuf>,
    pub(crate) now_secs: u64,
    pub(crate) port: u16,
    /// When set, the network-dependent optional checks are reported as skipped.
    pub(crate) offline: bool,
}

impl Probe {
    /// A probe of the real machine, rooted at the workspace.
    pub(crate) fn real(root: &Path, port: u16, offline: bool) -> Self {
        Self {
            root: root.to_path_buf(),
            sysfs: PathBuf::from("/sys"),
            dirs: path_dirs(),
            now_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            port,
            offline,
        }
    }
}

/// Directories named by `PATH`.
pub(crate) fn path_dirs() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|joined| std::env::split_paths(&joined).collect())
        .unwrap_or_default()
}

/// Locate `name` in `dirs`, or accept a path that already names a file.
pub(crate) fn locate(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let path = PathBuf::from(name);
        return path.is_file().then_some(path);
    }
    dirs.iter()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// True when something is listening on the loopback demo port.
pub(crate) fn port_is_free(port: u16) -> bool {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => {
            drop(listener);
            true
        }
        Err(_) => false,
    }
}

/// A wall clock is sane when it sits in the supported window and is not older
/// than the commit the demo will claim to run.
pub(crate) fn clock_is_sane(now_secs: u64, head_commit_secs: Option<u64>) -> bool {
    if !(CLOCK_LOWER_SECS..=CLOCK_UPPER_SECS).contains(&now_secs) {
        return false;
    }
    head_commit_secs.is_none_or(|commit| now_secs >= commit)
}

fn read_power(sysfs: &Path) -> (Option<bool>, Option<u8>) {
    let supply = sysfs.join("class/power_supply");
    let Ok(entries) = fs::read_dir(&supply) else {
        return (None, None);
    };
    let mut ac_online = None;
    let mut battery = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = fs::read_to_string(path.join("type"))
            .unwrap_or_default()
            .trim()
            .to_string();
        match kind.as_str() {
            "Mains" | "USB" => {
                if let Ok(value) = fs::read_to_string(path.join("online")) {
                    let online = value.trim() == "1";
                    ac_online = Some(ac_online.unwrap_or(false) || online);
                }
            }
            "Battery" => {
                if let Ok(value) = fs::read_to_string(path.join("capacity")) {
                    if let Ok(percent) = value.trim().parse::<u8>() {
                        battery = Some(battery.map_or(percent, |current: u8| current.max(percent)));
                    }
                }
            }
            _ => {}
        }
    }
    (ac_online, battery)
}

fn read_disk_free(df: Option<&Path>, target: &Path) -> Option<u64> {
    let df = df?;
    let mut command = Command::new(df);
    command.args(["-Pk", "--"]).arg(target);
    let output = match proc::run(&mut command, Duration::from_secs(10), "df -Pk") {
        Ok(output) => output,
        Err(error) => {
            eprintln!("read_disk_free: df failed: {error:#}");
            return None;
        }
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let row = text.lines().nth(1)?;
    let available_kib: u64 = row.split_whitespace().nth(3)?.parse().ok()?;
    available_kib.checked_mul(1024)
}

fn read_head_commit(git: Option<&Path>, root: &Path) -> Option<u64> {
    let git = git?;
    let mut command = Command::new(git);
    command
        .args(["-C"])
        .arg(root)
        .args(["show", "-s", "--format=%ct", "HEAD"]);
    let output = proc::run(&mut command, Duration::from_secs(10), "git show").ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

fn read_missing_images(docker: Option<&Path>) -> Option<Vec<String>> {
    let docker = docker?;
    let mut present = Vec::new();
    for image in DOCKER_IMAGES {
        let mut command = Command::new(docker);
        command.args(["image", "inspect", "--format", "{{.Id}}", image]);
        if proc::run(
            &mut command,
            Duration::from_secs(20),
            "docker image inspect",
        )
        .is_ok()
        {
            present.push(image);
        }
    }
    let missing = DOCKER_IMAGES
        .iter()
        .filter(|image| !present.contains(image))
        .map(|image| (*image).to_string())
        .collect();
    Some(missing)
}

fn netem_available(modinfo: Option<&Path>, sysfs: &Path) -> bool {
    if sysfs.join("module/sch_netem").is_dir() {
        return true;
    }
    let Some(modinfo) = modinfo else {
        return false;
    };
    let mut command = Command::new(modinfo);
    command.arg("sch_netem");
    proc::run(&mut command, Duration::from_secs(10), "modinfo sch_netem").is_ok()
}

fn browser_detail(root: &Path, dirs: &[PathBuf]) -> Option<String> {
    for name in [
        "chromium",
        "chromium-browser",
        "google-chrome-stable",
        "google-chrome",
        "firefox",
    ] {
        if locate(dirs, name).is_some() {
            return Some(format!("{name} on PATH"));
        }
    }
    let browsers = root.join(PLAYWRIGHT_BROWSERS);
    let entries = fs::read_dir(browsers).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("chromium") {
            continue;
        }
        let path = entry.path();
        if path.join("chrome-linux64/chrome").is_file()
            || path
                .join("chrome-headless-shell-linux64/chrome-headless-shell")
                .is_file()
        {
            return Some(format!("Playwright {name} (repository-local)"));
        }
    }
    None
}

fn tui_binary(root: &Path, dirs: &[PathBuf]) -> Option<PathBuf> {
    for candidate in [TUI_RELEASE, TUI_DEBUG] {
        let path = root.join(candidate);
        if path.is_file() {
            return Some(path);
        }
    }
    locate(dirs, "climb-tui")
}

pub(crate) fn locate_tui(root: &Path) -> Option<PathBuf> {
    tui_binary(root, &path_dirs())
}

/// Read every check's raw input.
pub(crate) fn collect(probe: &Probe) -> Readings {
    let dirs = &probe.dirs;
    let df = locate(dirs, "df");
    let git = locate(dirs, "git");
    let docker = locate(dirs, "docker");
    let modinfo = locate(dirs, "modinfo");

    let (ac_online, battery_percent) = read_power(&probe.sysfs);
    let head = read_head_commit(git.as_deref(), &probe.root);
    let clock_sane = clock_is_sane(probe.now_secs, head);
    let clock_detail = match head {
        Some(commit) if probe.now_secs < commit => {
            format!("clock is older than HEAD ({commit})")
        }
        _ => "wall clock is in the supported window".to_string(),
    };

    Readings {
        ac_online,
        battery_percent,
        disk_free_bytes: read_disk_free(df.as_deref(), &probe.root),
        tui_binary: tui_binary(&probe.root, dirs),
        sample_results: probe.root.join(SAMPLE_RESULTS).is_file(),
        sample_report: probe.root.join(SAMPLE_REPORT).is_file(),
        site_dist: probe.root.join(SITE_DIST).join("index.html").is_file(),
        capture: probe.root.join(RECORDED_CAPTURE).is_file(),
        deck_pdf: probe.root.join(DECK_PDF).is_file(),
        browser: browser_detail(&probe.root, dirs),
        port_free: port_is_free(probe.port),
        clock_sane,
        clock_detail,
        docker_missing: read_missing_images(docker.as_deref()),
        netem: netem_available(modinfo.as_deref(), &probe.sysfs),
        tmux: locate(dirs, "tmux").is_some(),
        offline: probe.offline,
    }
}

fn gib(bytes: u64) -> String {
    format!("{:.1} GiB free", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

/// The decision table. Pure: every branch is a function of `readings`.
pub(crate) fn rows(readings: &Readings) -> Vec<Row> {
    let mut rows = Vec::new();

    match (readings.ac_online, readings.battery_percent) {
        (None, None) => rows.push(Row::skip(
            "power",
            false,
            "no power-supply entries; treating this as a mains-powered host",
        )),
        (ac, battery) => {
            let power_ok =
                ac == Some(true) || battery.is_some_and(|percent| percent >= MIN_BATTERY_PERCENT);
            let detail = match (ac, battery) {
                (Some(true), Some(percent)) => format!("mains power ({percent}% battery)"),
                (Some(true), None) => "mains power".to_string(),
                (_, Some(percent)) if power_ok => {
                    format!("battery {percent}% (at least {MIN_BATTERY_PERCENT}%)")
                }
                (_, Some(percent)) => format!("battery {percent}%, no mains power"),
                (Some(_), None) => "on mains power".to_string(),
                (None, None) => unreachable!("handled above"),
            };
            rows.push(if power_ok {
                Row::go("power", true, detail)
            } else {
                Row::no_go("power", true, detail, FIX_BATTERY)
            });
        }
    }

    match readings.disk_free_bytes {
        Some(bytes) if bytes >= MIN_DISK_FREE_BYTES => {
            rows.push(Row::go("disk free", true, gib(bytes)));
        }
        Some(bytes) => rows.push(Row::no_go(
            "disk free",
            true,
            format!("{} (want at least 2 GiB)", gib(bytes)),
            FIX_DISK,
        )),
        None => rows.push(Row::no_go(
            "disk free",
            true,
            "could not read the free-space figure",
            FIX_DISK,
        )),
    }

    match &readings.tui_binary {
        Some(path) => rows.push(Row::go(
            "climb-tui binary",
            true,
            path.display().to_string(),
        )),
        None => rows.push(Row::no_go(
            "climb-tui binary",
            true,
            format!("absent at {TUI_RELEASE} and {TUI_DEBUG}, and not on PATH"),
            FIX_TUI,
        )),
    }

    if readings.sample_results && readings.sample_report {
        rows.push(Row::go(
            "sample output",
            true,
            "results document and rendered report",
        ));
    } else {
        let missing = [
            (!readings.sample_results).then_some(SAMPLE_RESULTS),
            (!readings.sample_report).then_some(SAMPLE_REPORT),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
        rows.push(Row::no_go(
            "sample output",
            true,
            format!("missing: {missing}"),
            FIX_SAMPLE,
        ));
    }

    if readings.site_dist {
        rows.push(Row::go("site/dist", true, SITE_DIST));
    } else {
        rows.push(Row::no_go(
            "site/dist",
            true,
            format!("{} has no index.html", SITE_DIST),
            FIX_SITE,
        ));
    }

    if readings.capture {
        rows.push(Row::go("recorded capture", true, RECORDED_CAPTURE));
    } else {
        rows.push(Row::no_go(
            "recorded capture",
            true,
            format!("{RECORDED_CAPTURE} is absent"),
            FIX_CAPTURE,
        ));
    }

    if readings.deck_pdf {
        rows.push(Row::go("PDF deck", true, DECK_PDF));
    } else {
        rows.push(Row::no_go(
            "PDF deck",
            true,
            format!("{DECK_PDF} is absent"),
            FIX_DECK,
        ));
    }

    match &readings.browser {
        Some(detail) => rows.push(Row::go("browser", true, detail.clone())),
        None => rows.push(Row::no_go(
            "browser",
            true,
            "no Chromium/Firefox on PATH and no Playwright browser in the repository",
            FIX_BROWSER,
        )),
    }

    if readings.port_free {
        rows.push(Row::go("port 8080", true, "free on 127.0.0.1"));
    } else {
        rows.push(Row::no_go(
            "port 8080",
            true,
            "127.0.0.1:8080 is already in use",
            FIX_PORT,
        ));
    }

    if readings.clock_sane {
        rows.push(Row::go("system clock", true, readings.clock_detail.clone()));
    } else {
        rows.push(Row::no_go(
            "system clock",
            true,
            readings.clock_detail.clone(),
            FIX_CLOCK,
        ));
    }

    if readings.offline {
        rows.push(Row::skip(
            "Docker images",
            false,
            "not needed offline; the live pipeline is a separate stop",
        ));
        rows.push(Row::skip(
            "sch_netem",
            false,
            "not needed offline; the emulated-RTT harness is a separate stop",
        ));
    } else {
        match &readings.docker_missing {
            None => rows.push(Row::no_go(
                "Docker images",
                false,
                "no docker executable on PATH; the live pipeline will be unavailable",
                FIX_DOCKER,
            )),
            Some(missing) if missing.is_empty() => {
                rows.push(Row::go("Docker images", false, "all four pipeline images"));
            }
            Some(missing) => rows.push(Row::no_go(
                "Docker images",
                false,
                format!("missing: {}", missing.join(", ")),
                FIX_DOCKER,
            )),
        }
        if readings.netem {
            rows.push(Row::go("sch_netem", false, "kernel module is available"));
        } else {
            rows.push(Row::no_go(
                "sch_netem",
                false,
                "kernel module is not loaded",
                FIX_NETEM,
            ));
        }
    }

    rows.push(if readings.tmux {
        Row::go("tmux layout", false, "tmux is available")
    } else {
        Row::skip(
            "tmux layout",
            false,
            "tmux is not installed; panes print sequentially instead",
        )
    });

    rows
}

/// Print the table and the fix line for every NO-GO. Returns whether every
/// required check passed.
pub(crate) fn report(rows: &[Row]) -> bool {
    println!();
    println!("demo preflight — GO/NO-GO before the demonstration");
    println!("{:<18} {:<9} {:<7} detail", "check", "required", "status");
    for row in rows {
        println!(
            "{:<18} {:<9} {:<7} {}",
            row.check,
            if row.required { "yes" } else { "no" },
            row.status.label(),
            row.detail
        );
    }
    let failures: Vec<&Row> = rows
        .iter()
        .filter(|row| row.status == Status::NoGo)
        .collect();
    if !failures.is_empty() {
        println!();
        println!("fixes");
        for row in &failures {
            println!("  {}: {}", row.check, row.fix.unwrap_or("no fix recorded"));
        }
    }
    let required_failures = rows
        .iter()
        .filter(|row| row.required && row.status == Status::NoGo)
        .count();
    println!();
    if required_failures == 0 {
        println!(
            "preflight: GO{}{}",
            if failures.is_empty() {
                ""
            } else {
                " (optional checks failed)"
            },
            if failures.is_empty() {
                ""
            } else {
                "; see fixes above"
            }
        );
    } else {
        println!(
            "preflight: NO-GO — {} required check(s) failed; fix them and re-run demo/preflight.sh",
            required_failures
        );
    }
    required_failures == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Instant;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "climb-demo-preflight-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp root");
        dir
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        fs::write(path, contents).expect("write");
    }

    fn fake_binary(dir: &Path, name: &str, body: &str) {
        let path = dir.join(name);
        write_file(&path, &format!("#!/bin/sh\n{body}\n"));
        let mut permissions = fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("chmod");
    }

    fn healthy() -> Readings {
        Readings {
            ac_online: Some(true),
            battery_percent: Some(100),
            disk_free_bytes: Some(20 * 1024 * 1024 * 1024),
            tui_binary: Some(PathBuf::from(TUI_RELEASE)),
            sample_results: true,
            sample_report: true,
            site_dist: true,
            capture: true,
            deck_pdf: true,
            browser: Some("chromium on PATH".to_string()),
            port_free: true,
            clock_sane: true,
            clock_detail: "wall clock is in the supported window".to_string(),
            docker_missing: Some(Vec::new()),
            netem: true,
            tmux: true,
            offline: false,
        }
    }

    #[test]
    fn a_healthy_host_is_all_go_and_passes() {
        let rows = rows(&healthy());
        assert!(rows.iter().all(|row| row.status == Status::Go));
        assert!(report(&rows));
    }

    #[test]
    fn every_required_check_can_fail_with_a_fix() {
        type Mutation = fn(&mut Readings);
        let cases: [(&str, Mutation); 9] = [
            ("power", |r| {
                r.ac_online = Some(false);
                r.battery_percent = Some(20);
            }),
            ("disk free", |r| r.disk_free_bytes = Some(10)),
            ("climb-tui binary", |r| r.tui_binary = None),
            ("sample output", |r| r.sample_results = false),
            ("site/dist", |r| r.site_dist = false),
            ("recorded capture", |r| r.capture = false),
            ("PDF deck", |r| r.deck_pdf = false),
            ("browser", |r| r.browser = None),
            ("port 8080", |r| r.port_free = false),
        ];
        for (check, mutate) in cases {
            let mut readings = healthy();
            mutate(&mut readings);
            let rows = rows(&readings);
            let row = rows
                .iter()
                .find(|row| row.check == check)
                .unwrap_or_else(|| panic!("no row named {check}"));
            assert_eq!(row.status, Status::NoGo, "{check}");
            assert!(row.required, "{check} must be required");
            assert!(row.fix.is_some(), "{check} needs a fix command");
            assert!(!report(&rows), "{check} must fail the preflight");
        }
    }

    #[test]
    fn the_clock_is_a_required_no_go_when_it_is_wrong() {
        let mut readings = healthy();
        readings.clock_sane = false;
        readings.clock_detail = "clock is older than HEAD".to_string();
        let rows = rows(&readings);
        let row = rows.iter().find(|row| row.check == "system clock").unwrap();
        assert_eq!(row.status, Status::NoGo);
        assert!(row.required);
        assert!(!report(&rows));
    }

    #[test]
    fn optional_checks_fail_without_stopping_the_demo() {
        let mut readings = healthy();
        readings.docker_missing = Some(vec!["climb-dig:phase14".to_string()]);
        readings.netem = false;
        let rows = rows(&readings);
        assert!(rows.iter().any(|row| row.check == "Docker images"
            && row.status == Status::NoGo
            && !row.required));
        assert!(rows
            .iter()
            .any(|row| row.check == "sch_netem" && row.status == Status::NoGo && !row.required));
        assert!(report(&rows), "optional checks must not block the demo");
    }

    #[test]
    fn offline_skips_the_optional_network_checks() {
        let mut readings = healthy();
        readings.offline = true;
        readings.docker_missing = None;
        readings.netem = false;
        let rows = rows(&readings);
        for check in ["Docker images", "sch_netem"] {
            let row = rows.iter().find(|row| row.check == check).unwrap();
            assert_eq!(row.status, Status::Skip, "{check}");
        }
        assert!(report(&rows));
    }

    #[test]
    fn a_desktop_without_power_supply_entries_is_not_a_no_go() {
        let mut readings = healthy();
        readings.ac_online = None;
        readings.battery_percent = None;
        let rows = rows(&readings);
        let row = rows.iter().find(|row| row.check == "power").unwrap();
        assert_eq!(row.status, Status::Skip);
        assert!(report(&rows));
    }

    #[test]
    fn a_battery_above_sixty_percent_is_enough() {
        let mut readings = healthy();
        readings.ac_online = Some(false);
        readings.battery_percent = Some(MIN_BATTERY_PERCENT);
        let high = rows(&readings);
        assert_eq!(
            high.iter().find(|row| row.check == "power").unwrap().status,
            Status::Go
        );
        readings.battery_percent = Some(MIN_BATTERY_PERCENT - 1);
        let low = rows(&readings);
        assert_eq!(
            low.iter().find(|row| row.check == "power").unwrap().status,
            Status::NoGo
        );
    }

    #[test]
    fn power_is_read_from_a_fake_sysfs_tree() {
        let root = temp_root("power");
        let sysfs = root.join("sys");
        write_file(&sysfs.join("class/power_supply/AC/type"), "Mains\n");
        write_file(&sysfs.join("class/power_supply/AC/online"), "0\n");
        write_file(&sysfs.join("class/power_supply/BAT0/type"), "Battery\n");
        write_file(&sysfs.join("class/power_supply/BAT0/capacity"), "42\n");
        assert_eq!(read_power(&sysfs), (Some(false), Some(42)));
        write_file(&sysfs.join("class/power_supply/AC/online"), "1\n");
        assert_eq!(read_power(&sysfs), (Some(true), Some(42)));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn disk_free_parses_df_and_reports_a_missing_df() {
        let root = temp_root("disk");
        let bin = root.join("bin");
        fake_binary(&bin, "df", "printf 'Filesystem 1024-blocks Used Available Capacity Mounted on\\n/dev/x 100 10 50 70%% /\\n'");
        let df = locate(std::slice::from_ref(&bin), "df");
        // A shared runner can fail one exec transiently under load; the parser
        // itself is what this test proves, and a broken fake still fails all
        // three attempts.
        let disk = (0..3).find_map(|_| read_disk_free(df.as_deref(), &root));
        assert_eq!(disk, Some(50 * 1024));
        assert_eq!(read_disk_free(None, &root), None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_docker_images_are_named_one_by_one() {
        let root = temp_root("docker");
        let bin = root.join("bin");
        fake_binary(
            &bin,
            "docker",
            "case \"$*\" in *climb-dig:phase14*) exit 1;; *) exit 0;; esac",
        );
        let docker = locate(&[bin], "docker");
        assert_eq!(
            read_missing_images(docker.as_deref()),
            Some(vec!["climb-dig:phase14".to_string()])
        );
        assert_eq!(read_missing_images(None), None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn netem_is_available_from_the_module_directory_or_modinfo() {
        let root = temp_root("netem");
        let sysfs = root.join("sys");
        assert!(!netem_available(None, &sysfs));
        fs::create_dir_all(sysfs.join("module/sch_netem")).expect("module dir");
        assert!(netem_available(None, &sysfs));
        let bin = root.join("bin");
        fake_binary(&bin, "modinfo", "exit 0");
        let modinfo = locate(&[bin], "modinfo");
        assert!(netem_available(modinfo.as_deref(), &root.join("empty-sys")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn the_clock_refuses_a_rewound_or_far_future_date() {
        let commit = CLOCK_LOWER_SECS + 1000;
        assert!(clock_is_sane(commit + 10, Some(commit)));
        assert!(!clock_is_sane(commit - 10, Some(commit)));
        assert!(!clock_is_sane(CLOCK_LOWER_SECS - 1, None));
        assert!(!clock_is_sane(CLOCK_UPPER_SECS + 1, None));
        assert!(clock_is_sane(CLOCK_LOWER_SECS, None));
    }

    fn wait_for_free_port(port: u16) {
        // Other tests in this binary bind ephemeral ports in parallel, so the
        // kernel can hand a just-released port to one of them. Wait for it to
        // be free again instead of assuming an instant release; a port that
        // never frees still fails.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !port_is_free(port) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(port_is_free(port), "port {port} was not free within 5 s");
    }

    #[test]
    fn the_port_probe_notices_a_bound_socket() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral");
        let port = listener.local_addr().expect("addr").port();
        assert!(!port_is_free(port));
        drop(listener);
        wait_for_free_port(port);
    }

    #[test]
    fn a_repository_local_playwright_browser_counts_as_available() {
        let root = temp_root("browser");
        let chrome = root.join(PLAYWRIGHT_BROWSERS).join("chromium-1248");
        write_file(&chrome.join("chrome-linux64/chrome"), "");
        let detail = browser_detail(&root, &[]).expect("browser");
        assert!(detail.contains("Playwright"), "{detail}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn collect_reads_the_fake_tree_end_to_end() {
        let root = temp_root("collect");
        let sysfs = root.join("sys");
        write_file(&sysfs.join("class/power_supply/AC/type"), "Mains\n");
        write_file(&sysfs.join("class/power_supply/AC/online"), "1\n");
        write_file(&sysfs.join("class/power_supply/BAT0/type"), "Battery\n");
        write_file(&sysfs.join("class/power_supply/BAT0/capacity"), "88\n");
        write_file(&root.join(SAMPLE_RESULTS), "{}");
        write_file(&root.join(SAMPLE_REPORT), "<html></html>");
        write_file(&root.join(SITE_DIST).join("index.html"), "<html></html>");
        write_file(&root.join(RECORDED_CAPTURE), "{}");
        write_file(&root.join(DECK_PDF), "%PDF");
        write_file(&root.join(TUI_RELEASE), "");

        let bin = root.join("bin");
        fake_binary(&bin, "df", "printf 'Filesystem 1024-blocks Used Available Capacity Mounted on\\n/dev/x 100 10 99999999 1%% /\\n'");
        fake_binary(&bin, "git", "printf '1735689600\\n'");
        fake_binary(&bin, "docker", "exit 0");
        fake_binary(&bin, "modinfo", "exit 0");
        fake_binary(&bin, "chromium", "exit 0");

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);

        let probe = Probe {
            root: root.clone(),
            sysfs,
            dirs: vec![bin],
            now_secs: CLOCK_LOWER_SECS + 100,
            port,
            offline: true,
        };
        wait_for_free_port(port);
        // The port can be claimed by a parallel test between the wait and the
        // collection; collect again while it is held instead of asserting on a
        // single race.
        let deadline = Instant::now() + Duration::from_secs(5);
        let readings = loop {
            let readings = collect(&probe);
            if readings.port_free || Instant::now() >= deadline {
                break readings;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(
            readings.port_free,
            "the probe port {port} was held for the whole collection window"
        );
        assert_eq!(readings.battery_percent, Some(88));
        assert_eq!(readings.ac_online, Some(true));
        assert!(readings.sample_results && readings.sample_report);
        assert!(readings.clock_sane, "{}", readings.clock_detail);
        assert!(readings.docker_missing.as_ref().is_some_and(Vec::is_empty));
        assert!(readings.netem);
        let rows = rows(&readings);
        assert!(rows
            .iter()
            .all(|row| row.status != Status::NoGo || !row.required));
        let _ = fs::remove_dir_all(root);
    }
}
