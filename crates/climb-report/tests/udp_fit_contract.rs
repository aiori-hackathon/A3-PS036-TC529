//! Contract test for the analytical UDP-fit section of the consolidated
//! results artifact.
//!
//! `docs/sample-output/climb-results.json` is the committed copy of the real
//! consolidated artifact. This test pushes it through every consumer that
//! lives in this crate — deserialization, chart generation and the HTML
//! report (including `report --check`) — so a change to the artifact shape or
//! to the UDP-fit section fails here rather than at run time. The remaining
//! consumer, the TUI, is covered by `climb-tui`'s
//! `committed_sample_results_expose_udp_fit_rows_to_the_tui`.

use std::path::{Path, PathBuf};

use climb_report::charts::CHART_FILES;
use climb_report::html::{build_claims, render_html};
use climb_report::results::EvidenceClass;
use climb_report::{check_report, ClimbResults};

fn sample_results_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/sample-output/climb-results.json")
}

fn load_sample() -> ClimbResults {
    let path = sample_results_path();
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read the committed sample artifact {path:?}: {e}"));
    serde_json::from_str(&source).expect("the committed sample artifact parses")
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "climb-udp-fit-contract-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The committed artifact carries a full UDP-fit grid whose band counts
/// partition each row's messages exactly, whose percentages match those
/// counts, and whose capture cross-check is recorded at zero tolerance. When
/// the model inputs are available, every row is recomputed from the
/// analytical model and must reproduce the artifact byte for byte.
#[test]
fn committed_udp_fit_section_is_self_consistent() {
    let results = load_sample();
    let section = results
        .derived
        .udp_fit
        .as_ref()
        .expect("the committed artifact must exercise the UDP-fit section");
    assert!(!section.rows.is_empty());

    let mut profiles: Vec<&str> = Vec::new();
    let mut configs: Vec<&str> = Vec::new();
    let mut sizes: Vec<u64> = Vec::new();
    for row in &section.rows {
        if !profiles.contains(&row.profile.as_str()) {
            profiles.push(&row.profile);
        }
        if !configs.contains(&row.config.as_str()) {
            configs.push(&row.config);
        }
        if !sizes.contains(&row.messages) {
            sizes.push(row.messages);
        }

        // The three bands partition the row's messages exactly, and each
        // recorded percentage is recomputed from its own count.
        let sum = row
            .fit_1232_count
            .checked_add(row.tc_band_count)
            .and_then(|total| total.checked_add(row.over_4096_count))
            .expect("band counts must not overflow");
        assert_eq!(sum, row.messages, "bands must partition the messages");
        let msgs = row.messages as f64;
        assert!((row.fit_1232_percent - row.fit_1232_count as f64 / msgs * 100.0).abs() < 1e-9);
        assert!((row.tc_band_percent - row.tc_band_count as f64 / msgs * 100.0).abs() < 1e-9);
        assert!((row.needs_tcp_percent - row.over_4096_count as f64 / msgs * 100.0).abs() < 1e-9);
        assert_eq!(row.class, EvidenceClass::Analytical);
        assert!(row.max_response_bytes >= 1);
        // The modelled delay is only ever present with an RTT assumption.
        if row.tcp_modeled_ms.is_some() {
            assert!(section.rtt_ms.is_some(), "a delay implies an RTT input");
        }
    }
    sizes.sort_unstable();
    assert!(configs.contains(&"plain") && configs.contains(&"mtl-full"));
    assert!(configs.contains(&"mtl-amortised"));
    // The classical context baselines share the same grid.
    assert!(
        configs.contains(&"classical-ed25519")
            && configs.contains(&"classical-ecdsa-p256")
            && configs.contains(&"classical-rsa2048"),
        "classical context configs must be modelled: {configs:?}"
    );
    assert!(profiles.len() >= 2, "profiles={profiles:?}");
    assert!(sizes.len() >= 2, "sizes={sizes:?}");

    if let Some(v) = section.validation.as_ref() {
        assert_eq!(v.tolerance_bytes, 0, "tolerance is fixed at zero bytes");
        assert_eq!(
            v.matches,
            v.recorded_full_response_bytes == v.modeled_full_response_bytes
                && v.recorded_condensed_response_bytes == v.modeled_condensed_response_bytes,
            "the recorded verdict must follow from the recorded lengths"
        );
    }

    // Recompute every row from the analytical model when the model inputs
    // are all present: the artifact must be the model's own output.
    let Some(v) = section.validation.as_ref() else {
        return;
    };
    let Ok(plain) = usize::try_from(v.plain_signature_len_bytes) else {
        return;
    };
    // The signature length fed to the model for each configuration: A/B/C
    // share the plain SLH-DSA length, each classical config its own recorded
    // length (read from the artifact's classical rows, never typed here).
    let classical_len = |algorithm: &str| -> usize {
        results
            .classical
            .iter()
            .find(|c| c.algorithm == algorithm)
            .map(|c| c.signature_len_bytes)
            .unwrap_or_else(|| panic!("the artifact must carry {algorithm} rows"))
    };
    for row in &section.rows {
        let profile = climb_analytical::builtin_profiles()
            .into_iter()
            .find(|p| p.name == row.profile)
            .unwrap_or_else(|| panic!("profile {} missing from the model data", row.profile));
        let config = match row.config.as_str() {
            "plain" => climb_analytical::SignatureConfig::Plain,
            "mtl-full" => climb_analytical::SignatureConfig::MtlFull,
            "mtl-amortised" => climb_analytical::SignatureConfig::MtlAmortised,
            "classical-ed25519" => climb_analytical::SignatureConfig::ClassicalEd25519,
            "classical-ecdsa-p256" => climb_analytical::SignatureConfig::ClassicalEcdsaP256,
            "classical-rsa2048" => climb_analytical::SignatureConfig::ClassicalRsa2048,
            other => panic!("unknown signature configuration {other:?}"),
        };
        let length = match row.config.as_str() {
            "classical-ed25519" => classical_len("Ed25519"),
            "classical-ecdsa-p256" => classical_len("ECDSA P-256"),
            "classical-rsa2048" => classical_len("RSA-2048"),
            _ => plain,
        };
        let zone = row.zone_leaves;
        let messages = row.messages;
        let modelled = climb_analytical::udp_fit_for_size(config, &profile, zone, messages, length)
            .unwrap_or_else(|e| {
                panic!(
                    "model failed for {} / {} at {} messages: {e}",
                    row.profile, row.config, row.messages
                )
            });
        assert_eq!(
            modelled.max_bytes as u64, row.max_response_bytes,
            "max response for {} / {} at {} messages",
            row.profile, row.config, row.messages
        );
        assert_eq!(modelled.fit_1232_count, row.fit_1232_count);
        assert_eq!(modelled.tc_band_count, row.tc_band_count);
        assert_eq!(modelled.over_4096_count, row.over_4096_count);
    }
}

/// Every consumer of the artifact — chart generation and the HTML report
/// including `report --check` — accepts the committed artifact and renders
/// the UDP-fit section and its chart.
#[test]
fn committed_udp_fit_section_renders_through_charts_and_report() {
    let results = load_sample();
    let dir = temp_dir("render");
    let charts = dir.join("charts");

    let paths = climb_report::charts::generate_charts(&results, &charts).expect("charts");
    assert_eq!(paths.len(), CHART_FILES.len());
    assert_eq!(
        paths.last().and_then(|p| p.file_name()),
        Some(std::ffi::OsStr::new("udp_fit.svg")),
        "the UDP-fit chart must be the last chart written"
    );
    let fit_svg = charts.join("udp_fit.svg");
    let svg = std::fs::read_to_string(&fit_svg).expect("read udp-fit svg");
    assert!(svg.contains("<svg") && svg.contains("</svg>"));
    assert!(
        svg.contains("Modelled DNS response size over UDP"),
        "the UDP-fit chart must carry its caption"
    );

    let html = render_html(&results, &charts, "../climb-results.json").expect("render report");
    assert!(
        html.contains("DNS over UDP: fit and TCP fallback"),
        "the report must render the UDP-fit section"
    );
    assert!(
        html.contains("Signature context: classical beside post-quantum"),
        "the report must render the signature-context section"
    );

    // Every UDP-fit claim's rendered string must appear verbatim in the
    // body, which is exactly what `report --check` re-validates.
    let claims = build_claims(&results).expect("claims");
    assert!(
        claims.iter().any(|c| c.id.starts_with("udpfit-")),
        "the registry must carry UDP-fit claims"
    );
    for claim in claims.iter().filter(|c| c.id.starts_with("udpfit-")) {
        assert!(
            html.contains(&claim.formatted)
                || html.contains(&claim.formatted.replace('&', "&amp;")),
            "claim {} rendered string missing: {}",
            claim.id,
            claim.formatted
        );
    }
    assert!(
        claims.iter().any(|c| c.id.starts_with("context-")),
        "the registry must carry signature-context claims"
    );
    for claim in claims.iter().filter(|c| c.id.starts_with("context-")) {
        assert!(
            html.contains(&claim.formatted)
                || html.contains(&claim.formatted.replace('&', "&amp;")),
            "claim {} rendered string missing: {}",
            claim.id,
            claim.formatted
        );
    }
    assert_eq!(check_report(&html, &results), Vec::<String>::new());

    let _ = std::fs::remove_dir_all(&dir);
}

/// The committed artifact's signature-context section frames the post-quantum
/// numbers with the classical baselines: six rows at the headline dataset
/// size, the classical ones first, each signature size matching the
/// artifact's own classical rows and each fits cell read from the modelled
/// UDP-fit grid.
#[test]
fn committed_signature_context_matches_the_udp_fit_model() {
    let results = load_sample();
    let section = &results.derived.signature_context;
    assert!(
        !section.is_empty(),
        "the committed artifact must carry classical context rows"
    );
    assert_eq!(
        section.len(),
        6,
        "three classical baselines plus configurations A, B and C"
    );
    let configs: Vec<&str> = section.iter().map(|r| r.config.as_str()).collect();
    assert_eq!(
        configs,
        [
            "classical-ed25519",
            "classical-ecdsa-p256",
            "classical-rsa2048",
            "plain",
            "mtl-full",
            "mtl-amortised",
        ],
        "classical baselines first, then A, B, C"
    );
    assert!(section.iter().all(|r| r.size == "Medium"));
    assert!(section[..3]
        .iter()
        .all(|r| r.family == "classical (context)"));
    assert!(section[3..].iter().all(|r| r.family == "post-quantum"));

    // The classical signature sizes are the lengths read from the produced
    // signatures, recorded in the artifact's classical rows.
    for (row, algorithm) in section[..3]
        .iter()
        .zip(["Ed25519", "ECDSA P-256", "RSA-2048"])
    {
        let recorded = results
            .classical
            .iter()
            .find(|c| c.algorithm == algorithm)
            .unwrap_or_else(|| panic!("the artifact must carry {algorithm} rows"));
        assert_eq!(
            row.signature_bytes,
            Some(recorded.signature_len_bytes as f64),
            "{algorithm} context size must match its classical row"
        );
    }

    // The fits cell is the modelled UDP-fit row for the same configuration
    // at the headline message count, under the headline profile.
    let udp = results
        .derived
        .udp_fit
        .as_ref()
        .expect("the committed artifact must carry the UDP-fit section");
    for row in section {
        let modelled = udp
            .rows
            .iter()
            .find(|r| r.profile == "positive_a" && r.config == row.config && r.messages == 10_000)
            .unwrap_or_else(|| panic!("no modelled row for {} at Medium", row.config));
        assert_eq!(
            row.fits_1232_percent,
            Some(modelled.fit_1232_percent),
            "fits cell for {} must come from the modelled row",
            row.config
        );
    }
}
