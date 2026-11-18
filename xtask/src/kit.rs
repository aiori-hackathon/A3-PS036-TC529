//! The `kit` subcommand: pack the travel kit that demonstrates CLIMB without a
//! build and without a network.
//!
//! ```text
//! cargo xtask kit
//! cargo xtask kit --skip-site-build
//! cargo xtask kit --skip-docker
//! cargo xtask kit --with-docker
//! cargo xtask kit --out /media/usb/climb-kit
//! ```
//!
//! The kit is one directory holding the built site with a loopback-only server,
//! the release viewer dashboard, the recorded fixtures, the self-checked report
//! bundle (with its own `SHA256SUMS` and `PROVENANCE.txt`), the deck PDF, the
//! pitch, the judge Q&A, the fallback guide, this packing guide and a printable
//! restore page. Every file is digested into one manifest, and the kit's
//! `verify.sh` re-checks that manifest — and the report bundle's own manifest —
//! from wherever the drive is mounted.
//!
//! Packing is strict about its inputs and conservative about its output. A
//! missing input is a hard error naming the path; the new kit is staged beside
//! the old one and swapped only after it is complete; and a directory that does
//! not carry the kit's own markers is never cleared. Docker images are optional:
//! they are packed when they are present, their compressed sizes are printed,
//! and `--skip-docker`/`--with-docker` make that choice explicit.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::package::{capture, render_sums, sha256_hex};
use crate::proc;

/// The default kit directory.
const KIT_DIR: &str = "dist/kit";
/// The built site, copied to `site/dist` inside the kit.
const SITE_DIST: &str = "site/dist";
/// The committed report bundle: results, report, charts, manifest, provenance.
const SAMPLE_DIR: &str = "docs/sample-output";
/// The submission deck at the repository root.
const DECK: &str = "A3-PS036-TC529.pdf";
/// The dashboard binary's name, on disk and inside the kit.
const BINARY: &str = "climb-tui";
/// Recorded fixtures, with the filename they keep inside the kit.
const FIXTURES: [&str; 2] = [
    "data/dns-capture.recorded.json",
    "data/ladder-verification.recorded.json",
];
/// Documents the kit ships, as `(source, name in the kit)`.
const DOCS: [(&str, &str); 5] = [
    ("docs/PITCH.md", "PITCH.md"),
    ("docs/QA.md", "QA.md"),
    ("docs/DEMO-FALLBACKS.md", "DEMO-FALLBACKS.md"),
    ("docs/EVIDENCE-SHEET.html", "EVIDENCE-SHEET.html"),
    ("docs/KIT.md", "KIT.md"),
];
/// The live-pipeline images, packed when they exist locally.
const DOCKER_IMAGES: [&str; 4] = [
    "climb-nsd:phase12",
    "climb-unbound:phase13",
    "climb-dig:phase14",
    "climb-ldns-signer:phase14",
];
/// The kit manifest filename; excluded from its own digest list.
const MANIFEST: &str = "SHA256SUMS";
/// The directory name a staged kit is built under before it is swapped in.
const STAGING: &str = ".kit.staging";

/// Arguments for `cargo xtask kit`.
#[derive(Args)]
pub(crate) struct KitArgs {
    /// Directory the kit is written into (created when absent).
    #[arg(long, default_value = KIT_DIR)]
    pub(crate) out: PathBuf,

    /// Reuse an existing `site/dist` instead of running the site build.
    #[arg(long)]
    pub(crate) skip_site_build: bool,

    /// Leave the Docker images out even when they are present locally.
    #[arg(long, conflicts_with = "with_docker")]
    pub(crate) skip_docker: bool,

    /// Require every live-pipeline Docker image instead of skipping a missing one.
    #[arg(long)]
    pub(crate) with_docker: bool,

    /// Accept a site whose `/tui` window has no browser module. The terminal
    /// dashboard is still packed; the browser window shows its build state.
    #[arg(long)]
    pub(crate) skip_browser_dashboard: bool,

    /// Skip the committed-sample and generated-site-data checks.
    #[arg(long)]
    pub(crate) skip_checks: bool,
}

/// What to do about the optional Docker images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DockerMode {
    /// Pack none, even when they exist.
    Skip,
    /// Pack the ones that exist and print a note for the rest.
    Auto,
    /// Pack every one; a missing image is an error.
    Required,
}

/// Decide the Docker policy from the two flags. clap already refuses both at
/// once, so this also guards the decision when it is called directly.
pub(crate) fn docker_mode(skip: bool, required: bool) -> Result<DockerMode> {
    match (skip, required) {
        (true, true) => bail!("--skip-docker and --with-docker cannot be combined"),
        (true, false) => Ok(DockerMode::Skip),
        (false, true) => Ok(DockerMode::Required),
        (false, false) => Ok(DockerMode::Auto),
    }
}

/// Whether `dir` looks like a kit this command produced, and may therefore be
/// replaced. A directory without either marker is never cleared.
pub(crate) fn is_previous_kit(dir: &Path) -> bool {
    dir.join("README.txt").is_file() || dir.join("verify.sh").is_file()
}

/// Format a byte count for a human. Binary units, two decimals above one
/// kibibyte, and an exact byte count below it.
pub(crate) fn human_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let value = bytes as f64;
    if value < KIB {
        format!("{bytes} B")
    } else if value < MIB {
        format!("{:.1} KiB", value / KIB)
    } else if value < GIB {
        format!("{:.2} MiB", value / MIB)
    } else {
        format!("{:.2} GiB", value / GIB)
    }
}

/// The target directory, honouring `CARGO_TARGET_DIR`.
fn target_dir() -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from("target"),
    }
}

/// Every regular file under `root`, as a path relative to it, sorted. Symlinks
/// and special files are refused: a kit that links outside itself cannot be
/// verified once the drive is moved.
pub(crate) fn walk_all(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    walk(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = fs::read_dir(dir)
        .with_context(|| format!("failed to read the directory {}", dir.display()))?;
    for entry in entries {
        let entry =
            entry.with_context(|| format!("failed to read an entry under {}", dir.display()))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        if metadata.file_type().is_symlink() {
            bail!(
                "{} is a symbolic link; a kit must contain the bytes it verifies",
                path.display()
            );
        }
        if metadata.is_dir() {
            walk(root, &path, out)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .with_context(|| format!("{} is outside {}", path.display(), root.display()))?;
            out.push(relative.to_path_buf());
        } else {
            bail!("{} is not a regular file or directory", path.display());
        }
    }
    Ok(())
}

/// The files the manifest describes: every regular file except the manifest
/// itself. The report bundle keeps its own nested `SHA256SUMS`, which is part
/// of the payload and therefore digested like any other file.
pub(crate) fn manifest_files(root: &Path) -> Result<Vec<PathBuf>> {
    Ok(walk_all(root)?
        .into_iter()
        .filter(|relative| relative != Path::new(MANIFEST))
        .collect())
}

/// Render a relative path with forward slashes, the form the manifest uses.
fn manifest_name(relative: &Path) -> String {
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Write `SHA256SUMS` over every payload file and return the file count and the
/// number of payload bytes hashed.
pub(crate) fn write_manifest(root: &Path) -> Result<(usize, u64)> {
    let files = manifest_files(root)?;
    let mut sums = BTreeMap::new();
    let mut total = 0_u64;
    for relative in &files {
        let path = root.join(relative);
        let bytes = fs::read(&path)
            .with_context(|| format!("failed to read {} for the manifest", path.display()))?;
        total += bytes.len() as u64;
        sums.insert(manifest_name(relative), sha256_hex(&bytes));
    }
    fs::write(root.join(MANIFEST), render_sums(&sums))
        .with_context(|| format!("failed to write {}/{MANIFEST}", root.display()))?;
    Ok((files.len(), total))
}

/// Total size of every file under `dir`, or zero when it does not exist.
pub(crate) fn tree_size(dir: &Path) -> Result<u64> {
    if !dir.exists() {
        return Ok(0);
    }
    let mut total = 0_u64;
    for relative in walk_all(dir)? {
        let path = dir.join(relative);
        total += fs::metadata(&path)
            .with_context(|| format!("failed to measure {}", path.display()))?
            .len();
    }
    Ok(total)
}

/// One `(label, directory)` pair for the printable size table.
const SIZE_COMPONENTS: [(&str, &str); 7] = [
    ("site/dist", "site"),
    ("bin", "bin"),
    ("data", "data"),
    ("report", "report"),
    ("deck", "deck"),
    ("docs", "docs"),
    ("docker", "docker"),
];

/// Measure each present component. A component that was not packed is omitted,
/// so a kit without Docker images does not claim an empty directory.
pub(crate) fn component_sizes(root: &Path) -> Result<Vec<(String, u64)>> {
    let mut sizes = Vec::new();
    for (label, dir) in SIZE_COMPONENTS {
        let path = root.join(dir);
        if path.is_dir() {
            sizes.push((label.to_string(), tree_size(&path)?));
        }
    }
    Ok(sizes)
}

/// Render the printable one-page restore guide. `sizes` is measured from the
/// kit that was just packed, so the page never carries a remembered number.
pub(crate) fn render_readme(sizes: &[(String, u64)]) -> String {
    let mut table = String::new();
    let mut payload = 0_u64;
    for (label, bytes) in sizes {
        payload += *bytes;
        table.push_str(&format!("  {label:<12} {:>10}\n", human_bytes(*bytes)));
    }
    table.push_str(&format!(
        "  {:<12} {:>10}\n",
        "payload",
        human_bytes(payload)
    ));
    format!(
        r#"CLIMB - travel kit
==================

Everything needed to demonstrate CLIMB offline is on this drive: the built
site, the release dashboard, the recorded evidence and the printed material.

Restore (about a minute)
------------------------
  1. Open a terminal in this directory (the one holding this file).
  2. Run sh verify.sh
     It prints only problems. "every file matches" means the drive is intact;
     any failure means: use the second drive, then rebuild the kit.
  3. Run sh serve.sh
     It prints a loopback address such as http://127.0.0.1:8080/
     Open it for the site, /tui/ for the browser dashboard, /report/ for the
     report. Pass a port to move it: sh serve.sh 8081
  4. For the terminal dashboard, copy the binary somewhere with Unix
     permissions and run it from there:
       cp bin/climb-tui /tmp/climb-tui && chmod +x /tmp/climb-tui
       /tmp/climb-tui
     (A FAT or exFAT drive cannot carry the executable bit.)
     Keys 1-9 switch tabs, t starts the guided tour, ? opens help, q quits.
  5. If a browser is unavailable, open report/report.html directly. It is one
     self-contained file.

Nothing here contacts a network service. The server binds the loopback
address only, the dashboard reads files on this drive, and the report embeds
its own charts and evidence.

What is here
------------
  site/dist/   the built static site, served by serve.sh
  bin/         the release dashboard binary (viewer-only; no crypto stack)
  data/        the recorded DNS capture and ladder-verification fixture
  report/      results, the self-checked HTML report, every chart, plus the
               bundle's own SHA256SUMS and PROVENANCE.txt
  deck/        the submission deck PDF
  docs/        PITCH.md, QA.md, DEMO-FALLBACKS.md, EVIDENCE-SHEET.html and KIT.md
  docker/      optional compressed live-pipeline images, when packed

Measured payload sizes
----------------------
{table}
The whole kit's size on disk is printed by ./verify.sh, which measures the
files it has just checked.

If something fails
------------------
Follow docs/DEMO-FALLBACKS.md first, then the printed deck. Do not improvise a
number: every claim in the report carries its source pointer, and the kit can
be rebuilt from the source checkout with `cargo xtask kit`.
"#
    )
}

mod scripts;

pub(crate) use scripts::{render_serve_script, render_verify_script};

/// Copy a tree of regular files, refusing anything that is not one.
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    if !from.is_dir() {
        bail!(
            "{} is not a directory — build it first, or pass --skip-site-build \
             only when it already exists",
            from.display()
        );
    }
    fs::create_dir_all(to).with_context(|| format!("failed to create {}", to.display()))?;
    for relative in walk_all(from)? {
        let source = from.join(&relative);
        let destination = to.join(&relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::copy(&source, &destination).with_context(|| {
            format!(
                "failed to copy {} to {}",
                source.display(),
                destination.display()
            )
        })?;
    }
    Ok(())
}

/// Copy a single file into `dir`, creating it.
fn copy_file(source: &Path, dir: &Path, name: &str) -> Result<()> {
    if !source.is_file() {
        bail!(
            "{} is missing — the kit cannot be packed without it",
            source.display()
        );
    }
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let destination = dir.join(name);
    fs::copy(source, &destination).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

/// Write a generated script with its executable bit set.
fn write_script(path: &Path, body: &str) -> Result<()> {
    fs::write(path, body).with_context(|| format!("failed to write {}", path.display()))?;
    set_executable(path)
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to mark {} executable", path.display()))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// Build the viewer-only release dashboard.
fn build_dashboard() -> Result<()> {
    println!("kit: building the viewer dashboard (release)");
    let mut command = Command::new("cargo");
    command.args([
        "build",
        "-p",
        "climb-tui",
        "--release",
        "--no-default-features",
        "--features",
        "terminal",
    ]);
    proc::run(
        &mut command,
        Duration::from_secs(3600),
        "cargo build -p climb-tui",
    )
    .context("the release dashboard build failed")?;
    let built = target_dir().join("release").join(BINARY);
    if !built.is_file() {
        bail!("{} was not produced by the release build", built.display());
    }
    Ok(())
}

/// Build the site with its own toolchain.
fn build_site() -> Result<()> {
    if !Path::new("site/node_modules").is_dir() {
        bail!(
            "site dependencies are not installed — run `cd site && npm ci` once, \
             or pass --skip-site-build with an existing {SITE_DIST}"
        );
    }
    println!("kit: building the site (npm run build in site/)");
    let mut command = Command::new("npm");
    command.args(["--prefix", "site", "run", "build"]);
    proc::run(&mut command, Duration::from_secs(1800), "npm run build")
        .context("the site build failed")?;
    Ok(())
}

/// File-name form of a Docker image reference.
pub(crate) fn image_slug(image: &str) -> String {
    image
        .chars()
        .map(|character| match character {
            ':' | '/' => '-',
            other => other,
        })
        .collect()
}

/// Whether `dir` holds a built browser module (at least one `.wasm` file).
pub(crate) fn browser_module_present(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("wasm"))
    })
}

/// Whether the local Docker daemon has `image`.
fn image_present(image: &str) -> bool {
    capture(
        "docker",
        &["image", "inspect", "--format", "{{.Id}}", image],
    )
    .is_some()
}

/// Save one image as a gzip-compressed tar and return its compressed size.
fn save_image(image: &str, dir: &Path) -> Result<u64> {
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let tar = dir.join(format!("{}.tar", image_slug(image)));
    let archive = dir.join(format!("{}.tar.gz", image_slug(image)));
    let mut command = Command::new("docker");
    command.args(["save", "--output"]).arg(&tar).arg(image);
    proc::run(&mut command, Duration::from_secs(1800), "docker save")
        .with_context(|| format!("failed to save the image {image}"))?;
    let mut gzip = Command::new("gzip");
    gzip.args(["-9", "-f"]).arg(&tar);
    if let Err(error) = proc::run(&mut gzip, Duration::from_secs(1800), "gzip") {
        let _ = fs::remove_file(&tar);
        return Err(error).with_context(|| format!("failed to compress the image {image}"));
    }
    fs::metadata(&archive)
        .map(|meta| meta.len())
        .with_context(|| format!("{} was not written", archive.display()))
}

/// Pack the optional images and return their `(image, compressed bytes)` sizes.
fn pack_images(root: &Path, mode: DockerMode) -> Result<Vec<(String, u64)>> {
    if mode == DockerMode::Skip {
        println!("kit: docker images skipped (--skip-docker)");
        return Ok(Vec::new());
    }
    let dir = root.join("docker");
    let mut packed = Vec::new();
    for image in DOCKER_IMAGES {
        if !image_present(image) {
            if mode == DockerMode::Required {
                bail!(
                    "{image} is not present locally — build the live pipeline with \
                     `docker compose -f docker/compose.dns-pipeline.yml build`, then rerun"
                );
            }
            println!(
                "kit: {image} is not present locally; skipped (pass --with-docker to require it)"
            );
            continue;
        }
        let size = save_image(image, &dir)?;
        println!(
            "kit: packed {image} as docker/{}.tar.gz ({})",
            image_slug(image),
            human_bytes(size)
        );
        packed.push((image.to_string(), size));
    }
    if packed.is_empty() && dir.is_dir() {
        let _ = fs::remove_dir(&dir);
    }
    Ok(packed)
}

/// What the packing checks do about the generated site data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SiteDataPolicy {
    /// A host fingerprint is present, so the committed data must re-render
    /// identically.
    Verify,
    /// A clean checkout has no fingerprint; ship the committed data as recorded.
    ShipAsRecorded,
}

/// Decide the site-data policy from whether `environment.json` is present.
pub(crate) fn site_data_policy(environment_present: bool) -> SiteDataPolicy {
    if environment_present {
        SiteDataPolicy::Verify
    } else {
        SiteDataPolicy::ShipAsRecorded
    }
}

/// Check the committed evidence before anything is copied.
fn check_evidence() -> Result<()> {
    println!("kit: checking the committed report pair and the generated site data");
    let sample = crate::sample_check::SampleCheckArgs {
        root: PathBuf::from("."),
        results: PathBuf::from(crate::sample_check::DEFAULT_RESULTS),
        report: PathBuf::from(crate::sample_check::DEFAULT_REPORT),
        manifest: PathBuf::from(crate::sample_check::DEFAULT_MANIFEST),
        provenance: PathBuf::from(crate::sample_check::DEFAULT_PROVENANCE),
        pair_only: false,
        require_commit: false,
    };
    crate::sample_check::run(&sample).context("the committed sample pair failed its check")?;
    // A clean checkout has no host fingerprint: `environment.json` is not
    // committed, because it describes the machine that recorded the evidence.
    // The generated site data is committed and shipped as recorded; only an
    // absent input is treated as a note, while a present one must be current.
    match site_data_policy(Path::new("environment.json").is_file()) {
        SiteDataPolicy::Verify => {
            crate::site_data::cmd_site_check(crate::site_data::SiteArgs {
                root: PathBuf::from("."),
                out: PathBuf::from(climb_report::site_data::DEFAULT_OUT_DIR),
            })
            .context("the generated site data is not current")?;
        }
        SiteDataPolicy::ShipAsRecorded => {
            println!(
                "kit: note: environment.json is absent (a clean checkout has no host \
                 fingerprint); shipping the committed site data as recorded"
            );
        }
    }
    Ok(())
}

/// Strip the staged dashboard, falling back to the unstripped copy when no
/// working `strip` is available.
fn strip_binary(path: &Path, original: &Path) -> Result<()> {
    let mut command = Command::new("strip");
    command.arg(path);
    match proc::run(&mut command, Duration::from_secs(120), "strip") {
        Ok(_) => Ok(()),
        Err(error) => {
            eprintln!("kit: note: the dashboard was not stripped ({error:#})");
            fs::copy(original, path)
                .with_context(|| format!("failed to restore {}", path.display()))?;
            set_executable(path)
        }
    }
}

/// Pack the travel kit.
pub(crate) fn cmd_kit(args: KitArgs) -> Result<()> {
    let mode = docker_mode(args.skip_docker, args.with_docker)?;
    let out = args.out;
    if out.file_name().is_none() {
        bail!("--out must name a directory, not {}", out.display());
    }

    if out.exists() && !is_previous_kit(&out) {
        bail!(
            "refusing to clear {}: it does not carry the kit's README.txt or verify.sh, \
             and only a previous kit may be replaced",
            out.display()
        );
    }

    if !args.skip_checks {
        check_evidence()?;
    }

    if args.skip_site_build {
        if !Path::new(SITE_DIST).is_dir() {
            bail!("{SITE_DIST} is missing — run `cd site && npm ci && npm run build`, or drop --skip-site-build");
        }
        println!("kit: reusing {SITE_DIST}");
    } else {
        if !args.skip_browser_dashboard
            && !browser_module_present(Path::new("crates/climb-web/dist"))
        {
            bail!(
                "crates/climb-web/dist has no browser module — run \
                 `cd crates/climb-web && trunk build --release` first (wasm32 target and \
                 wasm-opt required), or pass --skip-browser-dashboard"
            );
        }
        build_site()?;
    }

    build_dashboard()?;

    if args.skip_browser_dashboard {
        println!("kit: the browser dashboard is optional (--skip-browser-dashboard)");
    } else if !browser_module_present(&Path::new(SITE_DIST).join("tui")) {
        bail!(
            "the browser dashboard is missing from {SITE_DIST}/tui — run \
             `cd crates/climb-web && trunk build --release` (wasm32 target and wasm-opt \
             required) before the site build, or pass --skip-browser-dashboard"
        );
    }

    let parent = match out.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    fs::create_dir_all(&parent)
        .with_context(|| format!("failed to create {}", parent.display()))?;
    let staging = parent.join(STAGING);
    if staging.exists() {
        fs::remove_dir_all(&staging)
            .with_context(|| format!("failed to clear {}", staging.display()))?;
    }
    let root = staging.join("kit");
    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;

    let result = pack_payload(&root, mode);
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    let (files, hashed) = write_manifest(&root)?;

    if out.exists() {
        fs::remove_dir_all(&out).with_context(|| format!("failed to replace {}", out.display()))?;
    }
    fs::rename(&root, &out)
        .with_context(|| format!("failed to move the finished kit into {}", out.display()))?;
    fs::remove_dir_all(&staging)
        .with_context(|| format!("failed to clear {}", staging.display()))?;

    let sizes = component_sizes(&out)?;
    let total = tree_size(&out)?;
    println!(
        "kit: {} ({files} file(s) hashed, {hashed} payload B)",
        out.display()
    );
    for (label, bytes) in &sizes {
        println!("  {label:<12} {:>10}", human_bytes(*bytes));
    }
    println!("  {:<12} {:>10}", "whole kit", human_bytes(total));
    println!(
        "kit: next: {}/verify.sh, then {}/serve.sh",
        out.display(),
        out.display()
    );
    Ok(())
}

/// Copy every required payload file, write the scripts and the restore page.
fn pack_payload(root: &Path, mode: DockerMode) -> Result<()> {
    println!("kit: copying the built site");
    copy_tree(Path::new(SITE_DIST), &root.join("site").join("dist"))?;

    println!("kit: copying the report bundle");
    copy_tree(Path::new(SAMPLE_DIR), &root.join("report"))?;

    for fixture in FIXTURES {
        let source = Path::new(fixture);
        let name = fixture.rsplit('/').next().unwrap_or(fixture);
        copy_file(source, &root.join("data"), name)?;
    }

    for (source, name) in DOCS {
        copy_file(Path::new(source), &root.join("docs"), name)?;
    }

    copy_file(Path::new(DECK), &root.join("deck"), "climb-deck.pdf")?;

    println!("kit: staging the release dashboard");
    let built = target_dir().join("release").join(BINARY);
    let bin_dir = root.join("bin");
    copy_file(&built, &bin_dir, BINARY)?;
    let staged = bin_dir.join(BINARY);
    set_executable(&staged)?;
    strip_binary(&staged, &built)?;

    let packed_images = pack_images(root, mode)?;
    if !packed_images.is_empty() {
        let total: u64 = packed_images.iter().map(|(_, size)| *size).sum();
        println!(
            "kit: {} docker image(s), {} compressed",
            packed_images.len(),
            human_bytes(total)
        );
    }

    let sizes = component_sizes(root)?;
    let readme = render_readme(&sizes);
    fs::write(root.join("README.txt"), readme)
        .with_context(|| format!("failed to write {}/README.txt", root.display()))?;
    write_script(&root.join("verify.sh"), render_verify_script())?;
    write_script(&root.join("serve.sh"), render_serve_script())?;
    Ok(())
}

#[cfg(test)]
mod tests;
