//! `cargo xtask demo`: one command that puts the recorded evidence in front of
//! an audience.
//!
//! The run is arranged as four panes: the dashboard on the recorded artifact
//! with the brand theme, the verification trio with one verdict per check, a
//! loopback static server over the built site, and a cheat sheet whose five
//! rows are read from the artifact. `tmux` arranges the panes side by side;
//! without it, or with `--no-tmux`, the same panes print one after another so
//! a dry run is still reviewable. `--offline` runs the browser over every route
//! with all non-localhost requests blocked, which is the proof that the
//! demonstration needs no network.

mod plan;
mod preflight;
mod server;

use std::io::{IsTerminal, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use clap::{Args, ValueEnum};

use crate::proc;

pub(crate) use plan::DemoPlan;
pub(crate) use preflight::{Probe, DEMO_PORT};

/// A single pane of the demonstration, for tmux to run in its own window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Pane {
    /// The dashboard, on the recorded artifact, with the guided-tour keys.
    Tui,
    /// The three verification commands, one verdict each.
    Verify,
    /// The loopback static server over `site/dist`.
    Serve,
    /// The five-number cheat sheet and the closing summary.
    Cheat,
}

#[derive(Args)]
pub(crate) struct DemoArgs {
    /// Workspace root.
    #[arg(long, default_value = ".")]
    pub(crate) root: PathBuf,

    /// Prove the run needs no network: block every non-localhost request in a
    /// browser and load all five routes.
    #[arg(long)]
    pub(crate) offline: bool,

    /// Print the panes one after another even when tmux is installed.
    #[arg(long)]
    pub(crate) no_tmux: bool,

    /// Print the preflight table and the plan, then stop.
    #[arg(long = "plan")]
    pub(crate) plan_only: bool,

    /// Run the preflight only, and exit non-zero on a required NO-GO.
    #[arg(long)]
    pub(crate) preflight_only: bool,

    /// Run one pane; used by the tmux layout.
    #[arg(long, value_enum, hide = true)]
    pub(crate) pane: Option<Pane>,

    /// Loopback port for the static server.
    #[arg(long, default_value_t = DEMO_PORT)]
    pub(crate) port: u16,
}

pub(crate) fn cmd_demo(args: DemoArgs) -> Result<()> {
    if let Some(pane) = args.pane {
        return run_pane(&args, pane);
    }

    let probe = Probe::real(&args.root, args.port, args.offline);
    let readings = preflight::collect(&probe);
    let rows = preflight::rows(&readings);
    let go = preflight::report(&rows);
    if !go {
        bail!("preflight refused the demonstration; fix the NO-GO rows and re-run");
    }
    if args.preflight_only {
        return Ok(());
    }

    let tui = readings
        .tui_binary
        .clone()
        .context("the climb-tui binary is required to run the demo")?;
    let demo = plan::build(&args.root, &tui, args.port, args.offline)?;
    print_plan(&demo, &readings);

    if args.plan_only {
        println!();
        println!("plan: dry run, nothing started");
        return Ok(());
    }

    let use_tmux = readings.tmux && !args.no_tmux && std::io::stdout().is_terminal();
    if use_tmux {
        run_tmux(&demo)
    } else {
        let reason = if !readings.tmux {
            "tmux is not installed"
        } else if args.no_tmux {
            "--no-tmux was passed"
        } else {
            "stdout is not a terminal"
        };
        println!();
        println!("layout: sequential panes ({reason})");
        run_sequential(&demo, &args)
    }
}

fn print_plan(demo: &DemoPlan, readings: &preflight::Readings) {
    println!();
    println!(
        "demo plan — offline={} tmux={} port={}",
        if demo.offline { "yes" } else { "no" },
        if readings.tmux { "available" } else { "absent" },
        demo.port
    );
    println!("  1. dashboard   {}", demo.tui.display());
    println!(
        "     args        --results {} --theme brand --replay",
        demo.staged_results.display()
    );
    println!("     source      {}", demo.results.display());
    println!("     keys        {}", demo.tour_keys());
    println!("  2. verification");
    for step in &demo.verification {
        println!("     cargo xtask {}", step.args.join(" "));
    }
    println!(
        "  3. server      {} → {}",
        demo.server_root.display(),
        demo.server_url
    );
    println!(
        "  4. cheat sheet {} rows read from {}",
        demo.cheat_sheet.len(),
        demo.results.display()
    );
    if demo.offline {
        println!("     offline     Playwright blocks every non-localhost request");
    }
}

fn run_sequential(demo: &DemoPlan, args: &DemoArgs) -> Result<()> {
    print_tui_frame(demo)?;

    let exe = current_exe()?;
    let verdicts = run_verification(demo, &exe)?;
    if !verdicts {
        bail!("the verification trio disagreed with itself; fix the failing check before demonstrating");
    }

    let addr: SocketAddr = format!("127.0.0.1:{}", args.port)
        .parse()
        .context("the demo port is not a valid loopback address")?;
    let running = server::start(&demo.server_root, addr)?;
    println!();
    println!(
        "static server: {} serving {}",
        running.url(),
        demo.server_root.display()
    );
    let served = http_self_check(running.addr, &demo.server_url)?;
    if !served {
        running.stop();
        bail!("the static server did not answer every route; fix it before demonstrating");
    }

    if demo.offline {
        if let Err(error) = run_offline_smoke(demo) {
            running.stop();
            return Err(error);
        }
    }

    plan::print_cheat_sheet(demo);
    plan::print_closing(demo)?;
    running.stop();
    println!();
    println!("demo: sequential run complete; the static server is now stopped");
    Ok(())
}

fn print_tui_frame(demo: &DemoPlan) -> Result<()> {
    plan::stage_viewer(demo)?;
    let mut check = Command::new(&demo.tui);
    check
        .args(["--results"])
        .arg(&demo.staged_results)
        .args(["--theme", "brand", "--replay", "--check"]);
    let output = proc::run(&mut check, Duration::from_secs(60), "climb-tui --check")?;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        println!("  tui {line}");
    }
    println!();
    println!("dashboard — recorded data, brand theme");
    println!("keys: {}", demo.tour_keys());
    let mut frame = Command::new(&demo.tui);
    frame.args(["--results"]).arg(&demo.staged_results).args([
        "--theme",
        "brand",
        "--replay",
        "--dump-frame",
        "overview",
        "120x40",
    ]);
    let output = proc::run(
        &mut frame,
        Duration::from_secs(60),
        "climb-tui --dump-frame",
    )?;
    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

fn current_exe() -> Result<PathBuf> {
    std::env::current_exe().context("locating the running xtask binary")
}

fn last_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
}

fn run_verification(demo: &DemoPlan, exe: &Path) -> Result<bool> {
    println!();
    println!("verification trio");
    let mut all_ok = true;
    for step in &demo.verification {
        let mut command = Command::new(exe);
        command
            .args(&step.args)
            .current_dir(&demo.root)
            .env("NO_COLOR", "1");
        let output = proc::run_raw(&mut command, Duration::from_secs(900), step.name)?;
        let verdict = last_line(&output.stdout)
            .or_else(|| last_line(&output.stderr))
            .unwrap_or_else(|| "no output".to_string());
        if output.status.success() {
            println!("  [ok]     {:<16} {verdict}", step.name);
        } else {
            all_ok = false;
            println!("  [FAILED] {:<16} {verdict}", step.name);
            for line in String::from_utf8_lossy(&output.stderr).lines().take(8) {
                println!("           {line}");
            }
            println!("           fix: cargo xtask {}", step.args.join(" "));
        }
    }
    Ok(all_ok)
}

fn http_get(addr: SocketAddr, path: &str) -> Result<(u16, usize)> {
    use std::io::{BufRead, BufReader, Read};
    let mut stream = TcpStream::connect(addr).with_context(|| format!("connecting to {addr}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .context("setting the HTTP read timeout")?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .context("writing the HTTP request")?;
    let mut reader = BufReader::new(stream);
    let mut status = 0;
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .context("reading the status line")?;
    if let Some(code) = line.split_whitespace().nth(1) {
        status = code.parse().unwrap_or(0);
    }
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
            break;
        }
    }
    let mut body = Vec::new();
    reader.read_to_end(&mut body).context("reading the body")?;
    Ok((status, body.len()))
}

fn http_self_check(addr: SocketAddr, base: &str) -> Result<bool> {
    println!();
    println!("static server self-check");
    let routes = [
        ("/", 200_u16),
        ("/tui/", 200),
        ("/verify/", 200),
        ("/report/", 200),
        ("/reproduce/", 200),
        ("/this-page-does-not-exist", 404),
    ];
    let mut ok = true;
    for (path, expected) in routes {
        let (status, bytes) = http_get(addr, path)?;
        let good = status == expected;
        ok &= good;
        println!(
            "  {} {:<26} {}  {} bytes",
            if good { "ok  " } else { "FAIL" },
            path,
            status,
            bytes
        );
    }
    println!("  url {base}");
    if !ok {
        println!("  the expected 404 or a route status differed");
    }
    Ok(ok)
}

fn run_offline_smoke(demo: &DemoPlan) -> Result<()> {
    println!();
    println!("offline smoke — every non-localhost request is blocked");
    let site = demo.root.join("site");
    let controller = site.join("node_modules/@playwright/test");
    if !controller.is_dir() {
        bail!(
            "{} is absent; run `cd site && npm ci` before the offline check",
            controller.display()
        );
    }
    let npx = preflight::locate(&preflight::path_dirs(), "npx")
        .context("npx is not on PATH; install Node.js before the offline check")?;
    let mut command = Command::new(npx);
    command
        .args([
            "playwright",
            "test",
            "--config",
            "playwright.offline.config.ts",
        ])
        .current_dir(&site)
        .env("PLAYWRIGHT_BROWSERS_PATH", "0")
        .env("CLIMB_OFFLINE_BASE_URL", &demo.server_url)
        .env("npm_config_offline", "true");
    let output = proc::run_raw(
        &mut command,
        Duration::from_secs(300),
        "playwright offline smoke",
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        println!("  {line}");
    }
    if output.status.success() {
        let verdict = last_line(&output.stdout).unwrap_or_else(|| "passed".to_string());
        println!("  [ok]     offline smoke   {verdict}");
        Ok(())
    } else {
        for line in String::from_utf8_lossy(&output.stderr).lines().take(20) {
            println!("  {line}");
        }
        bail!("the offline smoke failed; fix the external request or console error it named")
    }
}

fn run_tmux(demo: &DemoPlan) -> Result<()> {
    let tmux = preflight::locate(&preflight::path_dirs(), "tmux").context("tmux is not on PATH")?;
    let exe = current_exe()?;
    let session = "climb-demo";
    let _ = Command::new(&tmux)
        .args(["kill-session", "-t", session])
        .status();

    let pane = |name: &str| pane_command(&exe, demo, name);
    let create = Command::new(&tmux)
        .args([
            "new-session",
            "-d",
            "-s",
            session,
            "-x",
            "220",
            "-y",
            "50",
            "-n",
            "demo",
        ])
        .args(pane("tui"))
        .status()
        .context("starting the tmux session")?;
    if !create.success() {
        bail!("tmux could not create the demo session");
    }
    for (pane_name, target, direction) in [
        ("serve", format!("{session}:0.0"), "-h"),
        ("cheat", format!("{session}:0.1"), "-v"),
        ("verify", format!("{session}:0.0"), "-v"),
    ] {
        let status = Command::new(&tmux)
            .args(["split-window", direction, "-t", &target])
            .args(pane(pane_name))
            .status()
            .with_context(|| format!("splitting in the {pane_name} pane"))?;
        if !status.success() {
            let _ = Command::new(&tmux)
                .args(["kill-session", "-t", session])
                .status();
            bail!("tmux could not create the {pane_name} pane");
        }
    }
    let _ = Command::new(&tmux)
        .args(["select-pane", "-t", &format!("{session}:0.0")])
        .status();

    let addr = format!("127.0.0.1:{}", demo.port).parse()?;
    wait_for_server(addr, Duration::from_secs(20))?;
    if demo.offline {
        if let Err(error) = run_offline_smoke(demo) {
            let _ = Command::new(&tmux)
                .args(["kill-session", "-t", session])
                .status();
            return Err(error);
        }
    }

    println!();
    println!("tmux session `{session}` is running; close it with the TUI's q or `tmux kill-session -t {session}`");
    let status = Command::new(&tmux)
        .args(["attach-session", "-t", session])
        .status()
        .context("attaching to the tmux session")?;
    let _ = status;
    plan::print_closing(demo)?;
    Ok(())
}

fn pane_command(exe: &Path, demo: &DemoPlan, name: &str) -> Vec<String> {
    let mut command = vec![
        exe.display().to_string(),
        "demo".to_string(),
        "--pane".to_string(),
        name.to_string(),
        "--root".to_string(),
        demo.root.display().to_string(),
        "--port".to_string(),
        demo.port.to_string(),
    ];
    if demo.offline {
        command.push("--offline".to_string());
    }
    command
}

fn wait_for_server(addr: SocketAddr, limit: Duration) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < limit {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("the static server did not come up on {addr} within {limit:?}")
}

fn run_pane(args: &DemoArgs, pane: Pane) -> Result<()> {
    match pane {
        Pane::Tui => {
            let tui = resolve_tui(&args.root)?;
            let demo = plan::build(&args.root, &tui, args.port, args.offline)?;
            plan::stage_viewer(&demo)?;
            println!("dashboard — recorded data, brand theme");
            println!(
                "keys: t start tour · n next · p pause · Esc leave tour · 1-9 tabs · T theme · ? help · q quit"
            );
            println!();
            let mut command = Command::new(&tui);
            command
                .args(["--results"])
                .arg(&demo.staged_results)
                .args(["--theme", "brand", "--replay"]);
            exec(command)
        }
        Pane::Verify => {
            let demo = plan::build(
                &args.root,
                &resolve_tui(&args.root)?,
                args.port,
                args.offline,
            )?;
            let exe = current_exe()?;
            let ok = run_verification(&demo, &exe)?;
            if demo.offline {
                run_offline_smoke(&demo)?;
            }
            wait_for_enter();
            if !ok {
                bail!("a verification command failed");
            }
            Ok(())
        }
        Pane::Serve => {
            let addr: SocketAddr = format!("127.0.0.1:{}", args.port)
                .parse()
                .context("the demo port is not a valid loopback address")?;
            let running = server::start(&args.root.join(preflight::SITE_DIST), addr)?;
            println!(
                "static server: {} serving {}",
                running.url(),
                args.root.join(preflight::SITE_DIST).display()
            );
            loop {
                thread::sleep(Duration::from_secs(3600));
            }
        }
        Pane::Cheat => {
            let demo = plan::build(
                &args.root,
                &resolve_tui(&args.root)?,
                args.port,
                args.offline,
            )?;
            plan::print_cheat_sheet(&demo);
            plan::print_closing(&demo)?;
            wait_for_enter();
            Ok(())
        }
    }
}

fn resolve_tui(root: &Path) -> Result<PathBuf> {
    preflight::locate_tui(root).with_context(|| {
        format!(
            "no climb-tui binary at {} or {}, and none on PATH",
            preflight::TUI_RELEASE,
            preflight::TUI_DEBUG
        )
    })
}

fn exec(mut command: Command) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        bail!("failed to run climb-tui: {error}");
    }
    #[cfg(not(unix))]
    {
        let status = command.status().context("running climb-tui")?;
        if !status.success() {
            bail!("climb-tui exited with {status}");
        }
        Ok(())
    }
}

fn wait_for_enter() {
    print!("\npress Enter to close this pane ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plan_is_reviewable_without_a_terminal() {
        // The dry run is the part of the demo that can be asserted without a
        // projector, a tmux server or a browser.
        let demo = DemoPlan {
            root: PathBuf::from("."),
            results: PathBuf::from(preflight::SAMPLE_RESULTS),
            staged_results: PathBuf::from("target/demo/climb-results.json"),
            tui: PathBuf::from(preflight::TUI_RELEASE),
            verification: Vec::new(),
            server_root: PathBuf::from(preflight::SITE_DIST),
            server_url: format!("http://127.0.0.1:{DEMO_PORT}"),
            port: DEMO_PORT,
            cheat_sheet: Vec::new(),
            rtt_pair: None,
            offline: true,
        };
        assert_eq!(demo.server_url, "http://127.0.0.1:8080");
        assert!(demo.offline);
    }

    #[test]
    fn a_pane_command_carries_the_root_port_and_offline_flag() {
        let demo = DemoPlan {
            root: PathBuf::from("/workspace"),
            results: PathBuf::from("docs/sample-output/climb-results.json"),
            staged_results: PathBuf::from("target/demo/climb-results.json"),
            tui: PathBuf::from("target/release/climb-tui"),
            verification: Vec::new(),
            server_root: PathBuf::from("site/dist"),
            server_url: "http://127.0.0.1:8080".to_string(),
            port: 8080,
            cheat_sheet: Vec::new(),
            rtt_pair: None,
            offline: true,
        };
        let command = pane_command(Path::new("/bin/xtask"), &demo, "serve");
        assert_eq!(command[0], "/bin/xtask");
        assert!(command.windows(2).any(|pair| pair == ["--pane", "serve"]));
        assert!(command.windows(2).any(|pair| pair == ["--port", "8080"]));
        assert!(command
            .windows(2)
            .any(|pair| pair == ["--root", "/workspace"]));
        assert!(command.iter().any(|arg| arg == "--offline"));
    }

    #[test]
    fn the_last_non_empty_line_is_the_verdict() {
        assert_eq!(last_line(b"first\nsecond\n\n").as_deref(), Some("second"));
        assert_eq!(last_line(b"\n\n"), None);
    }

    #[test]
    fn the_plan_output_names_every_pane() {
        let demo = DemoPlan {
            root: PathBuf::from("."),
            results: PathBuf::from(preflight::SAMPLE_RESULTS),
            staged_results: PathBuf::from("target/demo/climb-results.json"),
            tui: PathBuf::from(preflight::TUI_RELEASE),
            verification: vec![plan::VerifyStep {
                name: "sample-check",
                args: vec!["sample-check".to_string()],
            }],
            server_root: PathBuf::from(preflight::SITE_DIST),
            server_url: "http://127.0.0.1:8080".to_string(),
            port: 8080,
            cheat_sheet: Vec::new(),
            rtt_pair: None,
            offline: false,
        };
        let readings = preflight::Readings::default();
        // The printer writes to stdout; the assertion is that it does not
        // panic on a plan with an empty cheat sheet.
        print_plan(&demo, &readings);
    }
}
