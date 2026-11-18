//! Unit tests for the travel-kit packer and the scripts it emits.

use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

fn temp_root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("climb-kit-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp root");
    dir
}

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent");
    }
    fs::write(path, body).expect("write");
}

fn run_script(script: &Path, cwd: &Path) -> std::process::Output {
    Command::new("sh")
        .arg(script)
        .current_dir(cwd)
        .output()
        .expect("run the generated script")
}

#[test]
fn the_kit_layout_carries_every_promised_component() {
    assert_eq!(FIXTURES.len(), 2);
    assert_eq!(DOCS.len(), 5);
    assert_eq!(DOCKER_IMAGES.len(), 4);
    let readme = render_readme(&[("site/dist".to_string(), 2048)]);
    for needle in [
        "site/dist/",
        "bin/",
        "data/",
        "report/",
        "deck/",
        "docs/",
        "docker/",
        "sh verify.sh",
        "sh serve.sh",
        "bin/climb-tui",
        "DEMO-FALLBACKS.md",
    ] {
        assert!(readme.contains(needle), "restore page is missing {needle}");
    }
    for (_, name) in DOCS {
        assert!(readme.contains(name), "restore page does not name {name}");
    }
}

#[test]
fn the_verify_script_checks_the_kit_and_the_report_bundle() {
    let script = render_verify_script();
    assert!(script.contains("SHA256SUMS"));
    assert!(script.contains("report/SHA256SUMS"));
    assert!(script.contains("sha256sum --check --strict --quiet"));
    assert!(script.contains("shasum -a 256 --check"));
    assert!(script.contains("cmp -s"));
    assert!(script.contains("whole kit is"));
    assert!(script.starts_with("#!/bin/sh"));
}

#[test]
fn the_verify_script_passes_a_clean_tree_and_fails_a_corrupt_one() {
    let root = temp_root("verify");
    write(&root.join("site/dist/index.html"), "<h1>home</h1>");
    write(&root.join("report/report.html"), "<html>report</html>");
    write(
        &root.join("report/SHA256SUMS"),
        &render_sums(&BTreeMap::from([(
            "report.html".to_string(),
            sha256_hex(b"<html>report</html>"),
        )])),
    );
    write(&root.join("README.txt"), "restore page");
    write_script(&root.join("verify.sh"), render_verify_script()).expect("verify script");
    write_manifest(&root).expect("manifest");

    let clean = run_script(&root.join("verify.sh"), &root);
    assert!(
        clean.status.success(),
        "clean tree failed: {}{}",
        String::from_utf8_lossy(&clean.stdout),
        String::from_utf8_lossy(&clean.stderr)
    );
    assert!(String::from_utf8_lossy(&clean.stdout).contains("every file matches"));

    write(&root.join("site/dist/index.html"), "<h1>tampered</h1>");
    let corrupt = run_script(&root.join("verify.sh"), &root);
    assert!(!corrupt.status.success(), "a tampered file must fail");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&corrupt.stdout),
        String::from_utf8_lossy(&corrupt.stderr)
    );
    assert!(
        text.contains("FAILED"),
        "the failure should name the file: {text}"
    );

    // Replace the tampered file and add an unlisted one instead.
    write(&root.join("site/dist/index.html"), "<h1>home</h1>");
    write(&root.join("site/dist/extra.html"), "<p>unlisted</p>");
    let extra = run_script(&root.join("verify.sh"), &root);
    assert!(!extra.status.success(), "an unlisted file must fail");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&extra.stdout),
        String::from_utf8_lossy(&extra.stderr)
    );
    assert!(
        text.contains("extra.html"),
        "the extra file should be named: {text}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_verify_script_checks_the_report_bundle_against_its_own_manifest() {
    let root = temp_root("report-verify");
    write(&root.join("README.txt"), "restore page");
    write(&root.join("report/report.html"), "<html>report</html>");
    write(
        &root.join("report/SHA256SUMS"),
        &render_sums(&BTreeMap::from([(
            "report.html".to_string(),
            sha256_hex(b"<html>stale</html>"),
        )])),
    );
    write_script(&root.join("verify.sh"), render_verify_script()).expect("verify script");
    write_manifest(&root).expect("manifest");

    let output = run_script(&root.join("verify.sh"), &root);
    assert!(
        !output.status.success(),
        "a stale report manifest must fail"
    );
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(text.contains("report bundle"), "{text}");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_serve_script_serves_loopback_only_with_the_deployment_404() {
    let root = temp_root("serve");
    write(&root.join("site/dist/index.html"), "home page");
    write(&root.join("site/dist/404.html"), "custom not found");
    write_script(&root.join("serve.sh"), render_serve_script()).expect("serve script");

    let mut child = Command::new("sh")
        .arg(root.join("serve.sh"))
        .arg("0")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("start the server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut line = String::new();
        let _ = stdout.read_line(&mut line);
        let _ = sender.send(line);
    });
    let first = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("the server prints its address");
    assert!(
        first.contains("http://127.0.0.1:"),
        "the server must bind loopback only, printed: {first}"
    );
    let port: u16 = first
        .trim()
        .split("http://127.0.0.1:")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .and_then(|value| value.parse().ok())
        .expect("parse the printed port");

    let get = |path: &str| -> (u16, String) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(mut stream) => {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(10)))
                        .expect("timeout");
                    write!(stream, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n")
                        .expect("request");
                    let mut response = String::new();
                    stream.read_to_string(&mut response).expect("response");
                    let status = response
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .and_then(|code| code.parse().ok())
                        .expect("status");
                    return (status, response);
                }
                Err(error) if Instant::now() < deadline => {
                    let _ = error;
                    thread::sleep(Duration::from_millis(50));
                }
                Err(error) => panic!("connect to the server: {error}"),
            }
        }
    };

    let (status, response) = get("/");
    assert_eq!(status, 200);
    assert!(response.contains("home page"));

    let (status, response) = get("/missing");
    assert_eq!(status, 404);
    assert!(
        response.contains("custom not found"),
        "the deployment 404 page should be served: {response}"
    );

    let (status, _) = get("/site/");
    assert_eq!(
        status, 404,
        "a directory without an index is a 404, not a listing"
    );

    child.kill().expect("stop the server");
    let _ = child.wait();
    let _ = child.stderr.take();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_restore_page_is_one_printable_page() {
    let sizes = vec![
        ("site/dist".to_string(), 1024 * 1024),
        ("bin".to_string(), 512 * 1024),
    ];
    let readme = render_readme(&sizes);
    let lines = readme.lines().count();
    assert!(
        lines <= 72,
        "the restore page is {lines} lines, not one page"
    );
    assert!(readme.contains("Measured payload sizes"));
    assert!(readme.contains("1.00 MiB"));
    assert!(readme.contains("512.0 KiB"));
}

#[test]
fn docker_images_are_optional_or_strict_on_request() {
    assert_eq!(docker_mode(true, false).expect("skip"), DockerMode::Skip);
    assert_eq!(
        docker_mode(false, true).expect("require"),
        DockerMode::Required
    );
    assert_eq!(docker_mode(false, false).expect("auto"), DockerMode::Auto);
    assert!(docker_mode(true, true).is_err());
    assert_eq!(image_slug("climb-nsd:phase12"), "climb-nsd-phase12");
    assert_eq!(
        image_slug("registry.example/nsd:1"),
        "registry.example-nsd-1"
    );
}

#[test]
fn human_sizes_are_exact_below_one_kibibyte() {
    assert_eq!(human_bytes(0), "0 B");
    assert_eq!(human_bytes(512), "512 B");
    assert_eq!(human_bytes(1023), "1023 B");
    assert_eq!(human_bytes(1024), "1.0 KiB");
    assert_eq!(human_bytes(1536), "1.5 KiB");
    assert_eq!(human_bytes(1024 * 1024), "1.00 MiB");
    assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.00 GiB");
}

#[test]
fn the_browser_dashboard_is_required_unless_it_is_skipped() {
    let root = temp_root("browser");
    assert!(!browser_module_present(&root.join("tui")));
    write(&root.join("tui/index.html"), "window");
    assert!(!browser_module_present(&root.join("tui")));
    write(&root.join("tui/module.wasm"), "\0asm");
    assert!(browser_module_present(&root.join("tui")));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_clean_checkout_ships_the_committed_site_data_as_recorded() {
    assert_eq!(site_data_policy(true), SiteDataPolicy::Verify);
    assert_eq!(site_data_policy(false), SiteDataPolicy::ShipAsRecorded);
}

#[test]
fn a_directory_that_is_not_a_previous_kit_is_never_cleared() {
    let root = temp_root("guard");
    assert!(!is_previous_kit(&root));
    write(&root.join("README.txt"), "restore page");
    assert!(is_previous_kit(&root));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_manifest_is_sorted_relative_and_excludes_itself() {
    let root = temp_root("manifest");
    write(&root.join("b/two.txt"), "two");
    write(&root.join("a/one.txt"), "one");
    write(&root.join("report/SHA256SUMS"), "nested manifest");
    write(&root.join("SHA256SUMS"), "stale manifest");
    let (count, bytes) = write_manifest(&root).expect("manifest");
    assert_eq!(
        count, 3,
        "the root manifest is excluded, the nested one is not"
    );
    assert_eq!(bytes, 3 + 3 + "nested manifest".len() as u64);
    let text = fs::read_to_string(root.join("SHA256SUMS")).expect("manifest text");
    let names: Vec<&str> = text
        .lines()
        .map(|line| line.split_once("  ").expect("two spaces").1)
        .collect();
    assert_eq!(names, vec!["a/one.txt", "b/two.txt", "report/SHA256SUMS"]);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn component_sizes_measure_only_the_components_that_are_present() {
    let root = temp_root("sizes");
    write(&root.join("site/dist/index.html"), "12345");
    write(&root.join("bin/climb-tui"), "123");
    let sizes = component_sizes(&root).expect("sizes");
    assert_eq!(
        sizes,
        vec![("site/dist".to_string(), 5), ("bin".to_string(), 3),]
    );
    assert_eq!(tree_size(&root).expect("tree size"), 8);
    let _ = fs::remove_dir_all(root);
}
